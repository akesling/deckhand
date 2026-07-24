//! Presenter-side notes server: accepts connections on a unix socket and
//! broadcasts the current slide state as JSON lines.

use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use anyhow::{Context, Result};

use crate::proto::NotesState;

/// How long a single client write may block before that client is
/// dropped. A live client drains a one-line message immediately; only a
/// hung one (stopped terminal, ctrl-Z'd process) hits this.
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

enum Msg {
    Client(UnixStream),
    State(NotesState),
}

/// The presenter's side of the notes channel: a unix-socket listener
/// that pushes [`NotesState`] JSON lines to every connected client.
///
/// All socket writes happen on a background writer thread — a client
/// that stops reading can never block the presenter mid-slide-change.
pub struct NotesServer {
    path: PathBuf,
    tx: Sender<Msg>,
}

impl NotesServer {
    /// Bind the socket (replacing a stale file) and start the accept and
    /// writer threads. The socket is removed on drop.
    pub fn start(path: PathBuf) -> Result<Self> {
        // A connectable socket means another presenter is live on it —
        // don't silently steal its notes clients.
        if UnixStream::connect(&path).is_ok() {
            anyhow::bail!(
                "another deckhand is already presenting on {} — give this one its own socket \
                 with --socket",
                path.display()
            );
        }
        // Anything else on the path is a stale socket from a dead run.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)
            .with_context(|| format!("binding notes socket {}", path.display()))?;

        let (tx, rx) = mpsc::channel::<Msg>();

        // Accept thread: hands new clients to the writer thread.
        {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
                    if tx.send(Msg::Client(stream)).is_err() {
                        break;
                    }
                }
            });
        }

        // Writer thread: owns the client list and the latest state, so
        // slow-client writes stall here and not the render loop.
        std::thread::spawn(move || {
            let mut clients: Vec<UnixStream> = Vec::new();
            let mut latest: Option<String> = None;
            for msg in rx {
                match msg {
                    Msg::Client(mut stream) => {
                        // New client immediately gets the current state.
                        let greeted = match &latest {
                            Some(line) => writeln!(stream, "{line}").is_ok(),
                            None => true,
                        };
                        if greeted {
                            clients.push(stream);
                        }
                    }
                    Msg::State(state) => {
                        let Ok(line) = serde_json::to_string(&state) else {
                            continue;
                        };
                        clients.retain_mut(|s| writeln!(s, "{line}").is_ok());
                        latest = Some(line);
                    }
                }
            }
        });

        Ok(NotesServer { path, tx })
    }

    /// Queue `state` for every connected client (dropping any that have
    /// disconnected or hung) and remember it for clients that connect
    /// later. Never blocks.
    pub fn broadcast(&self, state: &NotesState) {
        let _ = self.tx.send(Msg::State(state.clone()));
    }
}

impl Drop for NotesServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Binds real unix sockets; run explicitly with
    /// `cargo test -- --ignored` (CI does).
    #[test]
    #[ignore = "binds a unix socket"]
    fn refuses_to_steal_a_live_socket() {
        // In-workspace rather than the system temp dir: unix-socket
        // paths are capped at ~104 bytes, and sandboxes that confine
        // builds to the workspace still allow binding here.
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("notes-{}.sock", std::process::id()));
        let first = NotesServer::start(path.clone()).expect("first bind");
        let err = match NotesServer::start(path.clone()) {
            Err(e) => e,
            Ok(_) => panic!("second presenter stole the live socket"),
        };
        assert!(
            format!("{err:#}").contains("--socket"),
            "unhelpful error: {err:#}"
        );
        drop(first);
        // A stale socket file — bound once, listener long gone — is
        // replaced without complaint.
        drop(UnixListener::bind(&path).unwrap());
        let _second = NotesServer::start(path).unwrap();
    }
}
