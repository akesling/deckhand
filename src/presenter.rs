//! The platform-independent presenter.
//!
//! `Presenter` owns everything every presentation context shares:
//! navigation across the 2-D grid, slide layout, the overview with jump
//! codes, theming, terminal focus/selection, and drawing — all into a
//! ratatui `Buffer`, so it runs anywhere.
//!
//! What it *can't* know is how live terminals work in a given context.
//! That's a `TerminalProvider`: native supplies real PTYs, the wasm build
//! supplies placeholders, and other embeddings (ssh, containers,
//! recordings…) can supply their own. The presenter draws all the chrome
//! — borders, pane numbers, focus hints, scrollbars — and delegates only
//! the inside of the box.
//!
//! Events arrive as platform-neutral [`KeyPress`] / [`Mouse`] values;
//! each front end translates its own event source into them.

use std::collections::HashMap;

use anyhow::{Context, Result};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    StatefulWidget, Widget,
};

use crate::deck::{Deck, Segment, Slide, TermBlock};
use crate::hints::{JUMP_KEYS, jump_codes};
use crate::markdown;
use crate::theme::{Theme, ThemeConfig, VAlign};

// ----------------------------------------------------------------- events

/// A platform-independent key, as translated by each front end from its
/// own event source (crossterm, DOM `KeyboardEvent`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A printable character (already case/shift-resolved).
    Char(char),
    /// Return / enter.
    Enter,
    /// Escape.
    Esc,
    /// Backspace.
    Backspace,
    /// Tab.
    Tab,
    /// Shift-tab.
    BackTab,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Insert.
    Insert,
    /// Forward delete.
    Delete,
    /// A function key (`F(1)` ..= `F(12)` are encodable).
    F(u8),
}

/// A [`Key`] plus its modifier state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyPress {
    /// The key itself.
    pub key: Key,
    /// Control held.
    pub ctrl: bool,
    /// Alt/option held.
    pub alt: bool,
    /// Shift held.
    pub shift: bool,
}

impl KeyPress {
    /// A press of `key` with no modifiers.
    pub fn plain(key: Key) -> Self {
        KeyPress {
            key,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }
}

/// What a mouse event did, in presenter terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    /// Left button pressed.
    LeftClick,
    /// Wheel scrolled up (away from the user).
    ScrollUp,
    /// Wheel scrolled down (toward the user).
    ScrollDown,
}

/// A mouse event in screen cell coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mouse {
    /// Column of the affected cell.
    pub x: u16,
    /// Row of the affected cell.
    pub y: u16,
    /// What happened.
    pub action: MouseAction,
}

