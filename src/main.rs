mod compile;
mod config;
mod deck;
mod markdown;
mod notes;
mod present;
mod proto;
mod server;
mod term;
mod theme;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "deckhand",
    version,
    about = "Present markdown slide decks as a TUI",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    /// Deck to present: a markdown file or a JSON manifest
    /// (shorthand for `deckhand present <deck>`)
    deck: Option<PathBuf>,
    /// Unix socket path for presenter-notes sync
    #[arg(long)]
    socket: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Present a deck
    Present {
        deck: PathBuf,
        /// Unix socket path for presenter-notes sync
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Follow presenter notes from a separate terminal
    Notes {
        /// Unix socket path the presenter is broadcasting on
        #[arg(long)]
        socket: Option<PathBuf>,
    },
    /// Flatten a deck (e.g. a JSON manifest) into one markdown file
    Compile {
        deck: PathBuf,
        /// Write here instead of stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn default_socket() -> PathBuf {
    std::env::temp_dir().join("deckhand.sock")
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Cmd::Present { deck, socket }) => {
            present::run(&deck, socket.unwrap_or_else(default_socket))
        }
        Some(Cmd::Notes { socket }) => notes::run(socket.unwrap_or_else(default_socket)),
        Some(Cmd::Compile { deck, output }) => compile::run(&deck, output.as_deref()),
        None => match cli.deck {
            Some(deck) => present::run(&deck, cli.socket.unwrap_or_else(default_socket)),
            None => {
                eprintln!("usage: deckhand <deck.md|deck.json> | deckhand notes");
                eprintln!("       deckhand --help");
                std::process::exit(2);
            }
        },
    }
}
