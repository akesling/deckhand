//! Bake terminal "screenshots" into a deck at compile time.
//!
//! `deckhand compile` runs every terminal block in a real PTY (by
//! default for local decks; `--snapshots` opts in for remote ones,
//! `--no-snapshots` opts out), waits for output to settle, and captures
//! the vt100 screen as replayable ANSI bytes (`contents_formatted`).
//! Contexts that can't spawn PTYs — the web presenter — replay the
//! capture instead of showing a placeholder. Native presenting always
//! runs live and ignores snapshots.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::deck::{Deck, TermSnapshot};
use crate::term::TermSession;

/// Default [`Options::wait`], in milliseconds. The CLI's `--help`
/// shows this same value via `cli`'s `default_value_t`.
pub const DEFAULT_WAIT_MS: u64 = 1500;
/// Default [`Options::cols`], shared with the CLI default.
pub const DEFAULT_COLS: u16 = 80;

/// Capture settings for [`capture`].
pub struct Options {
    /// How long to let each command run before capturing.
    pub wait: Duration,
    /// Capture width; height comes from each block (`fill` captures 24).
    pub cols: u16,
    /// Working directory for captured commands — controls what shell
    /// prompts display and what relative paths resolve against. `None`
    /// uses the deck's directory, matching live presenting.
    pub root: Option<std::path::PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            wait: Duration::from_millis(DEFAULT_WAIT_MS),
            cols: DEFAULT_COLS,
            root: None,
        }
    }
}

/// Rows used when capturing a fill-height terminal, which has no fixed
/// height of its own.
const FILL_CAPTURE_ROWS: u16 = 24;

/// Run every terminal block in the deck and store what its screen
/// looked like after [`Options::wait`]. Blocks that fail to spawn warn
/// and stay capture-less rather than failing the whole pass.
pub fn capture(deck: &mut Deck, deck_dir: &Path, opts: &Options) -> Result<()> {
    let cwd = match &opts.root {
        Some(root) => root
            .canonicalize()
            .with_context(|| format!("snapshot root {}", root.display()))?,
        None => deck_dir.to_path_buf(),
    };
    let cwd = cwd.as_path();
    for block in deck.terminals_mut() {
        let label = block.command.as_deref().unwrap_or("shell");
        eprintln!("snapshotting terminal {}: {label}", block.id + 1);
        let rows = if block.fill {
            FILL_CAPTURE_ROWS
        } else {
            block.rows
        };
        // A block that won't spawn shouldn't sink the compile —
        // it just keeps presenting as a placeholder.
        let mut session = match TermSession::spawn(block.command.as_deref(), rows, opts.cols, cwd) {
            Ok(session) => session,
            Err(e) => {
                eprintln!("warning: couldn't snapshot {label:?}: {e:#}");
                continue;
            }
        };
        std::thread::sleep(opts.wait);
        let data = {
            let parser = session.parser.lock().unwrap();
            parser.screen().contents_formatted()
        };
        session.kill();
        block.snapshot = Some(TermSnapshot {
            cols: opts.cols,
            rows,
            data,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::Segment;

    /// Spawns a real PTY; run explicitly with `cargo test -- --ignored`.
    #[test]
    #[ignore = "spawns a real PTY"]
    fn captures_command_output() {
        let mut deck =
            crate::deck::parse("```terminal rows=4\nprintf snap-test; sleep 5\n```\n", "t")
                .unwrap();
        let opts = Options {
            wait: Duration::from_millis(800),
            cols: 40,
            root: None,
        };
        capture(&mut deck, Path::new("."), &opts).unwrap();
        let Segment::Terminal(block) = &deck.slide(0, 0).segments[0] else {
            panic!("expected terminal");
        };
        let snap = block.snapshot.as_ref().unwrap();
        assert_eq!(snap.cols, 40);
        let text = String::from_utf8_lossy(&snap.data);
        assert!(text.contains("snap-test"), "capture missing output");
    }
}
