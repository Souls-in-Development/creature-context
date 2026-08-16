//! Canonical Atlas IDX decoder.
//!
//! Parses the record set emitted by `encode.rs` back into typed records.
//! Forward-compatible: unknown `@record-type` lines are retained verbatim in
//! `DecodedIdx::opaque_records` so they survive a round trip.

use super::{DecodedIdx, IdxError};
use creature_context_types::{
    AtlasEdge, AtlasEntity, AtlasSnapshot, AtlasSocket, ConflictId, ConflictRecord, ConflictState,
    EdgeId, EntityId, EntityKind, Evidence, EvidenceOutcome, FactSource, GreenAssessment,
    GreenAxis, GreenCode, ProofStrength, RecordId, RelationshipKind, RelationshipPlane, ScopeScale,
    SnapshotId, SocketDirection, SocketId, SocketShape,
    authority::AuthoritySource,
    context::{
        ContextRecord, ContextRecordType, ContextSource, PrivacyClass, RecordState, SourceKind,
    },
    socket::{
        AtlasSocket as Socket, FitBasis, FitPlane, FitProof, FitStatus, HoleReason, ProofPathState,
        SocketFit, SocketHole, SocketResolution,
    },
};
use std::collections::{BTreeMap, HashMap};

/// Tokenise a single IDX line into a record tag and its key/value fields.
///
/// Values may be bare atoms or JSON-quoted strings containing spaces and
/// escaped quotes. The first token is the record tag (`@entity`, `@edge`, ...).
/// Split off the first space-delimited token from `input`, respecting
/// JSON-quoted strings that may contain spaces.
fn split_first_token(input: &str) -> Result<(&str, &str), IdxError> {
    let mut in_quotes = false;
    let mut escaped = false;
    for (i, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' && in_quotes {
            escaped = true;
            continue;
        }
        if ch == '"' {
            in_quotes = !in_quotes;
            continue;
        }
        if ch == ' ' && !in_quotes {
            return Ok((&input[..i], input[i + 1..].trim_start()));
        }
    }
    if in_quotes {
        return Err(IdxError::Parse(
            "unterminated quote while splitting token".to_string(),
        ));
    }
    Ok((input, ""))
}

fn tokenize(line: &str) -> Result<(String, Vec<(String, String)>), IdxError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;

    for ch in line.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && in_quotes {
            escaped = true;
            current.push(ch);
            continue;
        }
        if ch == '"' {
            in_quotes = !in_quotes;
            current.push(ch);
            continue;
        }
        if ch == ' ' && !in_quotes {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    if in_quotes {
        return Err(IdxError::Parse(format!(
            "unterminated quote in line: {line}"
        )));
    }

    if tokens.is_empty() {
        return Err(IdxError::Parse("empty record line".to_string()));
    }

    let tag = tokens.remove(0);
    let mut fields = Vec::new();
    for token in tokens {
        let Some((key, value)) = token.split_once(':') else {
            return Err(IdxError::Parse(format!(
                "field without key:value separator: {token}"
            )));
        };
        let parsed_value = if value.starts_with('"') {
            serde_json::from_str::<String>(value)
                .map_err(|e| IdxError::Parse(format!("invalid quoted value {value}: {e}")))?
        } else {
            value.to_string()
        };
        fields.push((key.to_string(), parsed_value));
    }
    Ok((tag, fields))
}

fn get<'a>(fields: &'a [(String, String)], key: &str) -> Result<&'a str, IdxError> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| IdxError::Parse(format!("missing required field {key}")))
}

fn get_opt<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn parse<T>(value: &str) -> Result<T, IdxError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    serde_json::from_str(&format!("\"{value}\""))
        .map_err(|e| IdxError::Parse(format!("cannot parse '{value}' as enum: {e}")))
}

/// Local trait for constructing the generated ID wrappers from a raw UUID.
trait IdFromUuid {
    fn from_uuid(uuid: uuid::Uuid) -> Self;
}

