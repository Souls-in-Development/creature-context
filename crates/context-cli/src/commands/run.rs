use creature_context_runtime::service::run_service_multi;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Start the resident service and run until a termination signal arrives.
///
/// The signal handler flips a shared flag the service loop checks each cycle,
/// so SIGINT/SIGTERM stops the loop cleanly at a cycle boundary rather than
/// tearing it down mid-reconciliation.
pub fn handle_run(project_dirs: Vec<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let shutdown = Arc::new(AtomicBool::new(false));

    let signal_flag = shutdown.clone();
    ctrlc::set_handler(move || {
        signal_flag.store(true, Ordering::Relaxed);
    })?;

    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(run_service_multi(&project_dirs, shutdown))?;

    Ok(())
}
