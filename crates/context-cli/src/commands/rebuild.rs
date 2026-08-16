use creature_context_store::rebuild_repository_from_portable;
use std::path::PathBuf;

/// Reconstruct the disposable database from the portable root `ATLAS.idx`.
///
/// Reports what was restored rather than only exiting zero: a rebuild that
/// silently restores a partial snapshot is the failure this command exists to
/// make impossible.
pub fn handle_rebuild(
    project_dir: PathBuf,
    database_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let database = database_path.unwrap_or_else(|| project_dir.join(".creature/atlas.db"));

    let report = rebuild_repository_from_portable(&project_dir, &database)?;

    println!(
        "rebuilt {} from {}: snapshot {}, {} entities, {} edges, {} records",
        database.display(),
        project_dir.join("ATLAS.idx").display(),
        report.snapshot_id,
        report.entities,
        report.edges,
        report.records,
    );
    if report.opaque_records > 0 {
        println!(
            "  {} forward-compatible record(s) preserved but not understood by this build",
            report.opaque_records
        );
    }

    Ok(())
}