macro_rules! impl_id_from_uuid {
    ($($t:ty),*) => {
        $(impl IdFromUuid for $t {
            fn from_uuid(uuid: uuid::Uuid) -> Self {
                Self(uuid)
            }
        })*
    };
}

impl_id_from_uuid!(EntityId, EdgeId, RecordId, ConflictId, SocketId);

fn parse_uuid<T>(value: &str) -> Result<T, IdxError>
where
    T: IdFromUuid,
{
    let uuid = uuid::Uuid::parse_str(value)
        .map_err(|e| IdxError::Parse(format!("invalid uuid {value}: {e}")))?;
    Ok(T::from_uuid(uuid))
}

fn parse_f32(value: &str) -> Result<f32, IdxError> {
    value
        .parse()
        .map_err(|e| IdxError::Parse(format!("invalid f32 {value}: {e}")))
}

#[allow(dead_code)]
fn parse_u32(value: &str) -> Result<u32, IdxError> {
    value
        .parse()
        .map_err(|e| IdxError::Parse(format!("invalid u32 {value}: {e}")))
}

fn parse_bool(value: &str) -> Result<bool, IdxError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(IdxError::Parse(format!("invalid bool {value}"))),
    }
}

fn parse_green_code(value: &str) -> Result<GreenCode, IdxError> {
    match value {
        "G" | "green" => Ok(GreenCode::Green),
        "Y" | "yellow" => Ok(GreenCode::Yellow),
        "R" | "red" => Ok(GreenCode::Red),
        "U" | "unknown" => Ok(GreenCode::Unknown),
        _ => Err(IdxError::Parse(format!("invalid GreenCode {value}"))),
    }
}

fn parse_json_array<T>(value: &str) -> Result<Vec<T>, IdxError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    serde_json::from_str(value)
        .map_err(|e| IdxError::Parse(format!("invalid JSON array {value}: {e}")))
}

fn parse_entity(fields: &[(String, String)]) -> Result<AtlasEntity, IdxError> {
    let id = parse_uuid::<EntityId>(get(fields, "id")?)?;
    let scale = parse::<ScopeScale>(get(fields, "scale")?)?;
    let kind = parse::<EntityKind>(get(fields, "kind")?)?;
    let parent_id = get_opt(fields, "parent")
        .map(parse_uuid::<EntityId>)
        .transpose()?;
    let relative_path = get_opt(fields, "path").map(|s| s.to_string());
    let canonical_name = get(fields, "name")?.to_string();
    let structural_fingerprint = get_opt(fields, "fingerprint")
        .unwrap_or_default()
        .to_string();

    Ok(AtlasEntity {
        id,
        scale,
        kind,
        canonical_name,
        aliases: vec![],
        relative_path,
        parent_id,
        purpose_clauses: vec![],
        protected_decision_ids: vec![],
        responsibilities: vec![],
        interfaces: vec![],
        capabilities: vec![],
        sockets: vec![],
        source_spans: vec![],
        structural_fingerprint,
        local_evidence: vec![],
        inherited_evidence: vec![],
        green: None,
        open_conflict_ids: vec![],
        deterministic_summary: String::new(),
        inferred_summaries: vec![],
        uncertainty: vec![],
        snapshot_id: SnapshotId(String::new()),
        observed_at: String::new(),
        fresh_until: None,
    })
}

fn parse_edge(fields: &[(String, String)]) -> Result<AtlasEdge, IdxError> {
    let id = parse_uuid::<EdgeId>(get(fields, "id")?)?;
    let source_entity_id = parse_uuid::<EntityId>(get(fields, "from")?)?;
    let target_entity_id = parse_uuid::<EntityId>(get(fields, "to")?)?;
    let kind = parse::<RelationshipKind>(get(fields, "kind")?)?;
    let plane = parse::<RelationshipPlane>(get(fields, "plane")?)?;
    let required = parse_bool(get(fields, "required")?)?;
    let source_id = get(fields, "source")?.to_string();
    let confidence = parse_f32(get(fields, "confidence")?)?;
    let proof_record_ids = get_opt(fields, "proof")
        .map(parse_json_array::<RecordId>)
        .transpose()?
        .unwrap_or_default();

    Ok(AtlasEdge {
        id,
        source_entity_id,
        target_entity_id,
        kind,
        plane,
        proof_record_ids,
        evidence: vec![],
        source_id,
        confidence,
        observed_at: String::new(),
        fresh_until: None,
        required,
        snapshot_id: SnapshotId(String::new()),
    })
}

