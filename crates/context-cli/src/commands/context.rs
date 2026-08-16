use crate::{
    output::{OutputFormat, write_output},
    request::read_stdin_request,
};
use creature_context_core::orbit::compile_orbit;
use creature_context_store::AtlasRepository;
use std::path::PathBuf;

pub fn handle_context(
    project_dir: PathBuf,
    request_source: Option<String>,
    format: OutputFormat,
    compatibility: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let repo = AtlasRepository::open(&project_dir.join(".creature/atlas.db"))?;
    let snapshot = repo.load_snapshot()?;

    let request = if request_source.as_deref() == Some("-") {
        read_stdin_request()?
    } else {
        return Err("Only stdin requests are supported right now".into());
    };

    let packet = compile_orbit(&snapshot, &request)?;
    write_output(&packet, format, compatibility)?;

    Ok(())
}
