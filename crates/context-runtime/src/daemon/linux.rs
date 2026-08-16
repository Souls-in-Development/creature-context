//! systemd adapter: the resident service as a per-user unit.
//!
//! A *user* unit (`systemctl --user`), matching the macOS choice of a LaunchAgent
//! over a LaunchDaemon and for the same reason: the service indexes a user's
//! repository and writes into it, so it belongs to that user's session and needs
//! no privilege escalation.
//!
//! Honesty boundary: this is written against systemd's documented unit format
//! but has not been run — the development host is macOS, which has no systemd.
//! It therefore reports `ImplementedUnverified`, never `Verified`; only running
//! install/status/uninstall against a real `systemctl` on Linux earns that
//! (spec §16). The definition itself is pure data and is unit-tested here.

use super::{DaemonError, DaemonStatus, ServiceDefinition, label_for_roots, log_path};
use creature_context_types::model::CapabilityState;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Implemented but never executed on a Linux host by this project. Not
/// `Verified` — no run, no claim.
pub fn capability() -> CapabilityState {
    CapabilityState::ImplementedUnverified
}

/// `~/.config/systemd/user`, where a per-user unit belongs.
fn units_dir() -> Result<PathBuf, DaemonError> {
    let home = std::env::var("HOME")
        .map_err(|_| DaemonError::Supervisor("HOME is not set".to_string()))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("systemd")
        .join("user"))
}

/// The unit file for `root`, as data. Pure — writes nothing.
pub fn definition(roots: &[PathBuf], binary: &Path) -> Result<ServiceDefinition, DaemonError> {
    let label = label_for_roots(roots)?;
    let mut canonical_roots = Vec::new();
    for root in roots {
        canonical_roots.push(root.canonicalize()?);
    }
    let canonical = canonical_roots
        .first()
        .cloned()
        .ok_or_else(|| DaemonError::Supervisor("no project roots given".into()))?;
    let root_arguments = canonical_roots
        .iter()
        .map(|root| format!("{}", root.display()))
        .collect::<Vec<_>>()
        .join(" ");
    let contents = format!(
        "[Unit]\n\
         Description=Creature Context resident service for {root}\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={binary} run {roots}\n\
         WorkingDirectory={root}\n\
         Restart=always\n\
         RestartSec=5\n\
         StandardOutput=append:{log}\n\
         StandardError=append:{log}\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        root = canonical.display(),
        roots = root_arguments,
        binary = binary.display(),
        log = log_path(&canonical).display(),
    );

    Ok(ServiceDefinition {
        unit_path: units_dir()?.join(format!("{label}.service")),
        label,
        contents,
    })
}

pub fn install(roots: &[PathBuf]) -> Result<ServiceDefinition, DaemonError> {
    let definition = definition(roots, &super::current_binary()?)?;
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

    let unit = format!("{}.service", definition.label);
    run_systemctl(&["daemon-reload"])?;
    run_systemctl(&["enable", "--now", &unit])?;
    Ok(definition)
}

pub fn uninstall(roots: &[PathBuf]) -> Result<(), DaemonError> {
    let label = label_for_roots(roots)?;
    let unit = format!("{label}.service");
    let _ = run_systemctl(&["disable", "--now", &unit]);
    let path = units_dir()?.join(&unit);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let _ = run_systemctl(&["daemon-reload"]);
    Ok(())
}

pub fn status(roots: &[PathBuf]) -> Result<DaemonStatus, DaemonError> {
    let label = label_for_roots(roots)?;
    let unit = format!("{label}.service");
    let installed = units_dir()?.join(&unit).exists();
    let loaded = Command::new("systemctl")
        .args(["--user", "is-active", &unit])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    Ok(DaemonStatus {
        label,
        installed,
        loaded,
    })
}

fn run_systemctl(args: &[&str]) -> Result<(), DaemonError> {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|error| DaemonError::Supervisor(error.to_string()))?;
    if !output.status.success() {
        return Err(DaemonError::Supervisor(format!(
            "systemctl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