fn parse_source(fields: &[(String, String)]) -> Result<ContextSource, IdxError> {
    Ok(ContextSource {
        id: get(fields, "id")?.to_string(),
        kind: parse::<SourceKind>(get(fields, "kind")?)?,
        locator: get(fields, "locator")?.to_string(),
    })
}

fn parse_green(fields: &[(String, String)]) -> Result<(EntityId, GreenAssessment), IdxError> {
    let target = parse_uuid::<EntityId>(get(fields, "target")?)?;
    let mut axes = BTreeMap::new();
    for axis in GreenAxis::ALL {
        let code = get_opt(fields, &format!("{}", axis_letter(axis)))
            .map(parse_green_code)
            .transpose()?
            .unwrap_or(GreenCode::Unknown);
        axes.insert(
            axis,
            creature_context_types::AxisAssessment {
                code,
                required_proof: ProofStrength::Unknown,
                evidence: vec![],
                reasons: vec![],
            },
        );
    }
    let overall = get_opt(fields, "overall")
        .map(parse_green_code)
        .transpose()?
        .unwrap_or(GreenCode::Unknown);
    Ok((
        target,
        GreenAssessment {
            overall,
            axes,
            snapshot_id: SnapshotId(String::new()),
        },
    ))
}

fn axis_letter(axis: GreenAxis) -> char {
    match axis {
        GreenAxis::Content => 'C',
        GreenAxis::Structure => 'S',
        GreenAxis::Integration => 'I',
        GreenAxis::Verification => 'V',
        GreenAxis::Freshness => 'F',
        GreenAxis::Coherence => 'H',
    }
}

fn parse_socket(fields: &[(String, String)]) -> Result<AtlasSocket, IdxError> {
    Ok(Socket {
        id: parse_uuid::<SocketId>(get(fields, "id")?)?,
        entity_id: parse_uuid::<EntityId>(get(fields, "entity")?)?,
        direction: parse::<SocketDirection>(get(fields, "dir")?)?,
        shape: SocketShape {
            hash: get(fields, "shape")?.to_string(),
            qualified_name: get(fields, "name")?.to_string(),
            structural_signature: get(fields, "signature")?.to_string(),
            version: get(fields, "version")?.to_string(),
        },
        optional: parse_bool(get(fields, "optional")?)?,
        resolution: SocketResolution::Unresolved,
        source_id: get(fields, "source")?.to_string(),
        confidence: parse_f32(get(fields, "confidence")?)?,
        observed_at: String::new(),
        snapshot_id: SnapshotId(String::new()),
    })
}

fn parse_fit(fields: &[(String, String)]) -> Result<(SocketId, SocketFit), IdxError> {
    let required = parse_uuid::<SocketId>(get(fields, "require")?)?;
    let checked_by = get_opt(fields, "checked_by")
        .filter(|v| *v != "none")
        .map(parse::<FitProof>)
        .transpose()?;
    Ok((
        required,
        SocketFit {
            provided_socket_id: parse_uuid::<SocketId>(get(fields, "provide")?)?,
            basis: parse::<FitBasis>(get(fields, "basis")?)?,
            status: parse::<FitStatus>(get(fields, "status")?)?,
            checked_by,
            proof_path: parse::<ProofPathState>(get(fields, "proof_path")?)?,
            plane: parse::<FitPlane>(get(fields, "plane")?)?,
            confidence: parse_f32(get(fields, "confidence")?)?,
        },
    ))
}

