use creature_context_types::orbit::OrbitRequest;
use std::io::{self, Read};

pub fn read_stdin_request() -> Result<OrbitRequest, io::Error> {
    let mut buffer = Vec::new();
    // Read at most 16 MiB from stdin
    io::stdin()
        .take(16 * 1024 * 1024)
        .read_to_end(&mut buffer)?;

    // Check for trailing non-whitespace
    let text =
        std::str::from_utf8(&buffer).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let request: OrbitRequest = serde_json::from_str(text).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid JSON request: {}", e),
        )
    })?;

    Ok(request)
}
