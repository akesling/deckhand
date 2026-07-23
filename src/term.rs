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

/// Lines of history kept per terminal for scrollback viewing.
const SCROLLBACK_LINES: usize = 10_000;

/// One live PTY session: a child process, its master side, and a vt100
/// emulator fed by a background reader thread.
pub struct TermSession {
    /// The terminal emulator; lock it and render `screen()` to display.
    pub parser: Arc<Mutex<vt100::Parser>>,
    /// Set by the reader thread when the child's output stream closes.
    pub exited: Arc<AtomicBool>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty>,
    child: Box<dyn Child + Send + Sync>,
    reaped: bool,
    rows: u16,
    cols: u16,
}

impl TermSession {
    /// Spawn `command` via `$SHELL -c` (or an interactive `$SHELL` when
    /// `None`) on a fresh PTY of the given size, in `cwd`.
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
            reaped: false,
            rows,
            cols,
        })
    }

    /// Whether the child's output stream has closed.
    pub fn exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }

    /// Send bytes to the child's stdin; also snaps scrollback to live.
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

    /// Route a wheel tick at cell (col, row): into the child when it
    /// wants it (mouse reporting, or alternate-screen arrow keys),
    /// otherwise through the local history view.
    pub fn wheel(&mut self, up: bool, col: u16, row: u16) {
        let to_child = {
            let p = self.parser.lock().unwrap();
            wheel_bytes(p.screen(), up, col, row)
        };
        match to_child {
            Some(bytes) => self.write_input(&bytes),
            None => self.scroll_lines(if up { 3 } else { -3 }),
        }
    }

    /// Forward a left click at (col, row) when the child reports mouse;
    /// returns whether it was sent (false keeps the caller's own click
    /// semantics).
    pub fn click(&mut self, col: u16, row: u16) -> bool {
        let report = {
            let p = self.parser.lock().unwrap();
            click_bytes(p.screen(), col, row)
        };
        match report {
            Some(bytes) => {
                self.write_input(&bytes);
                true
            }
            None => false,
        }
    }

    /// Scroll the view by half the viewport height.
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

    /// Resize the PTY and emulator together; no-op if unchanged.
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

    /// Reap the child once its output stream has closed, so commands
    /// that finish on their own don't linger as zombies for the rest of
    /// the presentation. No-op while the child is running (or once
    /// reaped); cheap enough to call every frame.
    pub fn reap(&mut self) {
        if self.reaped || !self.exited() {
            return;
        }
        if let Ok(Some(_)) = self.child.try_wait() {
            self.reaped = true;
        }
    }

    /// Kill the child process and reap it.
    pub fn kill(&mut self) {
        // Once reaped the pid may be reused — signaling it could hit an
        // unrelated process.
        if self.reaped {
            return;
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.reaped = true;
    }
}

/// What a wheel tick should send to the child, per its screen state:
/// mouse-reporting programs get a wheel report, alternate-screen
/// programs without mouse reporting get arrow keys (xterm's
/// "alternate scroll" convention — how `less` and `vim` scroll under a
/// wheel), and `None` means the child doesn't care — the caller moves
/// the local history view instead.
fn wheel_bytes(screen: &vt100::Screen, up: bool, col: u16, row: u16) -> Option<Vec<u8>> {
    if screen.mouse_protocol_mode() != vt100::MouseProtocolMode::None {
        return Some(mouse_report(
            screen,
            if up { 64 } else { 65 },
            col,
            row,
            true,
        ));
    }
    if screen.alternate_screen() {
        let arrow: &[u8] = match (screen.application_cursor(), up) {
            (true, true) => b"\x1bOA",
            (true, false) => b"\x1bOB",
            (false, true) => b"\x1b[A",
            (false, false) => b"\x1b[B",
        };
        return Some(arrow.repeat(3));
    }
    None
}

/// A left press+release pair, if the child asked for mouse reports.
fn click_bytes(screen: &vt100::Screen, col: u16, row: u16) -> Option<Vec<u8>> {
    if screen.mouse_protocol_mode() == vt100::MouseProtocolMode::None {
        return None;
    }
    let mut bytes = mouse_report(screen, 0, col, row, true);
    bytes.extend(mouse_report(screen, 0, col, row, false));
    Some(bytes)
}

/// One mouse report in the child's negotiated encoding (1-based cells).
fn mouse_report(screen: &vt100::Screen, button: u8, col: u16, row: u16, press: bool) -> Vec<u8> {
    let (x, y) = (u32::from(col) + 1, u32::from(row) + 1);
    if screen.mouse_protocol_encoding() == vt100::MouseProtocolEncoding::Sgr {
        let suffix = if press { 'M' } else { 'm' };
        return format!("\x1b[<{button};{x};{y}{suffix}").into_bytes();
    }
    // Legacy X10 bytes (utf8 mode only diverges past column 95, where
    // real programs negotiate SGR anyway). Release is button 3;
    // coordinates saturate at the encodable maximum.
    let b = if press { button } else { 3 };
    vec![
        0x1b,
        b'[',
        b'M',
        32 + b,
        (32 + x.min(223)) as u8,
        (32 + y.min(223)) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen_after(setup: &[u8]) -> vt100::Parser {
        let mut p = vt100::Parser::new(24, 80, 0);
        p.process(setup);
        p
    }

    #[test]
    fn wheel_scrolls_history_on_plain_screens() {
        let p = screen_after(b"plain output\r\n");
        assert_eq!(wheel_bytes(p.screen(), true, 5, 5), None);
    }

    #[test]
    fn wheel_sends_arrows_to_alternate_screen_tuis() {
        let p = screen_after(b"\x1b[?1049h");
        assert_eq!(
            wheel_bytes(p.screen(), true, 0, 0).unwrap(),
            b"\x1b[A\x1b[A\x1b[A"
        );
        // Application cursor keys (vim, htop) get SS3 arrows.
        let p = screen_after(b"\x1b[?1049h\x1b[?1h");
        assert_eq!(
            wheel_bytes(p.screen(), false, 0, 0).unwrap(),
            b"\x1bOB\x1bOB\x1bOB"
        );
    }

    #[test]
    fn wheel_reports_to_mouse_aware_programs() {
        let p = screen_after(b"\x1b[?1000h\x1b[?1006h");
        assert_eq!(
            wheel_bytes(p.screen(), true, 2, 4).unwrap(),
            b"\x1b[<64;3;5M"
        );
        let p = screen_after(b"\x1b[?1000h");
        assert_eq!(
            wheel_bytes(p.screen(), false, 2, 4).unwrap(),
            vec![0x1b, b'[', b'M', 32 + 65, 32 + 3, 32 + 5]
        );
    }

    #[test]
    fn clicks_forward_only_when_reported() {
        let p = screen_after(b"");
        assert_eq!(click_bytes(p.screen(), 1, 1), None);
        let p = screen_after(b"\x1b[?1000h\x1b[?1006h");
        assert_eq!(
            click_bytes(p.screen(), 1, 1).unwrap(),
            b"\x1b[<0;2;2M\x1b[<0;2;2m"
        );
    }
}
