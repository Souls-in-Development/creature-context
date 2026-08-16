//! A reporting seam for a long index, so the CLI can show what a scan is doing
//! without the library deciding how — or whether — that is rendered.
//!
//! The pipeline takes an `Option<&dyn ScanProgress>`: a one-shot CLI `scan`
//! passes a reporter that draws to the terminal, and the resident daemon (and
//! every test) passes `None`. There is deliberately no no-op implementation —
//! the absence of reporting is `None`, not an object that implements the trait
//! by doing nothing. Every implementation of this trait therefore actually
//! reports.
//!
//! `Sync` is required because the parse phase reports from worker threads; an
//! implementation that mutates must guard its own state.

/// The coarse phases of an index, in the order the pipeline reaches them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanStage {
    /// Walking the tree and building the containment hierarchy.
    Tree,
    /// Projecting the module map from the tree.
    ModuleMap,
    /// Parsing files into symbols — the per-folder detail pass.
    Folders,
    /// Projecting Green onto native file metadata.
    Tagging,
}

impl ScanStage {
    /// A stable lower-case label, for a reporter that prints the stage name.
    pub fn label(self) -> &'static str {
        match self {
            ScanStage::Tree => "tree",
            ScanStage::ModuleMap => "module-map",
            ScanStage::Folders => "folders",
            ScanStage::Tagging => "tagging",
        }
    }
}

/// Where an index reports its progress. An implementation renders however it
/// likes; the pipeline only announces stages and per-file ticks.
pub trait ScanProgress: Sync {
    /// A new stage has begun. `detail` is a short human note (a count, a name) or
    /// empty.
    fn stage(&self, stage: ScanStage, detail: &str);

    /// Progress within the current stage: `done` of `total` units complete.
    /// Called frequently (once per parsed file), possibly from several threads,
    /// so an implementation must be cheap and its own writes serialised.
    fn tick(&self, done: usize, total: usize);

    /// A named unit of the current stage has been reached — a top-level folder as
    /// the layered scan enters it. `index` is 1-based over `total` such units.
    fn unit(&self, name: &str, index: usize, total: usize);
}
