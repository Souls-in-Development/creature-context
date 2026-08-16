//! Canonical Atlas IDX encoder for the record set in specification section 5.2.
//!
//! Records are emitted in type priority order. Within a type, stable IDs and
//! source locators decide order, so the same typed snapshot produces the same
//! bytes on every supported operating system.

use super::{IdxError, IdxScope, escape::field};
use creature_context_types::{
    AtlasEntity, AtlasSnapshot, AtlasSocket, Evidence, ProjectId,
    authority::AuthoritySource,
    context::{ContextRecord, ContextRecordType, ContextSource},
    green::{GreenAssessment, GreenAxis},
    socket::{SocketFit, SocketHole, SocketResolution},
};

/// Defines the compact atoms used by this file. Specification 5.1 forbids an
/// IDX reader from needing undocumented abbreviations.
const LEGEND: &str = "@legend \
id=stable-id scale=universe|galaxy|system|planet|moon kind=entity-kind \
parent=containing-id path=repo-relative-path name=canonical-name \
from=source-id to=target-id entity=owner-id plane=declared|observed|inferred \
required=bool proof=evidence-ref proof_path=unchecked|available|unavailable \
outcome=unknown|pass|warning|fail authority=system|human|project|tool|model \
confidence=0..1 axis=content|structure|integration|verification|freshness|coherence \
C=content S=structure I=integration V=verification F=freshness H=coherence \
codes=G:green|Y:yellow|R:red|U:unknown state=open|resolved \
dir=requires|provides basis=unique|ranked fit=unconfirmed|confirmed|rejected \
hole=no_match|ambiguous signature=structural-signature version=shape-version \
checked_by=typecheck|build|test|human|none locator=portable-source-reference";

