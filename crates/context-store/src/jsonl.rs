//! Append-only JSONL journal with applied markers.
//!
//! This is the portable `.creature/journal.jsonl` from specification 4.2, not
//! the SQLite index. It is one of the files that travels with a project and
//! that a rebuild can read, so it must survive the two failures a resident
//! process actually suffers:
//!
//! - **A crash mid-write** leaves a truncated final line. That line is not a
//!   record; it is the absence of one. Replaying it as though it parsed would
//!   fabricate an event that never happened.
//! - **A restart** must not reprocess work already applied. Applied markers are
//!   recorded separately and append-only, so replay resumes rather than repeats.
//!
//! Corruption in the middle of the file is a different matter from a truncated
//! tail, and is reported rather than skipped: a crash cannot damage a line that
//! was already followed by others.

use serde::{Serialize, de::DeserializeOwned};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum JsonlError {
    #[error("cannot access journal {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("journal {path} is corrupt at line {line}: {message}")]
    Corrupt {
        path: String,
        line: usize,
        message: String,
    },
}

/// An entry a journal can address, so applied markers can refer to it.
pub trait JournalEntry {
    fn entry_id(&self) -> Uuid;
}

/// What `read_all` observed, so a caller can report a truncated tail rather
/// than silently discarding it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JournalFinding {
    TruncatedTail { bytes: usize },
}

pub struct JsonlJournal<T> {
    path: PathBuf,
    applied_path: PathBuf,
    entry: PhantomData<T>,
}

impl<T: Serialize + DeserializeOwned + JournalEntry> JsonlJournal<T> {
    /// Open (creating if absent) the journal at `path`. Applied markers live in
    /// a sibling file so the journal itself stays a pure record of events.
    pub fn open(path: &Path) -> Result<Self, JsonlError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| JsonlError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
        if !path.exists() {
            File::create(path).map_err(|source| JsonlError::Io {
                path: path.display().to_string(),
                source,
            })?;
        }
        Ok(Self {
            path: path.to_path_buf(),
            applied_path: path.with_extension("applied"),
            entry: PhantomData,
        })
    }

    /// Append one entry and flush it to disk before returning.
    ///
    /// The runtime appends before processing, so an event that was observed is
    /// durable even if the process dies while handling it.
    pub fn append(&mut self, entry: &T) -> Result<(), JsonlError> {
        let mut line = serde_json::to_string(entry).map_err(|e| JsonlError::Corrupt {
            path: self.path.display().to_string(),
            line: 0,
            message: e.to_string(),
        })?;
        line.push('\n');
        Self::append_line(&self.path, line.as_bytes())
    }

    fn append_line(path: &Path, bytes: &[u8]) -> Result<(), JsonlError> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|source| JsonlError::Io {
                path: path.display().to_string(),
                source,
            })?;
        file.write_all(bytes).map_err(|source| JsonlError::Io {
            path: path.display().to_string(),
            source,
        })?;
        file.sync_all().map_err(|source| JsonlError::Io {
            path: path.display().to_string(),
            source,
        })
    }

    /// Every entry in the journal, with any truncated final line reported
    /// rather than parsed.
    pub fn read_all_with_findings(&self) -> Result<(Vec<T>, Vec<JournalFinding>), JsonlError> {
        let file = File::open(&self.path).map_err(|source| JsonlError::Io {
            path: self.path.display().to_string(),
            source,
        })?;

        let raw: Vec<String> = BufReader::new(file)
            .lines()
            .collect::<Result<_, _>>()
            .map_err(|source| JsonlError::Io {
                path: self.path.display().to_string(),
                source,
            })?;

        // A file that does not end in a newline has an incomplete final line.
        let ends_cleanly = std::fs::read(&self.path)
            .map_err(|source| JsonlError::Io {
                path: self.path.display().to_string(),
                source,
            })?
            .last()
            .is_none_or(|b| *b == b'\n');

        let mut entries = Vec::new();
        let mut findings = Vec::new();
        let last_index = raw.len().saturating_sub(1);

        for (index, line) in raw.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<T>(line) {
                Ok(entry) => entries.push(entry),
                Err(message) => {
                    // Only the final line may be truncated by a crash. Anything
                    // earlier was complete when the next line was written, so a
                    // parse failure there is corruption, not truncation.
                    if index == last_index && !ends_cleanly {
                        findings.push(JournalFinding::TruncatedTail { bytes: line.len() });
                    } else {
                        return Err(JsonlError::Corrupt {
                            path: self.path.display().to_string(),
                            line: index + 1,
                            message: message.to_string(),
                        });
                    }
                }
            }
        }
        Ok((entries, findings))
    }

    pub fn read_all(&self) -> Result<Vec<T>, JsonlError> {
        Ok(self.read_all_with_findings()?.0)
    }

    /// Record that an entry has been processed. Append-only, so history is
    /// never rewritten.
    pub fn mark_applied(&mut self, id: Uuid) -> Result<(), JsonlError> {
        Self::append_line(&self.applied_path, format!("{id}\n").as_bytes())
    }

    pub fn applied_ids(&self) -> Result<BTreeSet<Uuid>, JsonlError> {
        if !self.applied_path.exists() {
            return Ok(BTreeSet::new());
        }
        let contents =
            std::fs::read_to_string(&self.applied_path).map_err(|source| JsonlError::Io {
                path: self.applied_path.display().to_string(),
                source,
            })?;
        Ok(contents
            .lines()
            .filter_map(|l| Uuid::parse_str(l.trim()).ok())
            .collect())
    }

    /// Entries not yet marked applied — what a restart must process, and
    /// nothing more.
    pub fn pending(&self) -> Result<Vec<T>, JsonlError> {
        let applied = self.applied_ids()?;
        Ok(self
            .read_all()?
            .into_iter()
            .filter(|entry| !applied.contains(&entry.entry_id()))
            .collect())
    }
}
