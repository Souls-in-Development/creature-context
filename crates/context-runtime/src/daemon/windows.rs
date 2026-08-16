//! Task Scheduler adapter: the resident service as a per-user logon task.
//!
//! A Scheduled Task rather than a Service Control Manager service, matching the
//! macOS choice of a LaunchAgent over a LaunchDaemon and the Linux choice of a
//! `--user` unit, and for the same reason: the service indexes a user's
//! repository and writes into it, so it belongs to that user's session. A
//! Scheduled Task registered under the current user needs no elevation; creating
//! an SCM service requires Administrator and would run as SYSTEM, in the wrong
//! session, with no access to the user's own files.
//!
//! `LogonTrigger` starts it at logon and `RestartOnFailure` brings it back if it
//! exits — together the Windows equivalent of `RunAtLoad` + `KeepAlive`. The
//! execution-time limit is disabled (`PT0S`) because a resident daemon is
//! supposed to run indefinitely; the Task Scheduler default would otherwise stop
//! it after three days.
//!
//! Honesty boundary: this is written against the documented Task Scheduler
//! schema but has **not been compiled or run** — the development host is macOS.
//! It reports `ImplementedUnverified`, never `Verified`; only running
//! install/status/uninstall against a real `schtasks` on Windows earns that
//! (spec §16). The definition is pure data and is unit-tested here, exactly as
//! the systemd unit is.

use super::{DaemonError, DaemonStatus, ServiceDefinition, label_for_roots, log_path};
use creature_context_types::model::CapabilityState;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Implemented but never executed on a Windows host by this project. Not
/// `Verified` — no run, no claim.
pub fn capability() -> CapabilityState {
    CapabilityState::ImplementedUnverified
}

/// Where the task definition is staged before `schtasks` registers it. The task
/// itself lives in the Task Scheduler's own store, not on disk, so this is a
/// staging file rather than the unit's home.
fn definition_dir(primary: &Path) -> PathBuf {
    primary.join(".creature")
}

/// Escape the five characters XML forbids in element text, so a repository path
/// containing `&` produces a valid task definition rather than a corrupt one.
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Quote one command-line argument for the `Arguments` element. Windows splits a
/// command line inside the receiving process, so a root containing a space must
/// arrive as one argument rather than two.
fn quote_argument(value: &str) -> String {
    if value.contains(' ') {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

/// The Task Scheduler definition for `roots`, as data. Pure — writes nothing.
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

    let arguments = std::iter::once("run".to_string())
        .chain(
            canonical_roots
                .iter()
                .map(|root| quote_argument(&root.to_string_lossy())),
        )
        .collect::<Vec<_>>()
        .join(" ");

    let contents = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Creature Context resident service ({label})</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>3</Count>
    </RestartOnFailure>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{binary}</Command>
      <Arguments>{arguments}</Arguments>
      <WorkingDirectory>{working}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#,
        label = xml_escape(&label),
        binary = xml_escape(&binary.to_string_lossy()),
        arguments = xml_escape(&arguments),
        working = xml_escape(&primary.to_string_lossy()),
    );

    Ok(ServiceDefinition {
        unit_path: definition_dir(&primary).join(format!("{label}.task.xml")),
        label,
        contents,
    })
}

/// Stage the task definition and register it with the Task Scheduler.
///
/// `/F` replaces an existing task of the same name, so installing twice replaces
/// rather than conflicts — the same guarantee the launchd and systemd adapters
/// give. `/RU` is not passed: the task registers under the invoking user, which
/// is the whole point of choosing a task over an SCM service.
pub fn install(roots: &[PathBuf]) -> Result<ServiceDefinition, DaemonError> {
    let definition = definition(roots, &super::current_binary()?)?;
    let primary = roots
        .first()
        .ok_or_else(|| DaemonError::Supervisor("no project roots given".into()))?
        .canonicalize()?;

    if let Some(parent) = log_path(&primary).parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = definition.unit_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // The Task Scheduler schema declares UTF-16; schtasks rejects a definition
    // whose encoding does not match its declaration.
    std::fs::write(
        &definition.unit_path,
        utf16le_with_bom(&definition.contents),
    )?;

    run_schtasks(&[
        "/Create",
        "/TN",
        &definition.label,
        "/XML",
        &definition.unit_path.to_string_lossy(),
        "/F",
    ])?;
    // A logon trigger fires at the next logon; start it now so installing has the
    // immediate effect the other platforms' RunAtLoad/--now gives.
    run_schtasks(&["/Run", "/TN", &definition.label])?;
    Ok(definition)
}

/// Stop the task and delete its registration, then remove the staged definition.
/// Deleting a task that does not exist succeeds — the desired end state is what
/// matters, matching the launchd and systemd adapters.
pub fn uninstall(roots: &[PathBuf]) -> Result<(), DaemonError> {
    let label = label_for_roots(roots)?;
    let _ = run_schtasks(&["/End", "/TN", &label]);
    let _ = run_schtasks(&["/Delete", "/TN", &label, "/F"]);
    if let Some(primary) = roots.first() {
        if let Ok(canonical) = primary.canonicalize() {
            let staged = definition_dir(&canonical).join(format!("{label}.task.xml"));
            if staged.exists() {
                std::fs::remove_file(staged)?;
            }
        }
    }
    Ok(())
}

/// Ask the Task Scheduler whether the task exists and whether it is running.
/// Both measured, neither assumed.
pub fn status(roots: &[PathBuf]) -> Result<DaemonStatus, DaemonError> {
    let label = label_for_roots(roots)?;
    let query = Command::new("schtasks")
        .args(["/Query", "/TN", &label, "/FO", "LIST"])
        .output();

    let (installed, loaded) = match query {
        Ok(output) if output.status.success() => {
            let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
            // "Running" is the Task Scheduler's own word for a live instance;
            // "Ready" means registered but not currently executing.
            (true, text.contains("running"))
        }
        _ => (false, false),
    };
    Ok(DaemonStatus {
        label,
        installed,
        loaded,
    })
}

/// Encode as UTF-16LE with a byte-order mark, which is what the `<?xml ...
/// encoding="UTF-16"?>` declaration promises and what `schtasks /XML` expects.
fn utf16le_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn run_schtasks(args: &[&str]) -> Result<(), DaemonError> {
    let output = Command::new("schtasks")
        .args(args)
        .output()
        .map_err(|error| DaemonError::Supervisor(error.to_string()))?;
    if !output.status.success() {
        return Err(DaemonError::Supervisor(format!(
            "schtasks {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