fn atom<T: serde::Serialize>(value: T) -> Result<String, IdxError> {
    Ok(serde_json::to_value(value)?
        .as_str()
        .unwrap_or_default()
        .to_string())
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

fn green_line(target: &str, green: &GreenAssessment) -> String {
    let mut line = format!("@green target:{target}");
    for axis in GreenAxis::ALL {
        let code = green.axes.get(&axis).map(|a| a.code.short()).unwrap_or('U');
        line.push_str(&format!(" {}:{}", axis_letter(axis), code));
    }
    line.push_str(&format!(" overall:{}", green.overall.short()));
    line
}

fn entity_line(entity: &AtlasEntity) -> Result<String, IdxError> {
    let mut line = format!(
        "@entity id:{} scale:{} kind:{}",
        entity.id,
        atom(entity.scale)?,
        atom(entity.kind)?,
    );
    if let Some(parent) = entity.parent_id {
        line.push_str(&format!(" parent:{parent}"));
    }
    if let Some(path) = &entity.relative_path {
        line.push_str(&format!(" path:{}", field(path)));
    }
    line.push_str(&format!(" name:{}", field(&entity.canonical_name)));
    if !entity.structural_fingerprint.is_empty() {
        line.push_str(&format!(
            " fingerprint:{}",
            field(&entity.structural_fingerprint)
        ));
    }
    Ok(line)
}

fn edge_line(edge: &creature_context_types::AtlasEdge) -> Result<String, IdxError> {
    let mut line = format!(
        "@edge id:{} from:{} to:{} kind:{} plane:{} required:{} source:{} confidence:{}",
        edge.id,
        edge.source_entity_id,
        edge.target_entity_id,
        atom(edge.kind)?,
        atom(edge.plane)?,
        edge.required,
        field(&edge.source_id),
        edge.confidence,
    );
    if !edge.proof_record_ids.is_empty() {
        let proof: Vec<String> = edge
            .proof_record_ids
            .iter()
            .map(ToString::to_string)
            .collect();
        line.push_str(&format!(" proof:{}", serde_json::to_string(&proof)?));
    }
    Ok(line)
}

fn evidence_line(target: &str, evidence: &Evidence) -> Result<(String, String), IdxError> {
    let axis = atom(evidence.axis)?;
    let outcome = atom(evidence.outcome)?;
    let proof = atom(evidence.proof)?;
    let source = atom(evidence.source)?;
    let stable_id = format!(
        "{target}/{axis}/{outcome}/{proof}/{source}/{}/{}/{}/{}",
        evidence.producer, evidence.fingerprint, evidence.snapshot_id, evidence.observed_at,
    );
    let line = format!(
        "@evidence id:{} target:{} axis:{} outcome:{} proof:{} source:{} confidence:{} producer:{} snapshot:{} observed:{}",
        field(&stable_id),
        target,
        axis,
        outcome,
        proof,
        source,
        evidence.confidence,
        field(&evidence.producer),
        field(&evidence.snapshot_id.0),
        field(&evidence.observed_at),
    );
    Ok((stable_id, line))
}

fn source_line(source: &ContextSource) -> Result<String, IdxError> {
    Ok(format!(
        "@source id:{} kind:{} locator:{}",
        field(&source.id),
        atom(source.kind)?,
        field(&source.locator),
    ))
}

fn socket_line(socket: &AtlasSocket) -> Result<String, IdxError> {
    Ok(format!(
        "@socket id:{} entity:{} dir:{} shape:{} name:{} signature:{} version:{} optional:{} source:{} confidence:{}",
        socket.id,
        socket.entity_id,
        atom(socket.direction)?,
        field(&socket.shape.hash),
        field(&socket.shape.qualified_name),
        field(&socket.shape.structural_signature),
        field(&socket.shape.version),
        socket.optional,
        field(&socket.source_id),
        socket.confidence,
    ))
}

fn fit_line(required: &AtlasSocket, fit: &SocketFit) -> Result<String, IdxError> {
    let checked_by = fit
        .checked_by
        .map(atom)
        .transpose()?
        .unwrap_or_else(|| "none".to_string());
    Ok(format!(
        "@fit require:{} provide:{} basis:{} status:{} checked_by:{} proof_path:{} plane:{} confidence:{}",
        required.id,
        fit.provided_socket_id,
        atom(fit.basis)?,
        atom(fit.status)?,
        checked_by,
        atom(fit.proof_path)?,
        atom(fit.plane)?,
        fit.confidence,
    ))
}

fn hole_line(required: &AtlasSocket, hole: &SocketHole) -> Result<String, IdxError> {
    let candidates: Vec<String> = hole.candidates.iter().map(ToString::to_string).collect();
    Ok(format!(
        "@hole socket:{} want:{} reason:{} candidates:{} adapter_target:{}",
        required.id,
        field(&required.shape.hash),
        atom(hole.reason)?,
        serde_json::to_string(&candidates)?,
        hole.adapter_target,
    ))
}

fn summary_source(authority: &AuthoritySource) -> &'static str {
    match authority {
        AuthoritySource::Human => "human",
        AuthoritySource::Model => "inferred",
        AuthoritySource::Project => "declared",
        AuthoritySource::System | AuthoritySource::Tool => "observed",
    }
}

fn matching_records<'a>(
    snapshot: &'a AtlasSnapshot,
    record_type: ContextRecordType,
    visible_entities: Option<&std::collections::BTreeSet<creature_context_types::EntityId>>,
) -> Vec<&'a ContextRecord> {
    let mut records: Vec<_> = snapshot
        .records
        .iter()
        .filter(|record| {
            record.record_type == record_type
                && visible_entities.is_none_or(|visible| visible.contains(&record.scope_id))
        })
        .collect();
    records.sort_by_key(|record| record.id.to_string());
    records
}

fn record_line(tag: &str, record: &ContextRecord) -> Result<String, IdxError> {
    let state_key = match record.record_type {
        ContextRecordType::Decision => "status",
        _ => "state",
    };
    let mut line = format!(
        "{tag} id:{} scope:{} authority:{}",
        record.id,
        record.scope_id,
        atom(record.authority.clone())?,
    );
    if record.record_type != ContextRecordType::Constraint {
        line.push_str(&format!(" {state_key}:{}", atom(record.state.clone())?));
    }
    line.push_str(&format!(
        " source:{} confidence:{} text:{}",
        field(&record.source_id),
        record.confidence,
        field(&record.value),
    ));
    Ok(line)
}

const ORBIT_LEGEND: &str = "@legend orbit \
kind=orbit ring=0..4 mandatory=bool score=int tokens=int reason=text \
axis=content|structure|integration|verification|freshness|coherence \
side=left|right|match|difference|left_only|right_only unresolved=text";

