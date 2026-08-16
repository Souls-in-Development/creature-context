pub mod commands;
pub mod output;
pub mod progress;
pub mod request;

use clap::{Parser, Subcommand, ValueEnum};
use creature_context_core::{
    green::evaluate_snapshot,
    orbit::compile_orbit,
    project::{ProjectPaths, atomic_write, init_project},
    scan::current_rfc3339,
};
use creature_context_store::{AtlasRepository, write_projections};
use creature_context_types::*;
use output::{OutputFormat, write_output, write_output_generic};
use serde::Serialize;
use std::{
    error::Error,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "creature-context",
    version,
    about = "Multiscale repository context for any coding platform"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init {
        path: PathBuf,
        #[arg(long, value_enum, default_value = "idx")]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        compatibility: bool,
    },
    Scan {
        path: PathBuf,
        #[arg(long, value_enum, default_value = "idx")]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        compatibility: bool,
    },
    /// Index and persist one top-level folder at a time, holding only the file
    /// tree in memory rather than the whole enriched graph.
    ScanLayered { path: PathBuf },
    /// Render the current snapshot's green-pixels texture to ATLAS.png.
    Map {
        path: PathBuf,
        /// Circle-pack the atlas tree (galaxy) instead of the Hilbert square.
        #[arg(long)]
        galaxy: bool,
        /// Colour by a single Green axis (e.g. `integration` — the compiler's
        /// verdict) instead of overall health. Galaxy layout only.
        #[arg(long)]
        axis: Option<String>,
    },
    /// Print the tree-ordered, role-tagged module map (code / data / docs).
    Modules { path: PathBuf },
    /// Run the project's build and record what the compiler says as Integration
    /// evidence — files that build go Green, files with errors go Red.
    Compile {
        path: PathBuf,
        /// Build command to run (default: auto-detect `cargo build` when a
        /// Cargo.toml is present). Cargo commands get `--message-format=json`.
        #[arg(long)]
        command: Option<String>,
    },
    Status {
        path: PathBuf,
        #[arg(long, value_enum, default_value = "idx")]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        compatibility: bool,
    },
    Atlas {
        path: PathBuf,
        reference: Option<String>,
        #[arg(long, value_enum)]
        scale: Option<ScaleArg>,
        #[arg(long, value_enum, default_value = "idx")]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        compatibility: bool,
    },
    Green {
        path: PathBuf,
        reference: Option<String>,
        #[arg(long, default_value_t = false)]
        explain: bool,
        #[arg(long, value_enum, default_value = "idx")]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        compatibility: bool,
    },
    Evidence {
        path: PathBuf,
        reference: String,
        #[arg(long, value_enum)]
        axis: AxisArg,
        #[arg(long, value_enum)]
        proof: ProofArg,
        #[arg(long, value_enum, default_value = "pass")]
        outcome: OutcomeArg,
        #[arg(long, value_enum, default_value = "observed")]
        source: SourceArg,
        #[arg(long)]
        producer: String,
        #[arg(long, default_value = "")]
        message: String,
        #[arg(long, default_value_t = false)]
        recursive: bool,
        #[arg(long, value_enum, default_value = "idx")]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        compatibility: bool,
    },
    Orbit {
        path: PathBuf,
        #[arg(long)]
        task: String,
        #[arg(long, value_enum, default_value = "adaptive")]
        scale: OrbitScaleArg,
        #[arg(long, value_enum, default_value = "focus")]
        mode: OrbitModeArg,
        #[arg(long, default_value_t = 64_000)]
        budget: usize,
        #[arg(long)]
        focus: Vec<String>,
        #[arg(long, default_value_t = false)]
        include_inferred: bool,
        #[arg(long, value_enum, default_value = "idx")]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        compatibility: bool,
    },
    Compare {
        path: PathBuf,
        left: String,
        right: String,
        #[arg(long, value_enum, default_value = "planet")]
        scale: OrbitScaleArg,
        #[arg(long, value_enum)]
        dimension: Vec<DimensionArg>,
        #[arg(long, default_value_t = 64_000)]
        budget: usize,
        #[arg(long, default_value_t = false)]
        include_inferred: bool,
        #[arg(long, value_enum, default_value = "idx")]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        compatibility: bool,
    },
    Context {
        path: PathBuf,
        #[arg(long)]
        request: Option<String>,
        #[arg(long, value_enum, default_value = "idx")]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        compatibility: bool,
    },
    Rebuild {
        path: PathBuf,
        #[arg(long)]
        database: Option<PathBuf>,
    },
    Permission {
        #[command(subcommand)]
        command: PermissionCommand,
    },
    Register {
        path: PathBuf,
        project_name: String,
        target_path: String,
    },
    Run {
        /// One or more project roots. A single daemon watches all of them.
        #[arg(required = true, num_args = 1..)]
        path: Vec<PathBuf>,
    },
    /// Register the resident daemon with the OS supervisor, so it runs in the
    /// background across logins rather than only while a terminal is open.
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    Ingest {
        path: PathBuf,
        /// The activity source: git, build, test, diagnostic, and so on.
        #[arg(long)]
        kind: String,
        /// The activity detail carried as the event payload.
        #[arg(long)]
        message: String,
    },
}