/// Encode a key press as the bytes a terminal would send — used to feed
/// input to a focused terminal, whatever the provider behind it is.
pub fn encode_key(key: &KeyPress) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    if key.alt {
        out.push(0x1b);
    }
    match key.key {
        Key::Char(c) => {
            if key.ctrl {
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
        Key::Enter => out.push(b'\r'),
        Key::Backspace => out.push(0x7f),
        Key::Tab => out.push(b'\t'),
        Key::BackTab => out.extend_from_slice(b"\x1b[Z"),
        Key::Esc => out.push(0x1b),
        Key::Up => out.extend_from_slice(b"\x1b[A"),
        Key::Down => out.extend_from_slice(b"\x1b[B"),
        Key::Right => out.extend_from_slice(b"\x1b[C"),
        Key::Left => out.extend_from_slice(b"\x1b[D"),
        Key::Home => out.extend_from_slice(b"\x1b[H"),
        Key::End => out.extend_from_slice(b"\x1b[F"),
        Key::PageUp => out.extend_from_slice(b"\x1b[5~"),
        Key::PageDown => out.extend_from_slice(b"\x1b[6~"),
        Key::Insert => out.extend_from_slice(b"\x1b[2~"),
        Key::Delete => out.extend_from_slice(b"\x1b[3~"),
        Key::F(n) => {
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
    }
    Some(out)
}

// -------------------------------------------------------------- providers

/// What a terminal block is doing right now, per its provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermState {
    /// The terminal is live and accepting input.
    Running {
        /// How far into history the view is (0 = live).
        scroll_offset: usize,
        /// Total history lines available.
        scroll_total: usize,
    },
    /// The process ended; `R` restarts it.
    Exited,
    /// The terminal couldn't start; the message is shown in the box.
    Failed(String),
    /// This context can't run terminals but is showing a capture baked
    /// in by `deckhand compile --snapshots`.
    Snapshot,
    /// This context can't run terminals (e.g. the browser).
    Unavailable,
}

/// Supplies the live, context-dependent innards of terminal blocks.
///
/// The presenter draws all chrome (borders, titles, pane numbering,
/// hints, scrollbars) and calls the provider for the inside of the box.
pub trait TerminalProvider {
    /// Called for each visible terminal every frame with its inner size;
    /// spawn or resize as needed.
    fn prepare(&mut self, block: &TermBlock, cols: u16, rows: u16);
    /// What this block is doing right now; drives border/hint chrome.
    fn state(&self, block: &TermBlock) -> TermState;
    /// Draw the terminal's contents into `inner`.
    fn draw(&mut self, block: &TermBlock, inner: Rect, buf: &mut Buffer);
    /// Bytes typed while this terminal is focused.
    fn input(&mut self, id: usize, bytes: &[u8]);
    /// Move the scrollback view; positive = further into history.
    fn scroll(&mut self, id: usize, delta: isize);
    /// Half-page scroll needs the viewport height; default derives it.
    fn scroll_page(&mut self, id: usize, up: bool, viewport_rows: u16) {
        let half = (viewport_rows / 2).max(1) as isize;
        self.scroll(id, if up { half } else { -half });
    }
    /// Kill and forget these terminals so they respawn (the `R` key).
    fn restart(&mut self, ids: &[usize]);
    /// Whether terminals can be focused and typed into in this context.
    fn interactive(&self) -> bool {
        true
    }
}

// -------------------------------------------------------------- presenter

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Slide,
    Overview { sel: (usize, usize) },
    Help,
}

/// The shared presentation state machine and renderer. See the
/// [module docs](self) for the division of labor with providers.
pub struct Presenter<P: TerminalProvider> {
    /// The context's terminal provider, reachable for lifecycle calls
    /// (e.g. the native front end kills its PTYs on shutdown).
    pub provider: P,
    deck: Deck,
    theme: Theme,
    slide_themes: HashMap<(usize, usize), Theme>,
    col: usize,
    row: usize,
    depth_memory: Vec<usize>,
    mode: Mode,
    focus: Option<usize>,
    selected: HashMap<(usize, usize), usize>,
    term_rects: Vec<(usize, Rect)>,
    overview_rects: Vec<((usize, usize), Rect)>,
    status: Option<String>,
    jump_input: String,
    /// Extra line shown at the bottom of the help overlay (the native
    /// front end uses it for the notes-socket hint).
    pub help_footer: Option<String>,
    quit: bool,
}

impl<P: TerminalProvider> Presenter<P> {
    /// `theme_cfgs` are merged lowest-precedence-first (e.g. user config,
    /// then the deck's own theme).
    pub fn new(deck: Deck, theme_cfgs: Vec<ThemeConfig>, provider: P) -> Result<Self> {
        anyhow::ensure!(!deck.columns.is_empty(), "deck contains no slides");
        let theme = Theme::resolve(theme_cfgs.clone())?;
        let mut slide_themes = HashMap::new();
        for (c, column) in deck.columns.iter().enumerate() {
            for (r, slide) in column.slides.iter().enumerate() {
                if let Some(cfg) = &slide.theme {
                    let mut cfgs = theme_cfgs.clone();
                    cfgs.push(cfg.clone());
                    let resolved = Theme::resolve(cfgs)
                        .with_context(|| format!("theme for slide {}.{}", c + 1, r + 1))?;
                    slide_themes.insert((c, r), resolved);
                }
            }
        }
        let ncols = deck.columns.len();
        Ok(Presenter {
            provider,
            deck,
            theme,
            slide_themes,
            col: 0,
            row: 0,
            depth_memory: vec![0; ncols],
            mode: Mode::Slide,
            focus: None,
            selected: HashMap::new(),
            term_rects: Vec::new(),
            overview_rects: Vec::new(),
            status: None,
            jump_input: String::new(),
            help_footer: None,
            quit: false,
        })
    }