fn orbit_header(packet: &creature_context_types::orbit::OrbitPacket) -> Result<String, IdxError> {
    let minimum = packet
        .minimum_required_tokens
        .map(|n| n.to_string())
        .unwrap_or_else(|| "-".to_string());
    Ok(format!(
        "@creature-context v:1 kind:orbit id:{} scale:{} mode:{} task:{} budget:{} estimated:{} minimum_required:{}\n",
        field(&packet.id),
        atom(packet.scale)?,
        atom(packet.mode)?,
        field(&packet.task),
        packet.budget,
        packet.estimated_total_tokens,
        minimum,
    ))
}

fn selected_entity_line(item: &creature_context_types::orbit::SelectedEntity) -> String {
    let mut line = format!(
        "@selected id:{} ring:{} mandatory:{} score:{} tokens:{} name:{} scale:{}",
        item.entity.id,
        item.ring,
        item.mandatory,
        item.score,
        item.estimated_tokens,
        field(&item.entity.canonical_name),
        atom(item.entity.scale).unwrap_or_default(),
    );
    for reason in &item.reasons {
        line.push_str(&format!(" reason:{}", field(reason)));
    }
    if let Some(path) = &item.entity.relative_path {
        line.push_str(&format!(" path:{}", field(path)));
    }
    line
}

fn spine_entity_line(entity: &creature_context_types::AtlasEntity) -> String {
    let mut line = format!(
        "@spine id:{} scale:{} name:{}",
        entity.id,
        atom(entity.scale).unwrap_or_default(),
        field(&entity.canonical_name),
    );
    if let Some(path) = &entity.relative_path {
        line.push_str(&format!(" path:{}", field(path)));
    }
    line
}

fn relationship_line(
    item: &creature_context_types::orbit::SelectedEdge,
) -> Result<String, IdxError> {
    Ok(format!(
        "@relationship id:{} from:{} to:{} kind:{} plane:{} ring:{} mandatory:{} tokens:{} reason:{}",
        item.edge.id,
        item.edge.source_entity_id,
        item.edge.target_entity_id,
        atom(item.edge.kind)?,
        atom(item.edge.plane)?,
        item.ring,
        item.mandatory,
        item.estimated_tokens,
        field(&item.reasons.join("; ")),
    ))
}

fn context_record_line(
    item: &creature_context_types::orbit::SelectedContextRecord,
) -> Result<String, IdxError> {
    Ok(format!(
        "@context id:{} type:{} scope:{} ring:{} mandatory:{} tokens:{} authority:{} source:{} text:{}",
        item.record.id,
        atom(item.record.record_type.clone())?,
        item.record.scope_id,
        item.ring,
        item.mandatory,
        item.estimated_tokens,
        atom(item.record.authority.clone())?,
        field(&item.record.source_id),
        field(&item.record.value),
    ))
}

fn comparison_line(result: &creature_context_types::ComparisonResult) -> String {
    format!(
        "@comparison left:{} right:{} matches:{} differences:{} left_only:{} right_only:{} unresolved:{}",
        result.left_id,
        result.right_id,
        result.matches.len(),
        result.differences.len(),
        result.left_only.len(),
        result.right_only.len(),
        serde_json::to_string(&result.unresolved).unwrap_or_default(),
    )
}

fn comparison_item_line(
    tag: &str,
    item: &creature_context_types::ComparisonItem,
) -> Result<String, IdxError> {
    let left = item.left.as_deref().unwrap_or("-");
    let right = item.right.as_deref().unwrap_or("-");
    Ok(format!(
        "@compare_item tag:{} dimension:{} confidence:{} left:{} right:{} explanation:{}",
        tag,
        atom(item.dimension)?,
        item.confidence,
        field(left),
        field(right),
        field(&item.explanation),
    ))
}

