//! Live PTY sessions embedded in slides.
//!
//! Each `TermBlock` in the deck gets at most one `TermSession`: a child
//! process on a PTY, with output fed into a vt100 emulator that the
//! presenter UI renders via `tui-term`.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Lines of history kept per terminal for scrollback viewing.
const SCROLLBACK_LINES: usize = 10_000;

pub struct TermSession {
    pub parser: Arc<Mutex<vt100::Parser>>,
    pub exited: Arc<AtomicBool>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty>,
    child: Box<dyn Child + Send + Sync>,
    rows: u16,
    cols: u16,
}

impl TermSession {
    pub fn spawn(command: Option<&str>, rows: u16, cols: u16, cwd: &Path) -> Result<Self> {
        let pty = native_pty_system();
        let pair = pty.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut cmd = match command {
            Some(script) => {
                let mut c = CommandBuilder::new(&shell);
                c.arg("-c");
                c.arg(script);
                c
            }
            None => CommandBuilder::new(&shell),
        };
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let writer = pair.master.take_writer()?;
        let mut reader = pair.master.try_clone_reader()?;

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, SCROLLBACK_LINES)));
        let exited = Arc::new(AtomicBool::new(false));
        {
            let parser = Arc::clone(&parser);
            let exited = Arc::clone(&exited);
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => {
                            exited.store(true, Ordering::SeqCst);
                            break;
                        }
                        Ok(n) => parser.lock().unwrap().process(&buf[..n]),
                    }
                }
            });
        }

        Ok(TermSession {
            parser,
            exited,
            writer,
            master: pair.master,
            child,
            rows,
            cols,
        })
    }

    pub fn exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    pub fn write_input(&mut self, bytes: &[u8]) {
        // Typing jumps back to the live view, like a normal terminal.
        {
            let mut p = self.parser.lock().unwrap();
            if p.screen().scrollback() > 0 {
                p.set_scrollback(0);
            }
        }
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    /// Move the scrollback view; positive = further into history.
    pub fn scroll_lines(&mut self, delta: isize) {
        let mut p = self.parser.lock().unwrap();
        let cur = p.screen().scrollback() as isize;
        let new = (cur + delta).max(0) as usize;
        p.set_scrollback(new);
    }

    pub fn scroll_page(&mut self, up: bool) {
        let half = (self.rows / 2).max(1) as isize;
        self.scroll_lines(if up { half } else { -half });
    }

    /// How far into history the view currently is (0 = live).
    pub fn scroll_offset(&self) -> usize {
        self.parser.lock().unwrap().screen().scrollback()
    }

    /// (current offset, total history lines available). vt100 doesn't
    /// expose the used scrollback length, but `set_scrollback` clamps to
    /// it — so probe with a huge value and restore.
    pub fn scroll_info(&self) -> (usize, usize) {
        let mut p = self.parser.lock().unwrap();
        let cur = p.screen().scrollback();
        p.set_scrollback(usize::MAX);
        let avail = p.screen().scrollback();
        p.set_scrollback(cur);
        (cur, avail)
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        if rows == self.rows && cols == self.cols {
            return;
        }
        self.rows = rows;
        self.cols = cols;
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.parser.lock().unwrap().set_size(rows, cols);
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Translate a crossterm key event into the byte sequence a terminal
/// would send. Returns `None` for keys with no sensible encoding.
pub fn key_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let mut out: Vec<u8> = Vec::new();
    if alt {
        out.push(0x1b);
    }
    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                let b = match c {
                    'a'..='z' => c as u8 - b'a' + 1,
                    'A'..='Z' => c.to_ascii_lowercase() as u8 - b'a' + 1,
                    '@' | ' ' => 0,
                    '[' => 27,
                    '\\' => 28,
                    ']' => 29,
                    '^' => 30,
                    '_' | '/' => 31,
                    _ => return None,
                };
                out.push(b);
            } else {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
        KeyCode::Enter => out.push(b'\r'),
        KeyCode::Backspace => out.push(0x7f),
        KeyCode::Tab => out.push(b'\t'),
        KeyCode::BackTab => out.extend_from_slice(b"\x1b[Z"),
        KeyCode::Esc => out.push(0x1b),
        KeyCode::Up => out.extend_from_slice(b"\x1b[A"),
        KeyCode::Down => out.extend_from_slice(b"\x1b[B"),
        KeyCode::Right => out.extend_from_slice(b"\x1b[C"),
        KeyCode::Left => out.extend_from_slice(b"\x1b[D"),
        KeyCode::Home => out.extend_from_slice(b"\x1b[H"),
        KeyCode::End => out.extend_from_slice(b"\x1b[F"),
        KeyCode::PageUp => out.extend_from_slice(b"\x1b[5~"),
        KeyCode::PageDown => out.extend_from_slice(b"\x1b[6~"),
        KeyCode::Insert => out.extend_from_slice(b"\x1b[2~"),
        KeyCode::Delete => out.extend_from_slice(b"\x1b[3~"),
        KeyCode::F(n) => {
            let seq: &[u8] = match n {
                1 => b"\x1bOP",
                2 => b"\x1bOQ",
                3 => b"\x1bOR",
                4 => b"\x1bOS",
                5 => b"\x1b[15~",
                6 => b"\x1b[17~",
                7 => b"\x1b[18~",
                8 => b"\x1b[19~",
                9 => b"\x1b[20~",
                10 => b"\x1b[21~",
                11 => b"\x1b[23~",
                12 => b"\x1b[24~",
                _ => return None,
            };
            out.extend_from_slice(seq);
        }
        _ => return None,
    }
    Some(out)
}
