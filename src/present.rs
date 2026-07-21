//! Native TUI front end.
//!
//! All presentation logic lives in [`crate::presenter`]; this module
//! supplies what only a real terminal can: crossterm events in, a
//! terminal screen out, real PTYs via [`PtyProvider`], and the
//! presenter-notes broadcast over the unix socket.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;
use tui_term::widget::PseudoTerminal;

use crate::deck::TermBlock;
use crate::presenter::{Key, KeyPress, Mouse, MouseAction, Presenter, TermState, TerminalProvider};
use crate::proto::NotesState;
use crate::server::NotesServer;
use crate::source;
use crate::term::TermSession;
use crate::theme::{self, ThemeConfig};

/// Real PTYs: spawns each terminal block as a child process the first
/// time it's shown, keeps it running while you navigate elsewhere. The
/// reference [`TerminalProvider`] implementation.
pub struct PtyProvider {
    deck_dir: PathBuf,
    sessions: HashMap<usize, TermSession>,
    failed: HashMap<usize, String>,
}

impl PtyProvider {
    fn shutdown(&mut self) {
        for (_, mut s) in self.sessions.drain() {
            s.kill();
        }
    }
}

impl TerminalProvider for PtyProvider {
    fn prepare(&mut self, block: &TermBlock, cols: u16, rows: u16) {
        let cols = cols.max(4);
        let rows = rows.max(2);
        if self.failed.contains_key(&block.id) {
            return;
        }
        match self.sessions.get_mut(&block.id) {
            Some(s) => s.resize(rows, cols),
            None => {
                match TermSession::spawn(block.command.as_deref(), rows, cols, &self.deck_dir) {
                    Ok(s) => {
                        self.sessions.insert(block.id, s);
                    }
                    Err(e) => {
                        self.failed.insert(block.id, e.to_string());
                    }
                }
            }
        }
    }

    fn state(&self, block: &TermBlock) -> TermState {
        if let Some(msg) = self.failed.get(&block.id) {
            return TermState::Failed(msg.clone());
        }
        match self.sessions.get(&block.id) {
            Some(s) if s.exited() => TermState::Exited,
            Some(s) => {
                let (scroll_offset, scroll_total) = s.scroll_info();
                TermState::Running {
                    scroll_offset,
                    scroll_total,
                }
            }
            // Not spawned yet; `prepare` will start it this frame.
            None => TermState::Running {
                scroll_offset: 0,
                scroll_total: 0,
            },
        }
    }

    fn draw(&mut self, block: &TermBlock, inner: Rect, buf: &mut Buffer) {
        if let Some(session) = self.sessions.get(&block.id) {
            let parser = session.parser.lock().unwrap();
            PseudoTerminal::new(parser.screen()).render(inner, buf);
        }
    }

    fn input(&mut self, id: usize, bytes: &[u8]) {
        if let Some(s) = self.sessions.get_mut(&id) {
            s.write_input(bytes);
        }
    }

    fn scroll(&mut self, id: usize, delta: isize) {
        if let Some(s) = self.sessions.get_mut(&id) {
            s.scroll_lines(delta);
        }
    }

    fn restart(&mut self, ids: &[usize]) {
        for id in ids {
            if let Some(mut s) = self.sessions.remove(id) {
                s.kill();
            }
            self.failed.remove(id);
        }
    }
}

pub fn run(input: &str, socket: PathBuf) -> Result<()> {
    let source::Loaded {
        deck,
        theme: deck_theme,
        base_dir,
    } = source::load(input)?;
    let cfgs: Vec<ThemeConfig> = [theme::user_config()?, deck_theme]
        .into_iter()
        .flatten()
        .collect();
    let provider = PtyProvider {
        deck_dir: base_dir,
        sessions: HashMap::new(),
        failed: HashMap::new(),
    };
    let mut presenter = Presenter::new(deck, cfgs, provider)?;
    presenter.help_footer = Some(format!(
        "notes: deckhand notes --socket {}",
        socket.display()
    ));

    let server = NotesServer::start(socket).context("starting presenter-notes server")?;
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    broadcast(&server, &presenter, started_at);

    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let result = event_loop(&mut terminal, &mut presenter, &server, started_at);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    presenter.provider.shutdown();
    result
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    presenter: &mut Presenter<PtyProvider>,
    server: &NotesServer,
    started_at: u64,
) -> Result<()> {
    while !presenter.should_quit() {
        terminal.draw(|f| {
            let area = f.area();
            presenter.draw(area, f.buffer_mut());
        })?;
        if event::poll(Duration::from_millis(33))? {
            let before = presenter.position();
            match event::read()? {
                Event::Key(k) if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                    if let Some(kp) = translate_key(&k) {
                        presenter.on_key(kp);
                    }
                }
                Event::Mouse(m) => {
                    if let Some(mm) = translate_mouse(&m) {
                        presenter.on_mouse(mm);
                    }
                }
                _ => {}
            }
            if presenter.position() != before {
                broadcast(server, presenter, started_at);
            }
        }
    }
    Ok(())
}

fn translate_key(k: &KeyEvent) -> Option<KeyPress> {
    let key = match k.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Insert => Key::Insert,
        KeyCode::Delete => Key::Delete,
        KeyCode::F(n) => Key::F(n),
        _ => return None,
    };
    Some(KeyPress {
        key,
        ctrl: k.modifiers.contains(KeyModifiers::CONTROL),
        alt: k.modifiers.contains(KeyModifiers::ALT),
        shift: k.modifiers.contains(KeyModifiers::SHIFT),
    })
}

fn translate_mouse(m: &event::MouseEvent) -> Option<Mouse> {
    let action = match m.kind {
        MouseEventKind::Down(MouseButton::Left) => MouseAction::LeftClick,
        MouseEventKind::ScrollUp => MouseAction::ScrollUp,
        MouseEventKind::ScrollDown => MouseAction::ScrollDown,
        _ => return None,
    };
    Some(Mouse {
        x: m.column,
        y: m.row,
        action,
    })
}

fn broadcast(server: &NotesServer, presenter: &Presenter<PtyProvider>, started_at: u64) {
    let deck = presenter.deck();
    let (col, row) = presenter.position();
    let flat = deck.flat();
    let idx = flat.iter().position(|&p| p == (col, row)).unwrap_or(0);
    let next_title = flat
        .get(idx + 1)
        .map(|&(c, r)| deck.slide(c, r).title.clone());
    let slide = deck.slide(col, row);
    server.broadcast(&NotesState {
        deck_title: deck.title.clone(),
        col,
        row,
        total_cols: deck.columns.len(),
        col_depth: deck.columns[col].slides.len(),
        slide_no: idx + 1,
        total_slides: flat.len(),
        title: slide.title.clone(),
        notes: slide.notes.clone(),
        next_title,
        started_at,
    });
}
