//! Presenter-side notes server: accepts connections on a unix socket and
//! broadcasts the current slide state as JSON lines.

use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

use crate::proto::NotesState;

/// The presenter's side of the notes channel: a unix-socket listener
/// that pushes [`NotesState`] JSON lines to every connected client.
pub struct NotesServer {
    path: PathBuf,
    clients: Arc<Mutex<Vec<UnixStream>>>,
    latest: Arc<Mutex<Option<NotesState>>>,
}

impl NotesServer {
    /// Bind the socket (replacing a stale file) and start accepting
    /// clients on a background thread. The socket is removed on drop.
    pub fn start(path: PathBuf) -> Result<Self> {
        // Stale socket from a previous run.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)
            .with_context(|| format!("binding notes socket {}", path.display()))?;

        let clients: Arc<Mutex<Vec<UnixStream>>> = Arc::new(Mutex::new(Vec::new()));
        let latest: Arc<Mutex<Option<NotesState>>> = Arc::new(Mutex::new(None));
        {
            let clients = Arc::clone(&clients);
            let latest = Arc::clone(&latest);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    // New client immediately gets the current state.
                    if let Some(state) = latest.lock().unwrap().clone()
                        && let Ok(line) = serde_json::to_string(&state)
                    {
                        let _ = writeln!(stream, "{line}");
                    }
                    clients.lock().unwrap().push(stream);
                }
            });
        }

        Ok(NotesServer {
            path,
            clients,
            latest,
        })
    }

    /// Send `state` to every connected client (dropping any that have
    /// disconnected) and remember it for clients that connect later.
    pub fn broadcast(&self, state: &NotesState) {
        *self.latest.lock().unwrap() = Some(state.clone());
        let Ok(line) = serde_json::to_string(state) else {
            return;
        };
        self.clients
            .lock()
            .unwrap()
            .retain_mut(|s| writeln!(s, "{line}").is_ok());
    }
}

impl Drop for NotesServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