/// Encode an Orbit packet as canonical IDX.
pub fn encode_orbit_idx(
    packet: &creature_context_types::orbit::OrbitPacket,
) -> Result<String, IdxError> {
    let mut out = orbit_header(packet)?;
    out.push_str(ORBIT_LEGEND);
    out.push('\n');

    for reason in &packet.selection_reasons {
        out.push_str(&format!("@selection reason:{}\n", field(reason)));
    }

    let mut selected = packet.selected_entities.clone();
    selected.sort_by_key(|s| {
        (
            s.ring,
            std::cmp::Reverse(s.mandatory),
            std::cmp::Reverse(s.score),
            s.entity.id,
        )
    });
    for item in &selected {
        out.push_str(&selected_entity_line(item));
        out.push('\n');
    }

    let mut spine = packet.architectural_spine.clone();
    spine.sort_by_key(|e| (e.scale.rank(), e.canonical_name.to_lowercase(), e.id));
    for entity in &spine {
        out.push_str(&spine_entity_line(entity));
        out.push('\n');
    }

    let mut relationships = packet.relationships.clone();
    relationships.sort_by_key(|r| (r.ring, r.edge.id));
    for item in &relationships {
        out.push_str(&relationship_line(item)?);
        out.push('\n');
    }

    let mut records = packet.context_records.clone();
    records.sort_by_key(|r| (r.ring, r.record.id));
    for item in &records {
        out.push_str(&context_record_line(item)?);
        out.push('\n');
    }

    for (category, count) in &packet.omission_counts {
        out.push_str(&format!(
            "@omission category:{} count:{}\n",
            field(category),
            count
        ));
    }

    for resolved in &packet.resolved_references {
        let requested = resolved
            .requested
            .relative_path
            .as_deref()
            .or(resolved.requested.symbol.as_deref())
            .unwrap_or("-");
        out.push_str(&format!(
            "@resolved requested:{} entity:{} reason:{}\n",
            field(requested),
            resolved.entity_id,
            field(&resolved.reason),
        ));
    }

    for uncertainty in &packet.uncertainty {
        out.push_str(&format!("@uncertainty text:{}\n", field(uncertainty)));
    }

    if let Some(comparison) = &packet.comparison {
        out.push_str(&comparison_line(comparison));
        out.push('\n');
        for item in &comparison.matches {
            out.push_str(&comparison_item_line("match", item)?);
            out.push('\n');
        }
        for item in &comparison.differences {
            out.push_str(&comparison_item_line("difference", item)?);
            out.push('\n');
        }
        for item in &comparison.left_only {
            out.push_str(&comparison_item_line("left_only", item)?);
            out.push('\n');
        }
        for item in &comparison.right_only {
            out.push_str(&comparison_item_line("right_only", item)?);
            out.push('\n');
        }
    }

    Ok(out)
}