    /// The deck being presented.
    pub fn deck(&self) -> &Deck {
        &self.deck
    }

    /// Current (column, depth), 0-based.
    pub fn position(&self) -> (usize, usize) {
        (self.col, self.row)
    }

    /// The slide currently displayed.
    pub fn current_slide(&self) -> &Slide {
        self.deck.slide(self.col, self.row)
    }

    /// Whether the user asked to quit (`q` / ctrl-c).
    pub fn should_quit(&self) -> bool {
        self.quit
    }

    // -------------------------------------------------------------- nav

    fn current_ids(&self) -> Vec<usize> {
        self.deck.slide(self.col, self.row).term_ids()
    }

    fn current_theme(&self) -> Theme {
        self.slide_themes
            .get(&(self.col, self.row))
            .copied()
            .unwrap_or(self.theme)
    }

    /// Jump to a slide; out-of-range coordinates clamp.
    pub fn goto(&mut self, col: usize, row: usize) {
        let col = col.min(self.deck.columns.len() - 1);
        let row = row.min(self.deck.columns[col].slides.len() - 1);
        if (col, row) != (self.col, self.row) {
            self.col = col;
            self.row = row;
            self.depth_memory[col] = row;
            self.focus = None;
        }
    }

    fn next(&mut self) {
        if self.row + 1 < self.deck.columns[self.col].slides.len() {
            self.goto(self.col, self.row + 1);
        } else if self.col + 1 < self.deck.columns.len() {
            self.goto(self.col + 1, 0);
        }
    }

    fn prev(&mut self) {
        if self.row > 0 {
            self.goto(self.col, self.row - 1);
        } else if self.col > 0 {
            let c = self.col - 1;
            self.goto(c, self.deck.columns[c].slides.len() - 1);
        }
    }

    // ------------------------------------------------------------- input

    /// Handle a key press: navigation, mode changes, terminal focus, or
    /// (while a terminal is focused) input forwarded to the provider.
    pub fn on_key(&mut self, key: KeyPress) {
        self.status = None;

        if let Some(id) = self.focus {
            if key.key == Key::Char('q') && key.ctrl {
                self.focus = None;
                return;
            }
            match self.provider.state(&self.block_by_id(id)) {
                TermState::Running { .. } => {}
                _ => {
                    self.focus = None;
                    return;
                }
            }
            if key.shift {
                let rows = self.block_by_id(id).rows;
                match key.key {
                    Key::PageUp => return self.provider.scroll_page(id, true, rows),
                    Key::PageDown => return self.provider.scroll_page(id, false, rows),
                    Key::Up => return self.provider.scroll(id, 1),
                    Key::Down => return self.provider.scroll(id, -1),
                    _ => {}
                }
            }
            if let Some(bytes) = encode_key(&key) {
                self.provider.input(id, &bytes);
            }
            return;
        }

        match self.mode {
            Mode::Help => self.mode = Mode::Slide,
            Mode::Overview { sel } => self.on_key_overview(key, sel),
            Mode::Slide => self.on_key_slide(key),
        }
    }

    fn block_by_id(&self, id: usize) -> TermBlock {
        for column in &self.deck.columns {
            for slide in &column.slides {
                for seg in &slide.segments {
                    if let Segment::Terminal(b) = seg
                        && b.id == id
                    {
                        return b.clone();
                    }
                }
            }
        }
        unreachable!("terminal id {id} not in deck")
    }

