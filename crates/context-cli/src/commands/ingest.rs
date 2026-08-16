use creature_context_core::project::ProjectPaths;
use creature_context_runtime::events::{RuntimeEvent, RuntimeEventKind};
use creature_context_store::JsonlJournal;
use std::path::PathBuf;

/// Append a client-supplied activity event to the journal.
///
/// This is how a coding platform hands Creature Context external evidence — a
/// git commit, a build result, a diagnostic — for the resident service to
/// reconcile later. The event is `ExternalActivity` and carries its payload
/// verbatim, because the payload *is* the evidence; an event recording only
/// that something happened cannot be replayed into an Atlas update.
pub fn handle_ingest(
    project_dir: PathBuf,
    kind: String,
    message: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let paths = ProjectPaths::new(&project_dir);

    let event = RuntimeEvent::new(
        RuntimeEventKind::ExternalActivity,
        chrono::Utc::now().to_rfc3339(),
    )
    .with_payload(serde_json::json!({
        "source": kind,
        "message": message,
    }));

    let mut journal = JsonlJournal::<RuntimeEvent>::open(&paths.journal)?;
    journal.append(&event)?;

    println!("ingested {kind}: {message}");
    Ok(())
}
