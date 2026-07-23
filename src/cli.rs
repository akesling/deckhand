//! Command-line interface.

use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

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

/// What `deckhand compile` writes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
enum Format {
    /// One presentable markdown file (the default)
    Md,
    /// A PDF, one page per slide
    Pdf,
    /// A single self-contained static HTML page
    Html,
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
    /// Compile a deck to a file: markdown (one presentable file), pdf,
    /// or html. Decks with unsnapshotted terminal blocks require
    /// choosing --snapshots (runs their commands, bakes captures in)
    /// or --no-snapshots
    Compile {
        deck: String,
        /// Write here. The extension picks the format (.pdf, .html,
        /// else markdown); md defaults to stdout, pdf/html to the
        /// deck's name with the format's extension
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Output format (default: from --output's extension, else md)
        #[arg(long, value_enum)]
        to: Option<Format>,
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
        /// Slide grid width for pdf/html output, in terminal columns
        #[arg(long, default_value_t = 100)]
        cols: u16,
        /// Slide grid height for pdf/html output, in terminal rows
        #[arg(long, default_value_t = 30)]
        rows: u16,
        /// Font size in points for pdf output; with the grid, this
        /// sets the page size
        #[arg(long, default_value_t = 10.0)]
        font_size: f32,
    },
}

fn default_socket() -> PathBuf {
    std::env::temp_dir().join("deckhand.sock")
}

/// The compile format: `--to` wins, then the output extension, then
/// markdown.
fn format_of(to: Option<Format>, output: Option<&Path>) -> Format {
    to.unwrap_or_else(|| {
        match output
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref()
        {
            Some("pdf") => Format::Pdf,
            Some("html") | Some("htm") => Format::Html,
            _ => Format::Md,
        }
    })
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
            to,
            snapshots,
            no_snapshots,
            snapshot_wait_ms,
            snapshot_cols,
            snapshot_root,
            cols,
            rows,
            font_size,
        }) => {
            let choice = match (snapshots, no_snapshots) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None, // compiling errors if a choice is needed
            };
            let snapshot_opts = crate::snapshot::Options {
                wait: std::time::Duration::from_millis(snapshot_wait_ms),
                cols: snapshot_cols,
                root: snapshot_root,
            };
            // Grid knobs only reach the pdf/html exporters; md (and
            // feature-less builds) load them not at all.
            let _ = (cols, rows, font_size);
            match format_of(to, output.as_deref()) {
                Format::Md => compile::run(&deck, output.as_deref(), choice, snapshot_opts),
                #[cfg(feature = "pdf")]
                Format::Pdf => crate::pdf::run(
                    &deck,
                    output.as_deref(),
                    &crate::pdf::Options {
                        cols,
                        rows,
                        font_size,
                        snapshots: choice,
                        snapshot_opts,
                    },
                ),
                #[cfg(not(feature = "pdf"))]
                Format::Pdf => anyhow::bail!(
                    "this deckhand was built without the `pdf` feature; \
                     rebuild with default features for PDF output"
                ),
                #[cfg(feature = "html")]
                Format::Html => crate::html::run(
                    &deck,
                    output.as_deref(),
                    &crate::html::Options {
                        cols,
                        rows,
                        snapshots: choice,
                        snapshot_opts,
                    },
                ),
                #[cfg(not(feature = "html"))]
                Format::Html => anyhow::bail!(
                    "this deckhand was built without the `html` feature; \
                     rebuild with default features for HTML output"
                ),
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_from_flag_or_extension() {
        let p = |s: &str| PathBuf::from(s);
        assert_eq!(format_of(None, None), Format::Md);
        assert_eq!(format_of(None, Some(&p("talk.md"))), Format::Md);
        assert_eq!(format_of(None, Some(&p("talk.pdf"))), Format::Pdf);
        assert_eq!(format_of(None, Some(&p("talk.HTML"))), Format::Html);
        assert_eq!(format_of(None, Some(&p("talk.htm"))), Format::Html);
        // --to overrides the extension.
        assert_eq!(
            format_of(Some(Format::Md), Some(&p("talk.pdf"))),
            Format::Md
        );
    }
}