    fn on_key_slide(&mut self, key: KeyPress) {
        match key.key {
            Key::Char('c') if key.ctrl => self.quit = true,
            Key::Char('q') => self.quit = true,
            Key::Char(' ') if key.shift => self.prev(),
            Key::Char(' ') | Key::Char('n') | Key::PageDown => self.next(),
            Key::Backspace | Key::Char('p') | Key::PageUp => self.prev(),
            Key::Left | Key::Char('h') => {
                if self.col > 0 {
                    self.goto(self.col - 1, self.depth_memory[self.col - 1]);
                }
            }
            Key::Right | Key::Char('l') => {
                if self.col + 1 < self.deck.columns.len() {
                    self.goto(self.col + 1, self.depth_memory[self.col + 1]);
                }
            }
            Key::Down | Key::Char('j') => self.goto(self.col, self.row + 1),
            Key::Up | Key::Char('k') => self.goto(self.col, self.row.saturating_sub(1)),
            Key::Char('g') | Key::Home => self.goto(0, 0),
            Key::Char('G') | Key::End => self.goto(self.deck.columns.len() - 1, 0),
            Key::Char('o') => {
                self.jump_input.clear();
                self.mode = Mode::Overview {
                    sel: (self.col, self.row),
                };
            }
            Key::Char('?') => self.mode = Mode::Help,
            Key::Enter | Key::Char('t') => self.focus_terminal(),
            Key::Char(c @ '1'..='9') => self.select_terminal(c as usize - '0' as usize),
            Key::Char('R') => self.restart_terminals(),
            _ => {}
        }
    }

    /// The terminal `t` will focus on this slide.
    fn selected_terminal(&self) -> Option<usize> {
        let ids = self.current_ids();
        self.selected
            .get(&(self.col, self.row))
            .copied()
            .filter(|id| ids.contains(id))
            .or_else(|| ids.first().copied())
    }

    fn select_terminal(&mut self, n: usize) {
        if !self.provider.interactive() {
            return;
        }
        let ids = self.current_ids();
        if ids.is_empty() {
            self.status = Some("no terminal on this slide".to_string());
            return;
        }
        match ids.get(n - 1) {
            Some(&id) => {
                self.selected.insert((self.col, self.row), id);
            }
            None => {
                self.status = Some(format!("no terminal {n} here ({} available)", ids.len()));
            }
        }
    }

    fn focus_terminal(&mut self) {
        if !self.provider.interactive() {
            self.status = Some("terminals run when presenting in a real terminal".to_string());
            return;
        }
        let Some(id) = self.selected_terminal() else {
            self.status = Some("no terminal on this slide".to_string());
            return;
        };
        self.selected.insert((self.col, self.row), id);
        self.focus = Some(id);
    }

    fn restart_terminals(&mut self) {
        let ids = self.current_ids();
        if ids.is_empty() || !self.provider.interactive() {
            return;
        }
        self.provider.restart(&ids);
        self.status = Some("terminals restarted".to_string());
    }

    fn overview_codes(&self) -> Vec<((usize, usize), String)> {
        let flat = self.deck.flat();
        let codes = jump_codes(flat.len());
        flat.into_iter().zip(codes).collect()
    }

    fn on_key_overview(&mut self, key: KeyPress, sel: (usize, usize)) {
        if let Key::Char(ch) = key.key
            && JUMP_KEYS.contains(&ch)
        {
            self.jump_input.push(ch);
            let codes = self.overview_codes();
            let hit = codes
                .iter()
                .find(|(_, code)| *code == self.jump_input)
                .map(|(target, _)| *target);
            if let Some(target) = hit {
                self.jump_input.clear();
                self.mode = Mode::Slide;
                self.goto(target.0, target.1);
            } else if !codes
                .iter()
                .any(|(_, code)| code.starts_with(self.jump_input.as_str()))
            {
                self.jump_input.clear();
            }
            return;
        }
        let had_pending = !self.jump_input.is_empty();
        self.jump_input.clear();

        let (mut c, mut r) = sel;
        match key.key {
            Key::Esc if had_pending => return,
            Key::Esc | Key::Char('o') => {
                self.mode = Mode::Slide;
                return;
            }
            Key::Char('q') => {
                self.quit = true;
                return;
            }
            Key::Enter => {
                self.mode = Mode::Slide;
                self.goto(c, r);
                return;
            }
            Key::Left | Key::Char('h') => c = c.saturating_sub(1),
            Key::Right | Key::Char('l') => c = (c + 1).min(self.deck.columns.len() - 1),
            Key::Up | Key::Char('k') => r = r.saturating_sub(1),
            Key::Down | Key::Char('j') => r += 1,
            _ => {}
        }
        r = r.min(self.deck.columns[c].slides.len() - 1);
        self.mode = Mode::Overview { sel: (c, r) };
    }

