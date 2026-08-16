//! launchd adapter: the resident service as a per-user LaunchAgent.
//!
//! A LaunchAgent rather than a LaunchDaemon deliberately. The service indexes a
//! user's repository, writes into it, and on macOS projects Green onto Finder
//! tags — all of which belong to a logged-in user's session, not to root. A
//! LaunchAgent needs no privilege escalation and can touch the user's files;
//! a LaunchDaemon would need root and could not write the user's Finder metadata.
//!
//! `KeepAlive` restarts the daemon if it exits, and `RunAtLoad` starts it at
//! login — together that is what makes the background lane constant rather than
//! "constant while a terminal is open".

use super::{DaemonError, DaemonStatus, ServiceDefinition, label_for_roots, log_path};
use creature_context_types::model::CapabilityState;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Verified: the plist this produces is validated by `plutil -lint`, and install
/// / status / uninstall are exercised against the real `launchctl` on macOS.
pub fn capability() -> CapabilityState {
    CapabilityState::Verified
}

/// `~/Library/LaunchAgents`, where a per-user agent belongs.
fn agents_dir() -> Result<PathBuf, DaemonError> {
    let home = std::env::var("HOME")
        .map_err(|_| DaemonError::Supervisor("HOME is not set".to_string()))?;
    Ok(PathBuf::from(home).join("Library").join("LaunchAgents"))
}

// Declared directly rather than by pulling the `libc` crate, matching how
// `metadata::macos` reaches the xattr calls.
unsafe extern "C" {
    fn getuid() -> u32;
}

/// The GUI domain for the current user, which is where a LaunchAgent is booted.
fn gui_domain() -> String {
    // `getuid` has no failure mode and no error state — it cannot fail.
    format!("gui/{}", unsafe { getuid() })
}

/// Escape the five characters XML forbids in element text, so a repository path
/// containing `&` or `<` produces a valid plist rather than a corrupt one.
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The LaunchAgent property list for `root`, as data. Pure — writes nothing.
pub fn definition(roots: &[PathBuf], binary: &Path) -> Result<ServiceDefinition, DaemonError> {
    let label = label_for_roots(roots)?;
    let mut canonical_roots = Vec::new();
    for root in roots {
        canonical_roots.push(root.canonicalize()?);
    }
    let primary = canonical_roots
        .first()
        .cloned()
        .ok_or_else(|| DaemonError::Supervisor("no project roots given".into()))?;
    // The daemon logs into the first root: a supervised process has no terminal,
    // and one log for the process is clearer than one per root it happens to watch.
    let log = log_path(&primary);
    let root_arguments = canonical_roots
        .iter()
        .map(|root| {
            format!(
                "        <string>{}</string>",
                xml_escape(&root.to_string_lossy())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let contents = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary}</string>
        <string>run</string>
{root_arguments}
    </array>
    <key>WorkingDirectory</key>
    <string>{root}</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardOutPath</key>
    <string>{log}</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        label = xml_escape(&label),
        binary = xml_escape(&binary.to_string_lossy()),
        root_arguments = root_arguments,
        root = xml_escape(&primary.to_string_lossy()),
        log = xml_escape(&log.to_string_lossy()),
    );

    Ok(ServiceDefinition {
        unit_path: agents_dir()?.join(format!("{label}.plist")),
        label,
        contents,
    })
}

/// Write the agent and boot it. Any existing registration is booted out first so
/// installing twice replaces rather than conflicts.
pub fn install(roots: &[PathBuf]) -> Result<ServiceDefinition, DaemonError> {
    let definition = definition(roots, &super::current_binary()?)?;

    // The log's directory must exist before launchd opens it for writing, or the
    // job fails to spawn with a permissions error that looks like a bug.
    let canonical = roots
        .first()
        .ok_or_else(|| DaemonError::Supervisor("no project roots given".into()))?
        .canonicalize()?;
    if let Some(parent) = log_path(&canonical).parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = definition.unit_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&definition.unit_path, &definition.contents)?;

    // Replace any previous registration. A not-loaded job makes this fail, which
    // is expected and ignored — the desired end state is "loaded once".
    let _ = Command::new("launchctl")
        .args(["bootout", &format!("{}/{}", gui_domain(), definition.label)])
        .output();

    let output = Command::new("launchctl")
        .args(["bootstrap", &gui_domain()])
        .arg(&definition.unit_path)
        .output()
        .map_err(|error| DaemonError::Supervisor(error.to_string()))?;
    if !output.status.success() {
        return Err(DaemonError::Supervisor(format!(
            "launchctl bootstrap failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(definition)
}

/// Boot the job out and delete its plist. Succeeds when nothing was installed.
///
/// `launchctl bootout` returns as soon as it has signalled the job, not when the
/// process has gone — the daemon stops at a loop boundary, so it outlives the
/// call by a moment. Returning there would report "uninstalled" while the process
/// was still running and still writing to the project, so this waits for launchd
/// to actually forget the job before removing the plist.
///
/// Exit status 3 is launchd's "no such process", which is the desired end state
/// rather than a failure: uninstalling something that was never installed, or
/// booting out twice, both succeed. Any other failure is surfaced instead of
/// being swallowed.
pub fn uninstall(roots: &[PathBuf]) -> Result<(), DaemonError> {
    let label = label_for_roots(roots)?;
    let service = format!("{}/{}", gui_domain(), label);

    let output = Command::new("launchctl")
        .args(["bootout", &service])
        .output()
        .map_err(|error| DaemonError::Supervisor(error.to_string()))?;
    if !output.status.success() && output.status.code() != Some(3) {
        return Err(DaemonError::Supervisor(format!(
            "launchctl bootout failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    // Wait for launchd to stop reporting the job. Bounded: a daemon wedged in a
    // long reconcile should not hang the caller forever, and the plist removal
    // below still deregisters it for the next login.
    for _ in 0..50 {
        if !job_is_loaded(&service) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    let path = agents_dir()?.join(format!("{label}.plist"));
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

/// Whether launchd currently holds this job. Measured by asking launchd, not
/// inferred from whether a file exists.
fn job_is_loaded(service: &str) -> bool {
    Command::new("launchctl")
        .args(["print", service])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Ask launchd whether the job is loaded, and the filesystem whether it is
/// installed. Both are measured, neither assumed.
pub fn status(roots: &[PathBuf]) -> Result<DaemonStatus, DaemonError> {
    let label = label_for_roots(roots)?;
    let installed = agents_dir()?.join(format!("{label}.plist")).exists();
    let loaded = job_is_loaded(&format!("{}/{}", gui_domain(), label));
    Ok(DaemonStatus {
        label,
        installed,
        loaded,
    })
}
