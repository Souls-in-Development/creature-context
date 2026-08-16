//! `compile`: run the project's build and record what the compiler actually says
//! as Integration evidence — so the atlas's Green comes from real compilation
//! (a file that builds goes green, a file with errors goes red) instead of
//! staying `Unknown` for want of proof. This is the compiler as the arbiter of
//! integration: "if it builds, it works."
//!
//! Diagnostics are read from Cargo's `--message-format=json` stream (the tool's
//! own language, and the one that reports per-file spans). A `--command` override
//! runs any build; without per-file JSON it can only record the overall pass/fail
//! against every source file. Evidence is written both onto the live snapshot
//! (immediate Green update) and to `.creature/evidence.json` (so it survives the
//! next scan, the same durable store the `evidence` command uses).

use creature_context_core::green::evaluate_snapshot;
use creature_context_core::project::{ProjectPaths, atomic_write, load_identity};
use creature_context_core::scan::current_rfc3339;
use creature_context_store::{AtlasRepository, write_projections};
use creature_context_types::*;
use std::collections::BTreeSet;
use std::error::Error;
use std::path::Path;
use std::process::Command;

const PRODUCER: &str = "compiler";

pub fn run_compile(root: &Path, command: Option<String>) -> Result<(), Box<dyn Error>> {
    // Decide the build command; auto-detect Cargo when none is given.
    let (parts, cargo_json): (Vec<String>, bool) = match command {
        Some(c) => {
            let is_cargo = c.split_whitespace().next() == Some("cargo");
            (c.split_whitespace().map(String::from).collect(), is_cargo)
        }
        None => {
            if root.join("Cargo.toml").exists() {
                (
                    ["cargo", "build", "--message-format=json", "--quiet"]
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                    true,
                )
            } else {
                return Err("no --command given and no Cargo.toml to auto-detect a build".into());
            }
        }
    };
    // Cargo only emits per-file JSON diagnostics when asked; add the flag if the
    // caller's cargo command omitted it.
    let mut parts = parts;
    let cargo_json = if cargo_json && !parts.iter().any(|p| p.starts_with("--message-format")) {
        parts.push("--message-format=json".to_string());
        true
    } else {
        cargo_json
    };
    let (program, args) = parts.split_first().ok_or("empty build command")?;

    eprintln!("compile: running `{}` …", parts.join(" "));
    let output = Command::new(program).args(args).current_dir(root).output()?;
    let success = output.status.success();

    // Per-file diagnostics from Cargo's JSON stream (primary spans only).
    let mut errors: BTreeSet<String> = BTreeSet::new();
    let mut warnings: BTreeSet<String> = BTreeSet::new();
    if cargo_json {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
                continue;
            }
            let msg = &v["message"];
            let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("");
            let bucket = match level {
                "error" => &mut errors,
                "warning" => &mut warnings,
                _ => continue,
            };
            if let Some(spans) = msg.get("spans").and_then(|s| s.as_array()) {
                for span in spans {
                    if span.get("is_primary").and_then(|p| p.as_bool()) == Some(true) {
                        if let Some(f) = span.get("file_name").and_then(|f| f.as_str()) {
                            bucket.insert(f.to_string());
                        }
                    }
                }
            }
        }
    }

    // Record Integration evidence per source file.
    let paths = ProjectPaths::new(root);
    let mut repository = AtlasRepository::open(&paths.database)?;
    let mut snapshot = repository.load_snapshot()?;
    let now = current_rfc3339();
    let snap_id = snapshot.id.clone();
    let fingerprint = snapshot.id.0.clone();

    let mut recorded: Vec<RecordedEvidence> = Vec::new();
    let (mut n_pass, mut n_fail, mut n_warn) = (0usize, 0usize, 0usize);
    for entity in snapshot.entities.iter_mut() {
        if entity.kind != EntityKind::File {
            continue;
        }
        let Some(path) = entity.relative_path.clone() else {
            continue;
        };
        let outcome = if errors.contains(&path) {
            n_fail += 1;
            EvidenceOutcome::Fail
        } else if warnings.contains(&path) {
            n_warn += 1;
            EvidenceOutcome::Warning
        } else if success {
            n_pass += 1;
            EvidenceOutcome::Pass
        } else {
            // A failed build says nothing certain about a file with no error of
            // its own — leave it Unknown rather than claim a pass.
            continue;
        };
        let evidence = Evidence {
            axis: GreenAxis::Integration,
            source: FactSource::Observed,
            proof: ProofStrength::Build,
            outcome,
            confidence: 1.0,
            fingerprint: fingerprint.clone(),
            observed_at: now.clone(),
            producer: PRODUCER.to_string(),
            snapshot_id: snap_id.clone(),
            message: String::new(),
        };
        // Replace any prior compiler evidence on this axis for this file.
        entity
            .local_evidence
            .retain(|e| !(e.axis == GreenAxis::Integration && e.producer == PRODUCER));
        entity.local_evidence.push(evidence.clone());
        recorded.push(RecordedEvidence {
            entity_id: entity.id,
            evidence,
        });
    }

    evaluate_snapshot(&mut snapshot, &GreenPolicy::default())?;
    repository.replace_snapshot(&snapshot)?;
    write_projections(root, &snapshot, &load_identity(root)?.project_id)?;

    // Merge into the durable evidence log so it survives the next scan (dedupe by
    // entity + axis + producer + snapshot, same key the `evidence` command uses).
    let mut log: Vec<RecordedEvidence> = if paths.evidence.exists() {
        serde_json::from_slice(&std::fs::read(&paths.evidence)?)?
    } else {
        Vec::new()
    };
    for rec in &recorded {
        log.retain(|r| {
            !(r.entity_id == rec.entity_id
                && r.evidence.axis == rec.evidence.axis
                && r.evidence.producer == rec.evidence.producer
                && r.evidence.snapshot_id == rec.evidence.snapshot_id)
        });
    }
    log.extend(recorded.iter().cloned());
    log.sort_by_key(|r| (r.entity_id, r.evidence.axis, r.evidence.producer.clone()));
    atomic_write(&paths.evidence, &serde_json::to_vec_pretty(&log)?)?;

    println!(
        "compile: build {} — integration evidence: {} pass, {} fail, {} warn",
        if success { "OK" } else { "FAILED" },
        n_pass,
        n_fail,
        n_warn
    );
    Ok(())
}
