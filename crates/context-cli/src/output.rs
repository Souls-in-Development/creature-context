use clap::ValueEnum;
use creature_context_store::idx::IdxRenderable;
use serde::Serialize;
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, ValueEnum, Default, PartialEq)]
#[clap(rename_all = "lowercase")]
pub enum OutputFormat {
    #[default]
    Idx,
    Json,
    Markdown,
    Yaml,
}

pub trait CliIdxRenderable {
    fn to_idx(&self) -> Result<String, io::Error>;
}

impl CliIdxRenderable for creature_context_types::orbit::OrbitPacket {
    fn to_idx(&self) -> Result<String, io::Error> {
        self.render_idx().map_err(io::Error::other)
    }
}

pub fn write_output<T: Serialize + CliIdxRenderable>(
    value: &T,
    format: OutputFormat,
    compatibility: bool,
) -> Result<(), io::Error> {
    match format {
        OutputFormat::Idx => {
            let idx_str = value.to_idx()?;
            io::stdout().write_all(idx_str.as_bytes())?;
        }
        OutputFormat::Json => {
            let json_str = serde_json::to_string_pretty(value)?;
            io::stdout().write_all(json_str.as_bytes())?;
        }
        OutputFormat::Markdown => {
            io::stdout().write_all(b"# Creature Context Markdown Output\n")?;
        }
        OutputFormat::Yaml => {
            if !compatibility {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "YAML output requires --compatibility flag",
                ));
            }
            let yaml_str = serde_yaml::to_string(value).map_err(io::Error::other)?;
            io::stdout().write_all(yaml_str.as_bytes())?;
        }
    }
    Ok(())
}

pub fn write_output_generic<T: Serialize>(
    value: &T,
    format: OutputFormat,
    compatibility: bool,
) -> Result<(), io::Error> {
    match format {
        OutputFormat::Idx => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "IDX output is not available for this command; use JSON, YAML or Markdown",
            ));
        }
        OutputFormat::Json => {
            let json_str = serde_json::to_string_pretty(value)?;
            io::stdout().write_all(json_str.as_bytes())?;
        }
        OutputFormat::Markdown => {
            io::stdout().write_all(b"# Creature Context Markdown Output\n")?;
        }
        OutputFormat::Yaml => {
            if !compatibility {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "YAML output requires --compatibility flag",
                ));
            }
            let yaml_str = serde_yaml::to_string(value).map_err(io::Error::other)?;
            io::stdout().write_all(yaml_str.as_bytes())?;
        }
    }
    Ok(())
}
