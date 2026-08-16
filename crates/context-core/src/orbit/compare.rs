use creature_context_types::*;
use std::collections::BTreeSet;

pub fn compare_entities(
    left: &AtlasEntity,
    right: &AtlasEntity,
    dimensions: &[ComparisonDimension],
) -> ComparisonResult {
    let dims: Vec<_> = if dimensions.is_empty() {
        vec![
            ComparisonDimension::Purpose,
            ComparisonDimension::Capabilities,
            ComparisonDimension::Architecture,
            ComparisonDimension::Verification,
            ComparisonDimension::ProtectedDecisions,
            ComparisonDimension::Risks,
        ]
    } else {
        dimensions.to_vec()
    };
    let mut result = ComparisonResult {
        left_id: left.id,
        right_id: right.id,
        matches: Vec::new(),
        differences: Vec::new(),
        left_only: Vec::new(),
        right_only: Vec::new(),
        unresolved: Vec::new(),
    };
    for dimension in dims {
        let (left_values, right_values) = values(left, right, dimension);
        let left_set: BTreeSet<_> = left_values.into_iter().collect();
        let right_set: BTreeSet<_> = right_values.into_iter().collect();
        for value in left_set.intersection(&right_set) {
            result.matches.push(item(
                dimension,
                Some(value.clone()),
                Some(value.clone()),
                "both entities declare the same value",
            ));
        }
        for value in left_set.difference(&right_set) {
            result.left_only.push(item(
                dimension,
                Some(value.clone()),
                None,
                "value appears only on the left",
            ));
        }
        for value in right_set.difference(&left_set) {
            result.right_only.push(item(
                dimension,
                None,
                Some(value.clone()),
                "value appears only on the right",
            ));
        }
        if !left_set.is_empty() && !right_set.is_empty() && left_set != right_set {
            result.differences.push(item(
                dimension,
                Some(left_set.iter().cloned().collect::<Vec<_>>().join("; ")),
                Some(right_set.iter().cloned().collect::<Vec<_>>().join("; ")),
                "declared values differ",
            ));
        }
        if left_set.is_empty() && right_set.is_empty() {
            result
                .unresolved
                .push(format!("no evidence for {dimension:?}"));
        }
    }
    result
}

fn item(
    dimension: ComparisonDimension,
    left: Option<String>,
    right: Option<String>,
    explanation: &str,
) -> ComparisonItem {
    ComparisonItem {
        dimension,
        left,
        right,
        explanation: explanation.into(),
        confidence: 1.0,
        evidence: Vec::new(),
    }
}

fn values(
    left: &AtlasEntity,
    right: &AtlasEntity,
    dimension: ComparisonDimension,
) -> (Vec<String>, Vec<String>) {
    match dimension {
        ComparisonDimension::Purpose | ComparisonDimension::Responsibility => {
            (left.purpose_clauses.clone(), right.purpose_clauses.clone())
        }
        ComparisonDimension::Capabilities | ComparisonDimension::Interfaces => {
            (left.capabilities.clone(), right.capabilities.clone())
        }
        ComparisonDimension::ProtectedDecisions => (
            left.protected_decision_ids
                .iter()
                .map(|id| id.0.to_string())
                .collect(),
            right
                .protected_decision_ids
                .iter()
                .map(|id| id.0.to_string())
                .collect(),
        ),
        ComparisonDimension::Risks => (left.uncertainty.clone(), right.uncertainty.clone()),
        ComparisonDimension::Architecture | ComparisonDimension::Implementation => (
            vec![left.deterministic_summary.clone()]
                .into_iter()
                .filter(|s: &String| !s.is_empty())
                .collect(),
            vec![right.deterministic_summary.clone()]
                .into_iter()
                .filter(|s: &String| !s.is_empty())
                .collect(),
        ),
        ComparisonDimension::Verification => (
            left.green
                .iter()
                .map(|g| format!("{:?}", g.overall))
                .collect(),
            right
                .green
                .iter()
                .map(|g| format!("{:?}", g.overall))
                .collect(),
        ),
        ComparisonDimension::Dependencies => (Vec::new(), Vec::new()),
    }
}
