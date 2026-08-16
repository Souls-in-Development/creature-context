//! The terminal reporter for a one-shot `scan`.
//!
//! It draws only when standard error is a terminal, so a piped or redirected run
//! (`scan … | jq`, a CI log, the daemon) stays silent and leaves stdout — which
//! carries the JSON/YAML/IDX payload — untouched. Progress is a courtesy for a
//! human watching a long scan, never part of the machine contract.
//!
//! Every write goes to stderr under a single lock, so the ticks arriving from the
//! parser's worker threads cannot interleave into a garbled line. Redraws are
//! throttled to whole-percent changes, so a 300k-file parse repaints ~100 times
//! rather than 300k.

use creature_context_types::{ScanProgress, ScanStage};
use std::io::{IsTerminal, Write};
use std::sync::Mutex;

/// Terminal width to devote to the bar itself, between the brackets.
const BAR_WIDTH: usize = 24;

pub struct TtyProgress {
    enabled: bool,
    state: Mutex<State>,
}

struct State {
    stage: ScanStage,
    last_percent: i32,
}

impl TtyProgress {
    pub fn new() -> Self {
        Self {
            enabled: std::io::stderr().is_terminal(),
            state: Mutex::new(State {
                stage: ScanStage::Tree,
                last_percent: -1,
            }),
        }
    }
}

impl ScanProgress for TtyProgress {
    fn stage(&self, stage: ScanStage, detail: &str) {
        if !self.enabled {
            return;
        }
        let mut state = self.state.lock().expect("progress state");
        state.stage = stage;
        state.last_percent = -1;
        let mut err = std::io::stderr().lock();
        // Clear any in-progress bar on the current line, then announce the stage.
        let _ = write!(err, "\r\x1b[K\u{2023} {}", stage.label());
        if !detail.is_empty() {
            let _ = write!(err, " ({detail})");
        }
        let _ = writeln!(err);
        let _ = err.flush();
    }

    fn tick(&self, done: usize, total: usize) {
        if !self.enabled || total == 0 {
            return;
        }
        let percent = (done * 100 / total) as i32;
        let mut state = self.state.lock().expect("progress state");
        // Repaint only when the whole-percent figure moves, plus the final tick.
        if percent == state.last_percent && done != total {
            return;
        }
        state.last_percent = percent;
        let filled = percent.max(0) as usize * BAR_WIDTH / 100;
        let bar: String = (0..BAR_WIDTH)
            .map(|cell| if cell < filled { '=' } else { ' ' })
            .collect();
        let mut err = std::io::stderr().lock();
        let _ = write!(
            err,
            "\r\x1b[K  {} [{}] {percent:3}% ({done}/{total})",
            state.stage.label(),
            bar,
        );
        // Leave a finished stage on its own line rather than overwriting it.
        if done == total {
            let _ = writeln!(err);
        }
        let _ = err.flush();
    }

    fn unit(&self, name: &str, index: usize, total: usize) {
        if !self.enabled {
            return;
        }
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "\r\x1b[K  \u{b7} [{index}/{total}] {name}");
        let _ = err.flush();
    }
}