    /// Handle a mouse event: click to focus/release terminals or jump in
    /// the overview, wheel to scroll terminal history.
    pub fn on_mouse(&mut self, m: Mouse) {
        let pos = Position::new(m.x, m.y);
        let term_at = |rects: &[(usize, Rect)]| {
            rects
                .iter()
                .find(|(_, r)| r.contains(pos))
                .map(|(id, _)| *id)
        };
        match self.mode {
            Mode::Help => {
                if m.action == MouseAction::LeftClick {
                    self.mode = Mode::Slide;
                }
            }
            Mode::Overview { .. } => {
                if m.action == MouseAction::LeftClick
                    && let Some(&(target, _)) =
                        self.overview_rects.iter().find(|(_, r)| r.contains(pos))
                {
                    self.mode = Mode::Slide;
                    self.goto(target.0, target.1);
                }
            }
            Mode::Slide => match m.action {
                MouseAction::LeftClick => {
                    if !self.provider.interactive() {
                        return;
                    }
                    match term_at(&self.term_rects) {
                        Some(id) => {
                            self.selected.insert((self.col, self.row), id);
                            self.focus = Some(id);
                        }
                        None => self.focus = None,
                    }
                }
                MouseAction::ScrollUp | MouseAction::ScrollDown => {
                    if let Some(id) = term_at(&self.term_rects) {
                        let delta = if m.action == MouseAction::ScrollUp {
                            3
                        } else {
                            -3
                        };
                        self.provider.scroll(id, delta);
                    }
                }
            },
        }
    }

    // -------------------------------------------------------------- draw

    /// Render the current frame (slide or overview, plus status bar and
    /// overlays) into `buf`. Also records hit-test regions for mouse
    /// events and gives the provider its per-frame `prepare` calls.
    pub fn draw(&mut self, area: Rect, buf: &mut Buffer) {
        self.term_rects.clear();
        self.overview_rects.clear();
        if area.height < 2 {
            return;
        }
        let content = Rect {
            height: area.height - 1,
            ..area
        };
        let status = Rect {
            y: area.y + area.height - 1,
            height: 1,
            ..area
        };
        match self.mode {
            Mode::Overview { .. } => self.draw_overview(content, buf),
            _ => self.draw_slide(content, buf),
        }
        if self.mode == Mode::Help {
            self.draw_help(content, buf);
        }
        self.draw_status(status, buf);
    }

