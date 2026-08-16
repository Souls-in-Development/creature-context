//! `creature-context service` — hand the resident daemon to the OS supervisor.
//!
//! Without this, the background lane only runs while somebody keeps a terminal
//! open, and it belongs to whichever agent or shell started it. Registered with
//! the OS it is resident across logins and owned by no session (spec §7.1).

use creature_context_runtime::daemon;
use std::path::PathBuf;

/// Install and start the daemon for `project_dir`.
pub fn handle_install(project_dirs: Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let definition = daemon::install(&project_dirs)?;
    println!(
        "installed {} watching {} root(s)",
        definition.label,
        project_dirs.len()
    );
    println!("  definition: {}", definition.unit_path.display());
    println!(
        "  log:        {}",
        daemon::log_path(&project_dirs[0].canonicalize()?).display()
    );
    let status = daemon::status(&project_dirs)?;
    println!("  running:    {}", if status.loaded { "yes" } else { "no" });
    Ok(())
}

/// Stop and deregister the daemon for `project_dir`.
pub fn handle_uninstall(project_dirs: Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    daemon::uninstall(&project_dirs)?;
    println!("uninstalled");
    Ok(())
}

/// Report whether the daemon is registered and running, and whether this
/// platform can supervise one at all.
pub fn handle_status(project_dirs: Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let status = daemon::status(&project_dirs)?;
    println!("label:      {}", status.label);
    println!("installed:  {}", status.installed);
    println!("running:    {}", status.loaded);
    println!("supervisor: {:?}", daemon::capability());
    Ok(())
}

/// Print the supervisor definition without installing anything, so it can be
/// inspected — or reviewed — before it touches the system.
pub fn handle_show(project_dirs: Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let definition = daemon::definition(&project_dirs)?;
    println!("# {}", definition.unit_path.display());
    print!("{}", definition.contents);
    Ok(())
}
