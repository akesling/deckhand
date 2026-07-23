//! Command-line interface.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{compile, notes, present};

#[derive(Parser)]
#[command(
    name = "deckhand",
    version,
    about = "Present markdown slide decks as a TUI",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    /// Deck to present: a markdown file, a JSON manifest, a URL, or a
    /// GitHub gist (shorthand for `deckhand present <deck>`)
    deck: Option<String>,
    /// Unix socket path for presenter-notes sync
    #[arg(long)]
    socket: Option<PathBuf>,
    /// Don't live-reload when the deck's files change on disk
    #[arg(long)]
    no_watch: bool,
    /// Start terminal commands as soon as their slide appears (default:
    /// each terminal shows its command until run with t, then y)
    #[arg(long)]
    eager: bool,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Present a deck (a local file, a URL, or a GitHub gist). Local
    /// decks live-reload when their files change
    Present {
        deck: String,
        /// Unix socket path for presenter-notes sync
        #[arg(long)]
        socket: Option<PathBuf>,
        /// Don't live-reload when the deck's files change on disk
        #[arg(long)]
        no_watch: bool,
        /// Start terminal commands as soon as their slide appears
        /// (default: each shows its command until run with t, then y)
        #[arg(long)]
        eager: bool,
    },
    /// Follow presenter notes from a separate terminal
    Notes {
        /// Unix socket path the presenter is broadcasting on
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Flatten a deck (e.g. a JSON manifest) into one markdown file.
    /// Decks with terminal blocks require choosing --snapshots (runs
    /// their commands, bakes screen captures in) or --no-snapshots
    Compile {
        deck: String,
        /// Write here instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Run each terminal block in a PTY and bake its screen capture
        /// in (this RUNS the deck's commands — only for decks you trust)
        #[arg(long, conflicts_with = "no_snapshots")]
        snapshots: bool,
        /// Compile without running terminal blocks or baking captures
        #[arg(long)]
        no_snapshots: bool,
        /// How long to let each command run before capturing (ms)
        #[arg(long, default_value_t = 1500)]
        snapshot_wait_ms: u64,
        /// Terminal width for captures
        #[arg(long, default_value_t = 80)]
        snapshot_cols: u16,
        /// Working directory for captured commands (controls the prompt
        /// path etc.); defaults to the deck's directory
        #[arg(long)]
        snapshot_root: Option<PathBuf>,
    },
    /// Render a deck to a PDF file, one page per slide. Terminal blocks
    /// show their baked snapshots; decks with unsnapshotted terminals
    /// require choosing --snapshots (runs their commands, captures the
    /// output) or --no-snapshots
    #[cfg(feature = "pdf")]
    Pdf {
        deck: String,
        /// Write here (default: the deck's file name with .pdf)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Page grid width, in terminal columns
        #[arg(long, default_value_t = 100)]
        cols: u16,
        /// Page grid height, in terminal rows
        #[arg(long, default_value_t = 30)]
        rows: u16,
        /// Font size in points; with the grid, this sets the page size
        #[arg(long, default_value_t = 10.0)]
        font_size: f32,
        /// Run each terminal block in a PTY and render its screen
        /// capture (this RUNS the deck's commands — only for decks you
        /// trust)
        #[arg(long, conflicts_with = "no_snapshots")]
        snapshots: bool,
        /// Render terminal blocks without snapshots as placeholders
        #[arg(long)]
        no_snapshots: bool,
        /// How long to let each command run before capturing (ms)
        #[arg(long, default_value_t = 1500)]
        snapshot_wait_ms: u64,
        /// Terminal width for captures
        #[arg(long, default_value_t = 80)]
        snapshot_cols: u16,
        /// Working directory for captured commands (controls the prompt
        /// path etc.); defaults to the deck's directory
        #[arg(long)]
        snapshot_root: Option<PathBuf>,
    },
}

fn default_socket() -> PathBuf {
    std::env::temp_dir().join("deckhand.sock")
}

/// Parse arguments and dispatch to present / notes / compile.
pub fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Cmd::Present {
            deck,
            socket,
            no_watch,
            eager,
        }) => present::run(
            &deck,
            socket.unwrap_or_else(default_socket),
            !no_watch,
            eager,
        ),
        Some(Cmd::Notes { socket }) => notes::run(socket.unwrap_or_else(default_socket)),
        Some(Cmd::Compile {
            deck,
            output,
            snapshots,
            no_snapshots,
            snapshot_wait_ms,
            snapshot_cols,
            snapshot_root,
        }) => {
            let choice = match (snapshots, no_snapshots) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None, // compile errors if the deck has terminals
            };
            let opts = crate::snapshot::Options {
                wait: std::time::Duration::from_millis(snapshot_wait_ms),
                cols: snapshot_cols,
                root: snapshot_root,
            };
            compile::run(&deck, output.as_deref(), choice, opts)
        }
        #[cfg(feature = "pdf")]
        Some(Cmd::Pdf {
            deck,
            output,
            cols,
            rows,
            font_size,
            snapshots,
            no_snapshots,
            snapshot_wait_ms,
            snapshot_cols,
            snapshot_root,
        }) => {
            let opts = crate::pdf::Options {
                cols,
                rows,
                font_size,
                snapshots: match (snapshots, no_snapshots) {
                    (true, _) => Some(true),
                    (_, true) => Some(false),
                    _ => None, // errors if the deck has unsnapshotted terminals
                },
                snapshot_opts: crate::snapshot::Options {
                    wait: std::time::Duration::from_millis(snapshot_wait_ms),
                    cols: snapshot_cols,
                    root: snapshot_root,
                },
            };
            crate::pdf::run(&deck, output.as_deref(), &opts)
        }
        None => match cli.deck {
            Some(deck) => present::run(
                &deck,
                cli.socket.unwrap_or_else(default_socket),
                !cli.no_watch,
                cli.eager,
            ),
            None => {
                eprintln!("usage: deckhand <deck.md|deck.json|url> | deckhand notes");
                eprintln!("       deckhand --help");
                std::process::exit(2);
            }
        },
    }
}