fn parse_hole(fields: &[(String, String)]) -> Result<(SocketId, SocketHole), IdxError> {
    let socket_id = parse_uuid::<SocketId>(get(fields, "socket")?)?;
    let candidates = get_opt(fields, "candidates")
        .map(parse_json_array::<SocketId>)
        .transpose()?
        .unwrap_or_default();
    Ok((
        socket_id,
        SocketHole {
            reason: parse::<HoleReason>(get(fields, "reason")?)?,
            candidates,
            adapter_target: parse_bool(get(fields, "adapter_target")?)?,
        },
    ))
}

fn parse_record(tag: &str, fields: &[(String, String)]) -> Result<ContextRecord, IdxError> {
    let record_type = match tag {
        "@purpose" => ContextRecordType::Purpose,
        "@decision" => ContextRecordType::Decision,
        "@constraint" => ContextRecordType::Constraint,
        "@task" => ContextRecordType::Task,
        "@requirement" => ContextRecordType::Requirement,
        "@question" => ContextRecordType::Question,
        "@finding" => ContextRecordType::Finding,
        "@permission" => ContextRecordType::Permission,
        "@activity" => ContextRecordType::Activity,
        "@summary" => ContextRecordType::Summary,
        _ => return Err(IdxError::Parse(format!("not a record tag: {tag}"))),
    };

    let id = if let Some(raw) = get_opt(fields, "id") {
        if raw.contains('/') {
            // Synthesised IDs such as "<entity>/purpose/0" are not stable UUIDs.
            // The encoder did not preserve the original record ID, so generate a
            // fresh one. Re-encoding derives constraint IDs from entity state,
            // not from the decoded record, so this does not affect byte identity.
            RecordId::new()
        } else {
            parse_uuid::<RecordId>(raw)?
        }
    } else {
        RecordId::new()
    };

    let scope_id = get_opt(fields, "scope")
        .map(parse_uuid::<EntityId>)
        .transpose()?
        .unwrap_or_else(EntityId::default);

    let source_id = get_opt(fields, "source").unwrap_or("").to_string();
    let authority = get_opt(fields, "authority")
        .map(parse::<AuthoritySource>)
        .transpose()?
        .unwrap_or(AuthoritySource::System);
    let confidence = get_opt(fields, "confidence")
        .map(parse_f32)
        .transpose()?
        .unwrap_or(1.0);
    let state = get_opt(fields, "state")
        .or_else(|| get_opt(fields, "status"))
        .map(parse::<RecordState>)
        .transpose()?
        .unwrap_or(RecordState::Active);
    let value = get(fields, "text")?.to_string();

    Ok(ContextRecord {
        id,
        record_type,
        value,
        scope_id,
        source_id,
        authority,
        confidence,
        created_at: String::new(),
        observed_at: String::new(),
        expires_at: None,
        supersedes: vec![],
        contradicts: vec![],
        content_hash: String::new(),
        snapshot_id: SnapshotId(String::new()),
        privacy_class: PrivacyClass::Project,
        state,
    })
}

fn parse_evidence(fields: &[(String, String)]) -> Result<(String, Evidence), IdxError> {
    let target = get(fields, "target")?.to_string();
    let evidence = Evidence {
        axis: parse::<GreenAxis>(get(fields, "axis")?)?,
        source: parse::<FactSource>(get(fields, "source")?)?,
        proof: parse::<ProofStrength>(get(fields, "proof")?)?,
        outcome: parse::<EvidenceOutcome>(get(fields, "outcome")?)?,
        confidence: parse_f32(get(fields, "confidence")?)?,
        fingerprint: String::new(),
        observed_at: get_opt(fields, "observed").unwrap_or("").to_string(),
        producer: get_opt(fields, "producer").unwrap_or("").to_string(),
        snapshot_id: SnapshotId(get_opt(fields, "snapshot").unwrap_or("").to_string()),
        message: String::new(),
    };
    Ok((target, evidence))
}