    fn draw_slide(&mut self, area: Rect, buf: &mut Buffer) {
        let theme = self.current_theme();
        let w = area
            .width
            .saturating_sub(theme.margin.saturating_mul(2))
            .min(theme.max_width);
        if w < 10 || area.height < 2 {
            return;
        }
        let x = area.x + (area.width - w) / 2;

        enum RenderItem {
            Text(Text<'static>),
            Term(TermBlock),
        }
        let mut items: Vec<(Option<u16>, RenderItem)> = Vec::new();
        for seg in &self.deck.slide(self.col, self.row).segments {
            match seg {
                Segment::Markdown(src) => {
                    let text = markdown::render(src, w, &theme);
                    let h = text.height() as u16;
                    if h > 0 {
                        items.push((Some(h), RenderItem::Text(text)));
                    }
                }
                Segment::Terminal(b) => {
                    let h = if b.fill { None } else { Some(b.rows + 2) };
                    items.push((h, RenderItem::Term(b.clone())));
                }
            }
        }
        if items.is_empty() {
            return;
        }

        let box_h = area.height.min(theme.max_height);
        let gaps = items.len().saturating_sub(1) as u16;
        let fixed: u16 = items.iter().filter_map(|(h, _)| *h).sum();
        let nfill = items.iter().filter(|(h, _)| h.is_none()).count() as u16;
        let fill_h = box_h
            .saturating_sub(fixed + gaps)
            .checked_div(nfill)
            .map_or(0, |h| h.max(7));
        let heights: Vec<u16> = items.iter().map(|(h, _)| h.unwrap_or(fill_h)).collect();
        let total: u16 = heights.iter().sum::<u16>() + gaps;
        let visible = total.min(box_h);
        let mut y = area.y
            + if theme.vertical_align == VAlign::Center && visible < area.height {
                (area.height - visible) / 2
            } else {
                0
            };
        let bottom = y + visible;
        for ((_, item), h) in items.into_iter().zip(heights) {
            if y >= bottom {
                break;
            }
            let h = h.min(bottom - y);
            let rect = Rect {
                x,
                y,
                width: w,
                height: h,
            };
            match item {
                RenderItem::Text(text) => Paragraph::new(text).render(rect, buf),
                RenderItem::Term(block) => {
                    self.provider.prepare(
                        &block,
                        w.saturating_sub(2).max(4),
                        h.saturating_sub(2).max(1),
                    );
                    self.term_rects.push((block.id, rect));
                    self.draw_term(rect, &block, &theme, buf);
                }
            }
            y += h + 1;
        }
    }

    /// All terminal chrome; the provider draws the inside of the box.
    fn draw_term(&mut self, rect: Rect, block: &TermBlock, theme: &Theme, buf: &mut Buffer) {
        let focused = self.focus == Some(block.id);
        let state = self.provider.state(block);

        let label: String = block
            .command
            .as_deref()
            .and_then(|c| c.lines().next())
            .unwrap_or("shell")
            .chars()
            .take(40)
            .collect();

        let ids = self.current_ids();
        let idx = ids.iter().position(|i| *i == block.id).unwrap_or(0);
        let is_next = self.selected_terminal() == Some(block.id);
        let numbered = ids.len() > 1 && self.provider.interactive();
        let title = if numbered {
            let marker = if is_next && !focused { "▸ " } else { "" };
            format!(" {marker}{} · {label} ", idx + 1)
        } else {
            format!(" {label} ")
        };

        let hint = match &state {
            TermState::Failed(_) => "spawn failed".to_string(),
            TermState::Exited => "exited · R restarts".to_string(),
            TermState::Snapshot => "snapshot · runs live in the real TUI".to_string(),
            TermState::Unavailable => "runs live in the real TUI".to_string(),
            TermState::Running { scroll_offset, .. } if *scroll_offset > 0 => {
                format!("history -{scroll_offset} · shift-pgdn returns")
            }
            TermState::Running { .. } if focused => {
                "Ctrl-q releases · shift-pgup history".to_string()
            }
            TermState::Running { .. } if is_next => "t interacts".to_string(),
            TermState::Running { .. } => format!("{} selects", idx + 1),
        };

        let border_style = if focused {
            Style::default().fg(theme.accent)
        } else if state == TermState::Snapshot {
            Style::default().fg(theme.snapshot_border())
        } else {
            Style::default().fg(theme.term_border)
        };
        let title_style = if focused {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if is_next && numbered {
            Style::default().fg(theme.accent)
        } else {
            Style::default().fg(theme.muted)
        };

        let outer = Block::default()
            .borders(Borders::ALL)
            .border_type(theme.border_type)
            .border_style(border_style)
            .title(Line::from(title).style(title_style))
            .title_bottom(
                Line::from(format!(" {hint} "))
                    .style(Style::default().fg(theme.muted))
                    .right_aligned(),
            );
        let inner = outer.inner(rect);
        outer.render(rect, buf);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if let TermState::Failed(msg) = &state {
            Paragraph::new(msg.as_str())
                .style(Style::default().fg(Color::Red))
                .render(inner, buf);
            return;
        }
        self.provider.draw(block, inner, buf);

        if let TermState::Running {
            scroll_offset,
            scroll_total,
        } = state
            && scroll_total > 0
            && rect.height > 3
        {
            let viewport = inner.height as usize;
            let mut sb = ScrollbarState::new(scroll_total + viewport)
                .viewport_content_length(viewport)
                .position(scroll_total - scroll_offset.min(scroll_total));
            let thumb = if focused { theme.accent } else { theme.muted };
            StatefulWidget::render(
                Scrollbar::default()
                    .orientation(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None)
                    .track_style(border_style)
                    .thumb_style(Style::default().fg(thumb)),
                rect.inner(ratatui::layout::Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                buf,
                &mut sb,
            );
        }
    }

    fn draw_overview(&mut self, area: Rect, buf: &mut Buffer) {
        let Mode::Overview { sel } = self.mode else {
            return;
        };
        let bw: u16 = 26;
        let bh: u16 = 4;

        let mut header = vec![
            Span::styled(" overview ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                " — type a code or ↵ to jump · esc back",
                Style::default().fg(self.theme.muted),
            ),
        ];
        if !self.jump_input.is_empty() {
            header.push(Span::styled(
                format!("  {}…", self.jump_input),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        Paragraph::new(Line::from(header)).render(Rect { height: 1, ..area }, buf);
        let codes: HashMap<(usize, usize), String> = self.overview_codes().into_iter().collect();

        let grid = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(1),
        };
        if grid.width < bw || grid.height < bh {
            return;
        }
        let vis_cols = (grid.width / bw).max(1) as usize;
        let vis_rows = (grid.height / bh).max(1) as usize;
        let off_c = sel.0.saturating_sub(vis_cols - 1);
        let off_r = sel.1.saturating_sub(vis_rows - 1);

        for (ci, column) in self.deck.columns.iter().enumerate() {
            if ci < off_c || ci >= off_c + vis_cols {
                continue;
            }
            for (ri, slide) in column.slides.iter().enumerate() {
                if ri < off_r || ri >= off_r + vis_rows {
                    continue;
                }
                let rect = Rect {
                    x: grid.x + ((ci - off_c) as u16) * bw,
                    y: grid.y + ((ri - off_r) as u16) * bh,
                    width: bw - 1,
                    height: bh - 1,
                };
                self.overview_rects.push(((ci, ri), rect));
                let selected = (ci, ri) == sel;
                let current = (ci, ri) == (self.col, self.row);
                let border_style = if selected {
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else if current {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(self.theme.muted)
                };
                let mut title_spans = Vec::new();
                if let Some(code) = codes.get(&(ci, ri)) {
                    let candidate =
                        self.jump_input.is_empty() || code.starts_with(self.jump_input.as_str());
                    let code_style = if candidate {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.muted)
                    };
                    title_spans.push(Span::styled(format!(" {code} "), code_style));
                }
                title_spans.push(Span::styled(
                    format!("{}.{} ", ci + 1, ri + 1),
                    border_style,
                ));
                if current {
                    title_spans.push(Span::styled("● ", border_style));
                }
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_type(self.theme.border_type)
                    .border_style(border_style)
                    .title(Line::from(title_spans));
                let inner = block.inner(rect);
                block.render(rect, buf);

                let mut label: String = slide.title.chars().take((bw as usize) - 4).collect();
                if !slide.term_ids().is_empty() {
                    label.push_str(" ⌨");
                }
                Paragraph::new(label).render(inner, buf);
            }
        }
    }

    fn draw_help(&self, area: Rect, buf: &mut Buffer) {
        let dim = Style::default().fg(self.theme.muted);
        let key = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let row = |k: &str, desc: &str| {
            Line::from(vec![
                Span::styled(format!("  {k:<14}"), key),
                Span::raw(desc.to_string()),
            ])
        };
        let mut lines = vec![
            Line::from(Span::styled(" navigation", dim)),
            row("←/h  →/l", "previous / next column"),
            row("↓/j  ↑/k", "deeper / shallower"),
            row("space, n", "next slide (depth-first)"),
            row("shift-space", "previous slide (also bksp, p)"),
            row("g / G", "first / last column"),
            Line::default(),
        ];
        if self.provider.interactive() {
            lines.extend([
                Line::from(Span::styled(" terminals", dim)),
                row("1-9", "select terminal pane"),
                row("t, enter", "focus selected terminal"),
                row("click", "select + focus terminal"),
                row("ctrl-q", "release terminal focus (or click outside)"),
                row("shift-pgup/dn", "scroll terminal history (or wheel)"),
                row("R", "restart terminals on this slide"),
                Line::default(),
            ]);
        }
        lines.extend([
            Line::from(Span::styled(" modes", dim)),
            row("o", "overview (type a slide's code to jump)"),
            row("?", "this help"),
            row("q", "quit"),
        ]);
        if let Some(footer) = &self.help_footer {
            lines.push(Line::default());
            lines.push(Line::from(vec![
                Span::styled(" ", dim),
                Span::raw(footer.clone()),
            ]));
        }
        let w = 66.min(area.width);
        let h = (lines.len() as u16 + 2).min(area.height);
        let rect = Rect {
            x: area.x + (area.width - w) / 2,
            y: area.y + (area.height - h) / 2,
            width: w,
            height: h,
        };
        Clear.render(rect, buf);
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(self.theme.border_type)
                    .border_style(Style::default().fg(self.theme.accent))
                    .title(" help "),
            )
            .render(rect, buf);
    }

    fn draw_status(&self, area: Rect, buf: &mut Buffer) {
        let bar = Style::default()
            .bg(self.theme.status_bg)
            .fg(self.theme.status_fg);
        let depth = self.deck.columns[self.col].slides.len();
        let flat = self.deck.flat();
        let idx = flat
            .iter()
            .position(|&p| p == (self.col, self.row))
            .unwrap_or(0);

        let mut left = format!(" {} · {}.{}", self.deck.title, self.col + 1, self.row + 1);
        if depth > 1 {
            left.push_str(&format!(" · depth {}/{}", self.row + 1, depth));
        }
        left.push_str(&format!(" · {}/{}", idx + 1, flat.len()));
        if let Some(status) = &self.status {
            left.push_str(&format!("  —  {status}"));
        }
        Paragraph::new(left).style(bar).render(area, buf);

        let right = if self.focus.is_some() {
            Span::styled(
                " TERMINAL · ctrl-q releases ",
                Style::default()
                    // Indexed(16) is true black; ANSI Black gets remapped
                    // near-background by schemes like solarized.
                    .fg(Color::Indexed(16))
                    .bg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else if self.provider.interactive() {
            Span::styled(" space next · o overview · ? help ", bar)
        } else {
            Span::styled(" space next · o overview ", bar)
        };
        Paragraph::new(Line::from(right).right_aligned()).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A provider with no terminals at all, for exercising the core.
    struct NullProvider;
    impl TerminalProvider for NullProvider {
        fn prepare(&mut self, _: &TermBlock, _: u16, _: u16) {}
        fn state(&self, _: &TermBlock) -> TermState {
            TermState::Unavailable
        }
        fn draw(&mut self, _: &TermBlock, _: Rect, _: &mut Buffer) {}
        fn input(&mut self, _: usize, _: &[u8]) {}
        fn scroll(&mut self, _: usize, _: isize) {}
        fn restart(&mut self, _: &[usize]) {}
        fn interactive(&self) -> bool {
            false
        }
    }

    fn presenter() -> Presenter<NullProvider> {
        let deck = crate::deck::parse("# a\n---\n# b\n--\n# b2\n---\n# c\n", "t").unwrap();
        Presenter::new(deck, vec![], NullProvider).unwrap()
    }

    #[test]
    fn navigation_walks_the_grid() {
        let mut p = presenter();
        p.on_key(KeyPress::plain(Key::Char(' ')));
        assert_eq!(p.position(), (1, 0));
        p.on_key(KeyPress::plain(Key::Char(' ')));
        assert_eq!(p.position(), (1, 1));
        p.on_key(KeyPress {
            shift: true,
            ..KeyPress::plain(Key::Char(' '))
        });
        assert_eq!(p.position(), (1, 0));
        p.on_key(KeyPress::plain(Key::Char('G')));
        assert_eq!(p.position(), (2, 0));
    }

    #[test]
    fn overview_jump_codes_navigate() {
        let mut p = presenter();
        p.on_key(KeyPress::plain(Key::Char('o')));
        // 4 slides → single-letter codes; 'd' is the third (column 2 deep)
        p.on_key(KeyPress::plain(Key::Char('d')));
        assert_eq!(p.position(), (1, 1));
        assert!(matches!(p.mode, Mode::Slide));
    }

    #[test]
    fn non_interactive_context_refuses_focus() {
        let deck = crate::deck::parse("```terminal\n```\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        p.on_key(KeyPress::plain(Key::Char('t')));
        assert!(p.focus.is_none());
    }

    #[test]
    fn draws_without_panicking() {
        let mut p = presenter();
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        p.on_key(KeyPress::plain(Key::Char('o')));
        p.draw(area, &mut buf);
        p.on_key(KeyPress::plain(Key::Char('?')));
        p.draw(area, &mut buf);
    }
}