/// Encode a snapshot as canonical IDX at the requested scope.
///
/// `project_id` is explicit because project identity belongs to the project
/// registry, not to each persisted snapshot.
pub fn encode_atlas_idx(
    snapshot: &AtlasSnapshot,
    scope: IdxScope,
    project_id: &ProjectId,
) -> Result<String, IdxError> {
    let (scale_atom, root) = match scope {
        IdxScope::Galaxy => ("galaxy", None),
        IdxScope::Folder(id) => ("folder", Some(id)),
    };

    let in_scope = |entity: &AtlasEntity| -> bool {
        match root {
            None => true,
            Some(root_id) => {
                if entity.id == root_id {
                    return true;
                }
                // A per-folder Atlas includes only the folder entity and its
                // direct children. Deeper descendants live in their own
                // folder's ATLAS.idx, referenced by @child records.
                entity.parent_id == Some(root_id)
            }
        }
    };

    let mut entities: Vec<&AtlasEntity> =
        snapshot.entities.iter().filter(|e| in_scope(e)).collect();
    entities.sort_by_key(|entity| (entity.scale.rank(), entity.id.to_string()));
    let visible: std::collections::BTreeSet<creature_context_types::EntityId> =
        entities.iter().map(|entity| entity.id).collect();
    let record_scope = root.map(|_| &visible);

    let mut out = String::new();
    out.push_str(&format!(
        "@creature-context v:1 kind:atlas scale:{} project:{} snapshot:{}\n",
        scale_atom,
        field(&project_id.to_string()),
        field(&snapshot.id.0),
    ));
    out.push_str(&format!(
        "@generated {} producer:creature-context deterministic:true\n",
        field(&snapshot.timestamp),
    ));
    out.push_str(LEGEND);
    out.push('\n');

    // Core records follow specification 5.2 type priority.
    for record in matching_records(snapshot, ContextRecordType::Purpose, record_scope) {
        out.push_str(&format!(
            "@purpose id:{} authority:{} scope:{} text:{} source:{}\n",
            record.id,
            atom(record.authority.clone())?,
            record.scope_id,
            field(&record.value),
            field(&record.source_id),
        ));
    }

    for entity in &entities {
        out.push_str(&entity_line(entity)?);
        out.push('\n');
    }

    for entity in &entities {
        if !entity.deterministic_summary.is_empty() {
            out.push_str(&format!(
                "@summary id:{} source:parsed confidence:1 text:{}\n",
                entity.id,
                field(&entity.deterministic_summary),
            ));
        }
        let mut inferred = entity
            .inferred_summaries
            .iter()
            .enumerate()
            .collect::<Vec<_>>();
        inferred.sort_by_key(|(_, summary)| {
            (
                summary.model_id.as_str(),
                summary.value.as_str(),
                summary.snapshot_id.0.as_str(),
            )
        });
        for (index, summary) in inferred {
            out.push_str(&format!(
                "@summary id:{}/inferred/{} source:inferred confidence:{} model:{} text:{}\n",
                entity.id,
                index,
                summary.confidence,
                field(&summary.model_id),
                field(&summary.value),
            ));
        }
    }
    for record in matching_records(snapshot, ContextRecordType::Summary, record_scope) {
        out.push_str(&format!(
            "@summary id:{} source:{} authority:{} confidence:{} text:{}\n",
            record.id,
            summary_source(&record.authority),
            atom(record.authority.clone())?,
            record.confidence,
            field(&record.value),
        ));
    }

    let mut edges: Vec<_> = snapshot
        .edges
        .iter()
        .filter(|edge| {
            visible.contains(&edge.source_entity_id) || visible.contains(&edge.target_entity_id)
        })
        .collect();
    edges.sort_by_key(|edge| edge.id.to_string());
    for edge in &edges {
        out.push_str(&edge_line(edge)?);
        out.push('\n');
    }

    for record in matching_records(snapshot, ContextRecordType::Decision, record_scope) {
        out.push_str(&record_line("@decision", record)?);
        out.push('\n');
    }

    let mut constraints: Vec<(String, String)> =
        matching_records(snapshot, ContextRecordType::Constraint, record_scope)
            .into_iter()
            .map(|record| Ok((record.id.to_string(), record_line("@constraint", record)?)))
            .collect::<Result<_, IdxError>>()?;
    for entity in &entities {
        for (index, clause) in entity.purpose_clauses.iter().enumerate() {
            let id = format!("{}/purpose/{index}", entity.id);
            constraints.push((
                id.clone(),
                format!(
                    "@constraint id:{} scope:{} authority:human source:PURPOSE.md confidence:1 text:{}",
                    field(&id),
                    entity.id,
                    field(clause),
                ),
            ));
        }
    }
    constraints.sort_by(|left, right| left.0.cmp(&right.0));
    for (_, line) in constraints {
        out.push_str(&line);
        out.push('\n');
    }

    for (record_type, tag) in [
        (ContextRecordType::Task, "@task"),
        (ContextRecordType::Requirement, "@requirement"),
        (ContextRecordType::Question, "@question"),
        (ContextRecordType::Finding, "@finding"),
        (ContextRecordType::Permission, "@permission"),
        (ContextRecordType::Activity, "@activity"),
    ] {
        for record in matching_records(snapshot, record_type, record_scope) {
            out.push_str(&record_line(tag, record)?);
            out.push('\n');
        }
    }

    let mut evidence_lines = std::collections::BTreeMap::new();
    for entity in &entities {
        for evidence in entity
            .local_evidence
            .iter()
            .chain(entity.inherited_evidence.iter())
        {
            let (id, line) = evidence_line(&entity.id.to_string(), evidence)?;
            evidence_lines.insert(id, line);
        }
    }
    for edge in &edges {
        for evidence in &edge.evidence {
            let (id, line) = evidence_line(&edge.id.to_string(), evidence)?;
            evidence_lines.insert(id, line);
        }
    }
    for (_, line) in evidence_lines {
        out.push_str(&line);
        out.push('\n');
    }

    for entity in &entities {
        if let Some(green) = &entity.green {
            out.push_str(&green_line(&entity.id.to_string(), green));
            out.push('\n');
        }
    }

    let mut conflicts: Vec<_> = snapshot.conflicts.iter().collect();
    conflicts.sort_by_key(|conflict| conflict.id.to_string());
    for conflict in conflicts {
        out.push_str(&format!(
            "@conflict id:{} left:{} right:{} state:{} severity:{}\n",
            conflict.id,
            conflict.left_record_id,
            conflict.right_record_id,
            atom(conflict.state)?,
            conflict.severity.short(),
        ));
    }

    if let Some(root_id) = root {
        let mut children: Vec<_> = snapshot
            .entities
            .iter()
            .filter(|entity| {
                entity.parent_id == Some(root_id)
                    && entity.scale != creature_context_types::ScopeScale::Moon
                    && entity.relative_path.is_some()
            })
            .collect();
        children.sort_by_key(|child| child.id.to_string());
        for child in children {
            let path = child.relative_path.as_deref().unwrap_or_default();
            let atlas_path = if path == "." || path.is_empty() {
                "ATLAS.idx".to_string()
            } else {
                format!("{}/ATLAS.idx", path.trim_end_matches('/'))
            };
            let summary = if child.deterministic_summary.is_empty() {
                &child.canonical_name
            } else {
                &child.deterministic_summary
            };
            out.push_str(&format!(
                "@child id:{} path:{} summary:{}\n",
                child.id,
                field(&atlas_path),
                field(summary),
            ));
        }
    }

    // Sockets belong to visible entities; collect them early so their source IDs
    // can be included in the source filter below.
    let mut sockets: Vec<&AtlasSocket> = entities
        .iter()
        .flat_map(|entity| entity.sockets.iter())
        .collect();
    sockets.sort_by_key(|socket| socket.id.to_string());

    // Sources are only useful when something in the visible scope references them.
    // Collect referenced source IDs from emitted records, edges and sockets so that
    // a folder-scoped Atlas does not leak sources from sibling branches.
    let mut referenced_sources: std::collections::BTreeSet<&str> =
        std::collections::BTreeSet::new();
    for record in matching_records(snapshot, ContextRecordType::Purpose, record_scope) {
        referenced_sources.insert(record.source_id.as_str());
    }
    for record_type in [
        ContextRecordType::Decision,
        ContextRecordType::Constraint,
        ContextRecordType::Task,
        ContextRecordType::Requirement,
        ContextRecordType::Question,
        ContextRecordType::Finding,
        ContextRecordType::Permission,
        ContextRecordType::Activity,
    ] {
        for record in matching_records(snapshot, record_type, record_scope) {
            referenced_sources.insert(record.source_id.as_str());
        }
    }
    for edge in &edges {
        referenced_sources.insert(edge.source_id.as_str());
    }
    for socket in &sockets {
        referenced_sources.insert(socket.source_id.as_str());
    }

    let mut sources: Vec<_> = snapshot
        .sources
        .iter()
        .filter(|source| referenced_sources.contains(source.id.as_str()))
        .collect();
    sources.sort_by(|left, right| left.id.cmp(&right.id));
    for source in sources {
        out.push_str(&source_line(source)?);
        out.push('\n');
    }
    for socket in &sockets {
        out.push_str(&socket_line(socket)?);
        out.push('\n');
    }
    for socket in &sockets {
        if let SocketResolution::Fit(fit) = &socket.resolution {
            out.push_str(&fit_line(socket, fit)?);
            out.push('\n');
        }
    }
    for socket in &sockets {
        if let SocketResolution::Hole(hole) = &socket.resolution {
            out.push_str(&hole_line(socket, hole)?);
            out.push('\n');
        }
    }

    let mut uncertainties = Vec::new();
    for entity in &entities {
        for (index, uncertainty) in entity.uncertainty.iter().enumerate() {
            let id = format!("{}/uncertainty/{index}", entity.id);
            uncertainties.push((
                id.clone(),
                format!(
                    "@uncertainty id:{} scope:{} text:{}",
                    field(&id),
                    entity.id,
                    field(uncertainty),
                ),
            ));
        }
    }
    uncertainties.sort_by(|left, right| left.0.cmp(&right.0));
    for (_, line) in uncertainties {
        out.push_str(&line);
        out.push('\n');
    }

    Ok(out)
}