#[derive(Subcommand)]
pub enum ServiceCommand {
    /// Install and start the background daemon for a project.
    Install {
        #[arg(required = true, num_args = 1..)]
        path: Vec<PathBuf>,
    },
    /// Stop and deregister the background daemon. Pass the same roots that were
    /// installed — the registration is identified by the whole set.
    Uninstall {
        #[arg(required = true, num_args = 1..)]
        path: Vec<PathBuf>,
    },
    /// Report whether the daemon is registered and running.
    Status {
        #[arg(required = true, num_args = 1..)]
        path: Vec<PathBuf>,
    },
    /// Print the supervisor definition without installing it.
    Show {
        #[arg(required = true, num_args = 1..)]
        path: Vec<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum PermissionCommand {
    Allow {
        path: PathBuf,
        #[arg(long)]
        subject: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        resource: String,
        #[arg(long)]
        scope: String,
    },
    Deny {
        path: PathBuf,
        #[arg(long)]
        subject: String,
        #[arg(long)]
        action: String,
        #[arg(long)]
        resource: String,
        #[arg(long)]
        scope: String,
    },
    Supersede {
        path: PathBuf,
        old_id: String,
        #[arg(long)]
        with: String,
    },
    List {
        path: PathBuf,
        #[arg(long, value_enum, default_value = "idx")]
        format: OutputFormat,
        #[arg(long, default_value_t = false)]
        compatibility: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum ScaleArg {
    Universe,
    Galaxy,
    System,
    Planet,
    Moon,
}

#[derive(Clone, Copy, ValueEnum)]
enum OrbitScaleArg {
    Universe,
    Galaxy,
    System,
    Planet,
    Moon,
    Adaptive,
}

#[derive(Clone, Copy, ValueEnum)]
enum OrbitModeArg {
    Design,
    Focus,
    Trace,
    Compare,
    Health,
    Change,
}

#[derive(Clone, Copy, ValueEnum)]
enum DimensionArg {
    Purpose,
    Responsibility,
    Architecture,
    Capabilities,
    Dependencies,
    Interfaces,
    Implementation,
    Verification,
    ProtectedDecisions,
    Risks,
}

#[derive(Clone, Copy, ValueEnum)]
enum AxisArg {
    Content,
    Structure,
    Integration,
    Verification,
    Freshness,
    Coherence,
}

#[derive(Clone, Copy, ValueEnum)]
enum ProofArg {
    Unknown,
    Metadata,
    Syntax,
    Lint,
    Typecheck,
    Build,
    Test,
    Human,
}

#[derive(Clone, Copy, ValueEnum)]
enum OutcomeArg {
    Unknown,
    Pass,
    Warning,
    Fail,
}

#[derive(Clone, Copy, ValueEnum)]
enum SourceArg {
    Declared,
    Parsed,
    Observed,
    Inferred,
    Human,
}

#[derive(Serialize)]
pub struct Status<'a> {
    snapshot_id: &'a SnapshotId,
    entities: usize,
    relationships: usize,
    green: usize,
    yellow: usize,
    red: usize,
    unknown: usize,
}

impl<'a> creature_context_store::idx::IdxRenderable for Status<'a> {
    fn render_idx(&self) -> Result<String, creature_context_store::idx::IdxError> {
        Ok(format!(
            "@status entities:{} relationships:{}",
            self.entities, self.relationships
        ))
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("creature-context: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::Init {
            path,
            format,
            compatibility,
        } => {
            let identity = init_project(&path)?;
            write_output_generic(&identity, format, compatibility)?;
        }
        Command::Scan {
            path,
            format,
            compatibility,
        } => {
            let paths = ProjectPaths::new(&path);
            // Ensure the store directory exists before opening the database. The
            // previous scan-first ordering created it as a side effect; opening
            // the repository first means a `scan` on a not-yet-initialised project
            // must create it here.
            if let Some(dir) = paths.database.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let mut repository = AtlasRepository::open(&paths.database)?;
            // The shared index pipeline: scan, enrich with parsed structure,
            // reconcile identity against the previous snapshot, evaluate Green.
            // The resident daemon runs the exact same function, so a scanned and a
            // watched Atlas are identical.
            let previous = repository.load_snapshot().ok();
            // Report tree and per-file parse progress to the terminal; silent when
            // stderr is not a tty, so the stdout payload stays clean.
            let reporter = progress::TtyProgress::new();
            let snapshot = creature_context_parsers::index::index_project_reported(
                &path,
                previous.as_ref(),
                Some(&reporter),
            )?;
            repository.replace_snapshot(&snapshot)?;
            reporter.stage(ScanStage::ModuleMap, "");
            write_projections(
                &path,
                &snapshot,
                &creature_context_core::project::load_identity(&path)?.project_id,
            )?;
            // Project Green onto native file metadata (Finder tags on macOS). It
            // is reversible and rebuildable from the Atlas; a no-op where no
            // adapter exists.
            reporter.stage(ScanStage::Tagging, "");
            creature_context_runtime::metadata::apply(&path, &snapshot);
            append_journal(&paths.journal, &snapshot)?;
            write_output_generic(&status(&snapshot), format, compatibility)?;
        }
        Command::ScanLayered { path } => {
            let reporter = progress::TtyProgress::new();
            let layered = commands::scan_layered::scan_layered(&path, Some(&reporter))?;
            eprintln!(
                "layered scan: {} directories, {} entities, {} relationships",
                layered.directories, layered.entities, layered.relationships
            );
            println!("{}", layered.snapshot_id.0);
        }
        Command::Compile { path, command } => {
            commands::compile::run_compile(&path, command)?;
        }
        Command::Map { path, galaxy, axis } => {
            commands::map::run_map(&path, galaxy, axis.as_deref())?;
        }
        Command::Modules { path } => {
            let paths = ProjectPaths::new(&path);
            let repository = AtlasRepository::open(&paths.database)?;
            let snapshot = repository.load_snapshot()?;
            let lines = creature_context_core::overview::module_overview(&snapshot);
            print!("{}", creature_context_core::overview::render_overview(&lines));
        }
        Command::Status {
            path,
            format,
            compatibility,
        } => {
            let snapshot = load_snapshot(&path)?;
            write_output_generic(&status(&snapshot), format, compatibility)?;
        }
        Command::Atlas {
            path,
            reference,
            scale,
            format,
            compatibility,
        } => {
            let snapshot = load_snapshot(&path)?;
            let mut entities: Vec<_> = snapshot
                .entities
                .iter()
                .filter(|entity| {
                    reference
                        .as_ref()
                        .is_none_or(|value| matches_ref(entity, value))
                        && scale.is_none_or(|value| entity.scale == value.into())
                })
                .collect();
            entities.sort_by_key(|entity| {
                (
                    entity.scale.rank(),
                    entity.canonical_name.to_lowercase(),
                    entity.id,
                )
            });
            write_output_generic(&entities, format, compatibility)?;
        }
        Command::Green {
            path,
            reference,
            explain,
            format,
            compatibility,
        } => {
            let snapshot = load_snapshot(&path)?;
            let mut assessments: Vec<_> = snapshot.entities.iter().filter(|entity| reference.as_ref().is_none_or(|value| matches_ref(entity, value))).map(|entity| {
                if explain { serde_json::json!({"id": entity.id, "name": entity.canonical_name, "scale": entity.scale, "green": entity.green}) }
                else { serde_json::json!({"id": entity.id, "name": entity.canonical_name, "scale": entity.scale, "status": entity.green.as_ref().map(|g| g.overall)}) }
            }).collect();
            assessments.sort_by_key(|value| {
                value
                    .get("name")
                    .and_then(|name| name.as_str())
                    .unwrap_or_default()
                    .to_owned()
            });
            write_output_generic(&assessments, format, compatibility)?;
        }
        Command::Evidence {
            path,
            reference,
            axis,
            proof,
            outcome,
            source,
            producer,
            message,
            recursive,
            format,
            compatibility,
        } => {
            let mut snapshot = load_snapshot(&path)?;
            let root_id = resolve_ref(&snapshot, &reference)?;
            let mut targets = std::collections::BTreeSet::from([root_id]);
            if recursive {
                loop {
                    let before = targets.len();
                    for edge in &snapshot.edges {
                        if edge.kind == RelationshipKind::Contains
                            && targets.contains(&edge.source_entity_id)
                        {
                            targets.insert(edge.target_entity_id);
                        }
                    }
                    if targets.len() == before {
                        break;
                    }
                }
            }
            let paths = ProjectPaths::new(&path);
            let mut recorded: Vec<RecordedEvidence> = if paths.evidence.exists() {
                serde_json::from_slice(&std::fs::read(&paths.evidence)?)?
            } else {
                Vec::new()
            };
            for entity_id in targets {
                let evidence = creature_context_types::Evidence {
                    axis: axis.into(),
                    source: source.into(),
                    proof: proof.into(),
                    outcome: outcome.into(),
                    confidence: 1.0,
                    fingerprint: snapshot.id.0.clone(),
                    observed_at: current_rfc3339(),
                    producer: producer.clone(),
                    snapshot_id: snapshot.id.clone(),
                    message: message.clone(),
                };
                recorded.retain(|record| {
                    !(record.entity_id == entity_id
                        && record.evidence.axis == evidence.axis
                        && record.evidence.producer == evidence.producer
                        && record.evidence.snapshot_id == evidence.snapshot_id)
                });
                recorded.push(RecordedEvidence {
                    entity_id,
                    evidence: evidence.clone(),
                });
                if let Some(entity) = snapshot
                    .entities
                    .iter_mut()
                    .find(|entity| entity.id == entity_id)
                {
                    entity.local_evidence.push(evidence);
                }
            }
            recorded.sort_by_key(|record| {
                (
                    record.entity_id,
                    record.evidence.axis,
                    record.evidence.producer.clone(),
                )
            });
            atomic_write(&paths.evidence, &serde_json::to_vec_pretty(&recorded)?)?;
            evaluate_snapshot(&mut snapshot, &GreenPolicy::default())?;
            let mut repository = AtlasRepository::open(&paths.database)?;
            repository.replace_snapshot(&snapshot)?;
            write_projections(
                &path,
                &snapshot,
                &creature_context_core::project::load_identity(&path)?.project_id,
            )?;
            write_output_generic(&status(&snapshot), format, compatibility)?;
        }
        Command::Orbit {
            path,
            task,
            scale,
            mode,
            budget,
            focus,
            include_inferred,
            format,
            compatibility,
        } => {
            let snapshot = load_snapshot(&path)?;
            let focus: Vec<EntityId> = focus
                .iter()
                .map(|value| resolve_ref(&snapshot, value))
                .collect::<Result<_, _>>()?;
            let request = OrbitRequest {
                task,
                target_references: focus
                    .into_iter()
                    .map(|f| EntityReference {
                        stable_id: Some(f),
                        relative_path: None,
                        symbol: None,
                    })
                    .collect(),
                scale: scale.into(),
                mode: mode.into(),
                inferred_policy: if include_inferred {
                    InferredPolicy::IncludeAttributed
                } else {
                    InferredPolicy::Exclude
                },
                token_budget: budget,
                ..OrbitRequest::default()
            };
            write_output(&compile_orbit(&snapshot, &request)?, format, compatibility)?;
        }
        Command::Compare {
            path,
            left,
            right,
            scale,
            dimension,
            budget,
            include_inferred,
            format,
            compatibility,
        } => {
            let snapshot = load_snapshot(&path)?;
            let request = OrbitRequest {
                task: format!("Compare {left} with {right}"),
                mode: OrbitMode::Compare,
                scale: scale.into(),
                target_references: vec![
                    EntityReference {
                        stable_id: Some(resolve_ref(&snapshot, &left)?),
                        relative_path: None,
                        symbol: None,
                    },
                    EntityReference {
                        stable_id: Some(resolve_ref(&snapshot, &right)?),
                        relative_path: None,
                        symbol: None,
                    },
                ],
                comparison_dimensions: dimension.into_iter().map(Into::into).collect(),
                inferred_policy: if include_inferred {
                    InferredPolicy::IncludeAttributed
                } else {
                    InferredPolicy::Exclude
                },
                token_budget: budget,
                ..OrbitRequest::default()
            };
            write_output(&compile_orbit(&snapshot, &request)?, format, compatibility)?;
        }
        Command::Context {
            path,
            request,
            format,
            compatibility,
        } => {
            commands::context::handle_context(path, request, format, compatibility)?;
        }
        Command::Rebuild { path, database } => {
            commands::rebuild::handle_rebuild(path, database)?;
        }
        Command::Permission { command } => match command {
            PermissionCommand::Allow {
                path,
                subject,
                action,
                resource,
                scope,
            } => {
                commands::permission::handle_allow(path, subject, action, resource, scope)?;
            }
            PermissionCommand::Deny {
                path,
                subject,
                action,
                resource,
                scope,
            } => {
                commands::permission::handle_deny(path, subject, action, resource, scope)?;
            }
            PermissionCommand::Supersede { path, old_id, with } => {
                commands::permission::handle_supersede(path, old_id, with)?;
            }
            PermissionCommand::List {
                path,
                format,
                compatibility,
            } => {
                commands::permission::handle_list(path, format, compatibility)?;
            }
        },
        Command::Register {
            path,
            project_name,
            target_path,
        } => {
            commands::register::handle_register(path, project_name, target_path)?;
        }
        Command::Run { path } => {
            commands::run::handle_run(path)?;
        }
        Command::Service { command } => match command {
            ServiceCommand::Install { path } => commands::service::handle_install(path)?,
            ServiceCommand::Uninstall { path } => commands::service::handle_uninstall(path)?,
            ServiceCommand::Status { path } => commands::service::handle_status(path)?,
            ServiceCommand::Show { path } => commands::service::handle_show(path)?,
        },
        Command::Ingest {
            path,
            kind,
            message,
        } => {
            commands::ingest::handle_ingest(path, kind, message)?;
        }
    }
    Ok(())
}

fn load_snapshot(root: &Path) -> Result<AtlasSnapshot, Box<dyn Error>> {
    Ok(AtlasRepository::open(&ProjectPaths::new(root).database)?.load_snapshot()?)
}

fn status(snapshot: &AtlasSnapshot) -> Status<'_> {
    let mut counts = [0usize; 4];
    for entity in &snapshot.entities {
        match entity
            .green
            .as_ref()
            .map(|g| g.overall)
            .unwrap_or(GreenCode::Unknown)
        {
            GreenCode::Green => counts[0] += 1,
            GreenCode::Yellow => counts[1] += 1,
            GreenCode::Red => counts[2] += 1,
            GreenCode::Unknown => counts[3] += 1,
        }
    }
    Status {
        snapshot_id: &snapshot.id,
        entities: snapshot.entities.len(),
        relationships: snapshot.edges.len(),
        green: counts[0],
        yellow: counts[1],
        red: counts[2],
        unknown: counts[3],
    }
}

fn resolve_ref(snapshot: &AtlasSnapshot, value: &str) -> Result<EntityId, String> {
    snapshot
        .entities
        .iter()
        .find(|entity| matches_ref(entity, value))
        .map(|entity| entity.id)
        .ok_or_else(|| format!("reference not found: {value}"))
}

fn matches_ref(entity: &AtlasEntity, value: &str) -> bool {
    entity.id.to_string() == value
        || entity.canonical_name.eq_ignore_ascii_case(value)
        || entity.relative_path.as_deref() == Some(value)
        || entity.aliases.iter().any(|alias| alias == value)
}

fn append_journal(path: &Path, snapshot: &AtlasSnapshot) -> Result<(), Box<dyn Error>> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(
        file,
        "{}",
        serde_json::json!({"event": "scan", "snapshot_id": snapshot.id, "entities": snapshot.entities.len(), "relationships": snapshot.edges.len()})
    )?;
    file.sync_all()?;
    Ok(())
}

impl From<ScaleArg> for ScopeScale {
    fn from(value: ScaleArg) -> Self {
        match value {
            ScaleArg::Universe => Self::Universe,
            ScaleArg::Galaxy => Self::Galaxy,
            ScaleArg::System => Self::System,
            ScaleArg::Planet => Self::Planet,
            ScaleArg::Moon => Self::Moon,
        }
    }
}
impl From<OrbitScaleArg> for OrbitScale {
    fn from(value: OrbitScaleArg) -> Self {
        match value {
            OrbitScaleArg::Universe => Self::Universe,
            OrbitScaleArg::Galaxy => Self::Galaxy,
            OrbitScaleArg::System => Self::System,
            OrbitScaleArg::Planet => Self::Planet,
            OrbitScaleArg::Moon => Self::Moon,
            OrbitScaleArg::Adaptive => Self::Adaptive,
        }
    }
}
impl From<OrbitModeArg> for OrbitMode {
    fn from(value: OrbitModeArg) -> Self {
        match value {
            OrbitModeArg::Design => Self::Design,
            OrbitModeArg::Focus => Self::Focus,
            OrbitModeArg::Trace => Self::Trace,
            OrbitModeArg::Compare => Self::Compare,
            OrbitModeArg::Health => Self::Health,
            OrbitModeArg::Change => Self::Change,
        }
    }
}
impl From<DimensionArg> for ComparisonDimension {
    fn from(value: DimensionArg) -> Self {
        match value {
            DimensionArg::Purpose => Self::Purpose,
            DimensionArg::Responsibility => Self::Responsibility,
            DimensionArg::Architecture => Self::Architecture,
            DimensionArg::Capabilities => Self::Capabilities,
            DimensionArg::Dependencies => Self::Dependencies,
            DimensionArg::Interfaces => Self::Interfaces,
            DimensionArg::Implementation => Self::Implementation,
            DimensionArg::Verification => Self::Verification,
            DimensionArg::ProtectedDecisions => Self::ProtectedDecisions,
            DimensionArg::Risks => Self::Risks,
        }
    }
}

impl From<AxisArg> for GreenAxis {
    fn from(value: AxisArg) -> Self {
        match value {
            AxisArg::Content => Self::Content,
            AxisArg::Structure => Self::Structure,
            AxisArg::Integration => Self::Integration,
            AxisArg::Verification => Self::Verification,
            AxisArg::Freshness => Self::Freshness,
            AxisArg::Coherence => Self::Coherence,
        }
    }
}

impl From<ProofArg> for ProofStrength {
    fn from(value: ProofArg) -> Self {
        match value {
            ProofArg::Unknown => Self::Unknown,
            ProofArg::Metadata => Self::Metadata,
            ProofArg::Syntax => Self::Syntax,
            ProofArg::Lint => Self::Lint,
            ProofArg::Typecheck => Self::Typecheck,
            ProofArg::Build => Self::Build,
            ProofArg::Test => Self::Test,
            ProofArg::Human => Self::Human,
        }
    }
}

impl From<OutcomeArg> for EvidenceOutcome {
    fn from(value: OutcomeArg) -> Self {
        match value {
            OutcomeArg::Unknown => Self::Unknown,
            OutcomeArg::Pass => Self::Pass,
            OutcomeArg::Warning => Self::Warning,
            OutcomeArg::Fail => Self::Fail,
        }
    }
}

impl From<SourceArg> for FactSource {
    fn from(value: SourceArg) -> Self {
        match value {
            SourceArg::Declared => Self::Declared,
            SourceArg::Parsed => Self::Parsed,
            SourceArg::Observed => Self::Observed,
            SourceArg::Inferred => Self::Inferred,
            SourceArg::Human => Self::Human,
        }
    }
}
