use std::{fs, io, path::Path};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PurposeDocument {
    pub title: Option<String>,
    pub goals: Vec<String>,
    pub priorities: Vec<String>,
    pub constraints: Vec<String>,
    pub principles: Vec<String>,
    pub protected_decisions: Vec<String>,
    pub raw: String,
}

pub fn read_purpose(root: &Path) -> io::Result<Option<PurposeDocument>> {
    let path = root.join("PURPOSE.md");
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    Ok(Some(parse_purpose(&raw)))
}

pub fn parse_purpose(raw: &str) -> PurposeDocument {
    let mut document = PurposeDocument {
        raw: raw.to_owned(),
        ..PurposeDocument::default()
    };
    let mut section = String::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(header) = trimmed.strip_prefix('#') {
            let header = header.trim();
            if document.title.is_none() {
                document.title = Some(header.to_owned());
            }
            section = header.to_lowercase();
            continue;
        }
        let item = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .map(str::trim);
        let Some(item) = item.filter(|value| !value.is_empty()) else {
            continue;
        };
        if section.contains("protected") || section.contains("decision") {
            document.protected_decisions.push(item.to_owned());
        } else if section.contains("priorit") {
            document.priorities.push(item.to_owned());
        } else if section.contains("constraint") {
            document.constraints.push(item.to_owned());
        } else if section.contains("principle") {
            document.principles.push(item.to_owned());
        } else {
            document.goals.push(item.to_owned());
        }
    }
    if document.goals.is_empty() {
        document.goals = raw
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .take(3)
            .map(str::to_owned)
            .collect();
    }
    document
}