fn parse_conflict(fields: &[(String, String)]) -> Result<ConflictRecord, IdxError> {
    Ok(ConflictRecord {
        id: parse_uuid::<ConflictId>(get(fields, "id")?)?,
        left_record_id: parse_uuid::<RecordId>(get(fields, "left")?)?,
        right_record_id: parse_uuid::<RecordId>(get(fields, "right")?)?,
        state: parse::<ConflictState>(get(fields, "state")?)?,
        severity: parse_green_code(get(fields, "severity")?)?,
        resolution_record_id: None,
        created_at: String::new(),
        snapshot_id: SnapshotId(String::new()),
    })
}

/// Parse IDX text into typed records.
pub fn decode_atlas_idx(input: &str) -> Result<DecodedIdx, IdxError> {
    let normalised = input.replace("\r\n", "\n");
    let mut snapshot_id: Option<SnapshotId> = None;
    let mut timestamp = String::new();

    let mut entities: Vec<AtlasEntity> = Vec::new();
    let mut entity_map: HashMap<EntityId, usize> = HashMap::new();
    let mut edges: Vec<AtlasEdge> = Vec::new();
    let mut records: Vec<ContextRecord> = Vec::new();
    let mut sources: Vec<ContextSource> = Vec::new();
    let mut conflicts: Vec<ConflictRecord> = Vec::new();
    let mut opaque_records: Vec<String> = Vec::new();
    let mut green_map: HashMap<EntityId, GreenAssessment> = HashMap::new();
    let mut sockets: Vec<AtlasSocket> = Vec::new();
    let mut socket_map: HashMap<SocketId, usize> = HashMap::new();
    let mut fits: HashMap<SocketId, SocketFit> = HashMap::new();
    let mut holes: HashMap<SocketId, SocketHole> = HashMap::new();
    let mut entity_evidence: HashMap<EntityId, Vec<Evidence>> = HashMap::new();
    let mut edge_evidence_map: HashMap<EdgeId, Vec<Evidence>> = HashMap::new();

    for line in normalised.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Header lines do not start with a record type.
        if line.starts_with("@creature-context") {
            let (_, fields) = tokenize(line)?;
            if let Some(id) = get_opt(&fields, "snapshot") {
                snapshot_id = Some(SnapshotId(id.to_string()));
            }
            continue;
        }
        if let Some(after_tag) = line.strip_prefix("@generated") {
            // @generated <timestamp> producer:<name> deterministic:<bool>
            // The timestamp is positional and may itself contain colons, so it
            // is emitted as a JSON-quoted string. Extract it by walking past the
            // tag, then parse the remaining key:value fields.
            let after_tag = after_tag.trim_start();
            let (ts_token, rest) = split_first_token(after_tag)?;
            let ts = if ts_token.starts_with('"') {
                serde_json::from_str::<String>(ts_token)
                    .map_err(|e| IdxError::Parse(format!("invalid generated timestamp: {e}")))?
            } else {
                ts_token.to_string()
            };
            timestamp = ts;
            // We do not currently need producer/deterministic; ignore them.
            let _ = rest;
            continue;
        }
        if line.starts_with("@legend") {
            continue;
        }

        let (tag, fields) = tokenize(line)?;
        match tag.as_str() {
            "@entity" => {
                let entity = parse_entity(&fields)?;
                entity_map.insert(entity.id, entities.len());
                entities.push(entity);
            }
            "@edge" => {
                let edge = parse_edge(&fields)?;
                edges.push(edge);
            }
            "@source" => {
                sources.push(parse_source(&fields)?);
            }
            "@green" => {
                let (target, green) = parse_green(&fields)?;
                green_map.insert(target, green);
            }
            "@socket" => {
                let socket = parse_socket(&fields)?;
                socket_map.insert(socket.id, sockets.len());
                sockets.push(socket);
            }
            "@fit" => {
                let (required, fit) = parse_fit(&fields)?;
                fits.insert(required, fit);
            }
            "@hole" => {
                let (socket_id, hole) = parse_hole(&fields)?;
                holes.insert(socket_id, hole);
            }
            "@evidence" => {
                let (target, evidence) = parse_evidence(&fields)?;
                if let Ok(entity_id) = parse_uuid::<EntityId>(&target) {
                    entity_evidence.entry(entity_id).or_default().push(evidence);
                } else if let Ok(edge_id) = parse_uuid::<EdgeId>(&target) {
                    edge_evidence_map.entry(edge_id).or_default().push(evidence);
                }
            }
            "@conflict" => {
                conflicts.push(parse_conflict(&fields)?);
            }
            "@purpose" | "@decision" | "@constraint" | "@task" | "@requirement" | "@question"
            | "@finding" | "@permission" | "@activity" | "@summary" => {
                records.push(parse_record(&tag, &fields)?);
            }
            "@child" | "@uncertainty" => {
                // Projection and annotation records are not part of the canonical
                // snapshot; keep them as opaque forward-compatible data.
                opaque_records.push(line.to_string());
            }
            _ => {
                opaque_records.push(line.to_string());
            }
        }
    }

    // Attach green assessments to entities.
    for (entity_id, green) in green_map {
        if let Some(index) = entity_map.get(&entity_id) {
            entities[*index].green = Some(green);
        }
    }

    // Attach evidence to entities and edges.
    for (entity_id, evidence) in entity_evidence {
        if let Some(index) = entity_map.get(&entity_id) {
            // Round-trip cannot distinguish local from inherited; treat as local.
            entities[*index].local_evidence = evidence;
        }
    }
    for (edge_id, evidence) in edge_evidence_map {
        if let Some(edge) = edges.iter_mut().find(|e| e.id == edge_id) {
            edge.evidence = evidence;
        }
    }

    // Attach socket resolutions.
    for (socket_id, fit) in fits {
        if let Some(index) = socket_map.get(&socket_id) {
            sockets[*index].resolution = SocketResolution::Fit(fit);
        }
    }
    for (socket_id, hole) in holes {
        if let Some(index) = socket_map.get(&socket_id) {
            sockets[*index].resolution = SocketResolution::Hole(hole);
        }
    }

    // Attach sockets to their owning entities.
    for socket in sockets {
        if let Some(index) = entity_map.get(&socket.entity_id) {
            entities[*index].sockets.push(socket);
        }
    }

    // Fill snapshot IDs and timestamps from the header.
    let snapshot_id = snapshot_id.unwrap_or_else(|| SnapshotId(String::new()));
    for entity in &mut entities {
        entity.snapshot_id = snapshot_id.clone();
        if entity.observed_at.is_empty() {
            entity.observed_at.clone_from(&timestamp);
        }
    }
    for edge in &mut edges {
        edge.snapshot_id = snapshot_id.clone();
        if edge.observed_at.is_empty() {
            edge.observed_at.clone_from(&timestamp);
        }
    }
    for record in &mut records {
        record.snapshot_id = snapshot_id.clone();
        if record.observed_at.is_empty() {
            record.observed_at.clone_from(&timestamp);
        }
        if record.created_at.is_empty() {
            record.created_at.clone_from(&timestamp);
        }
    }

    Ok(DecodedIdx {
        snapshot: AtlasSnapshot {
            id: snapshot_id,
            timestamp,
            entities,
            edges,
            records,
            conflicts,
            sources,
        },
        opaque_records,
    })
}

#[cfg(test)]
mod tests {
    use super::tokenize;

    #[test]
    fn tokenize_respects_quoted_strings() {
        let line = r#"@entity id:abc name:"hello world" path:src/a.rs"#;
        let (tag, fields) = tokenize(line).unwrap();
        assert_eq!(tag, "@entity");
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[1].1, "hello world");
    }

    #[test]
    fn tokenize_handles_escaped_quotes() {
        let line = r#"@entity id:abc name:"say \"hi\"" path:src/a.rs"#;
        let (_tag, fields) = tokenize(line).unwrap();
        assert_eq!(fields[1].1, r#"say "hi""#);
    }
}
