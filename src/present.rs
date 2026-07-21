//! The presenter TUI.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use ratatui::Frame;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use tui_term::widget::PseudoTerminal;

use crate::deck::{Deck, Segment, TermBlock};
use crate::markdown;
use crate::proto::NotesState;
use crate::server::NotesServer;
use crate::source;
use crate::term::{TermSession, key_to_bytes};
use crate::theme::{self, Theme, ThemeConfig, VAlign};

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Slide,
    Overview { sel: (usize, usize) },
    Help,
}

enum RenderItem {
    Text(Text<'static>),
    Term(TermBlock),
}

/// Keys usable for overview jump codes, most ergonomic first. Excludes
/// the overview's own bindings (h j k l o q) so a code can never collide
/// with navigation.
const JUMP_KEYS: &[char] = &[
    'a', 's', 'd', 'f', 'g', 'e', 'r', 't', 'u', 'i', 'w', 'n', 'm', 'c', 'v', 'b', 'x', 'z', 'y',
    'p',
];

/// Hint codes for `n` slides: single letters while they last, otherwise
/// uniform two-letter codes (no prefix ambiguity either way).
fn jump_codes(n: usize) -> Vec<String> {
    if n <= JUMP_KEYS.len() {
        return JUMP_KEYS.iter().take(n).map(char::to_string).collect();
    }
    let mut out = Vec::with_capacity(n);
    'outer: for a in JUMP_KEYS {
        for b in JUMP_KEYS {
            if out.len() >= n {
                break 'outer;
            }
            out.push(format!("{a}{b}"));
        }
    }
    out
}

pub struct App {
    deck: Deck,
    deck_dir: PathBuf,
    col: usize,
    row: usize,
    depth_memory: Vec<usize>,
    mode: Mode,
    focus: Option<usize>,
    /// Per-slide: which terminal `t` focuses next (set by 1-9 or click).
    selected: HashMap<(usize, usize), usize>,
    /// Screen rects of terminals drawn last frame, for mouse hit-testing.
    term_rects: Vec<(usize, Rect)>,
    /// Screen rects of overview boxes drawn last frame.
    overview_rects: Vec<((usize, usize), Rect)>,
    sessions: HashMap<usize, TermSession>,
    failed: HashMap<usize, String>,
    server: NotesServer,
    socket_path: PathBuf,
    started_at: u64,
    status: Option<String>,
    /// Partially-typed overview jump code.
    jump_input: String,
    theme: Theme,
    slide_themes: HashMap<(usize, usize), Theme>,
    quit: bool,
}

pub fn run(input: &str, socket: PathBuf) -> Result<()> {
    let source::Loaded {
        deck,
        theme: deck_theme,
        base_dir: deck_dir,
    } = source::load(input)?;
    let base_cfgs: Vec<ThemeConfig> = [theme::user_config()?, deck_theme]
        .into_iter()
        .flatten()
        .collect();
    let theme = Theme::resolve(base_cfgs.clone())?;
    let mut slide_themes = HashMap::new();
    for (c, column) in deck.columns.iter().enumerate() {
        for (r, slide) in column.slides.iter().enumerate() {
            if let Some(cfg) = &slide.theme {
                let mut cfgs = base_cfgs.clone();
                cfgs.push(cfg.clone());
                let resolved = Theme::resolve(cfgs)
                    .with_context(|| format!("theme for slide {}.{}", c + 1, r + 1))?;
                slide_themes.insert((c, r), resolved);
            }
        }
    }
    let server = NotesServer::start(socket.clone()).context("starting presenter-notes server")?;
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let ncols = deck.columns.len();
    let mut app = App {
        deck,
        deck_dir,
        col: 0,
        row: 0,
        depth_memory: vec![0; ncols],
        mode: Mode::Slide,
        focus: None,
        selected: HashMap::new(),
        term_rects: Vec::new(),
        overview_rects: Vec::new(),
        sessions: HashMap::new(),
        failed: HashMap::new(),
        server,
        socket_path: socket,
        started_at,
        status: None,
        jump_input: String::new(),
        theme,
        slide_themes,
        quit: false,
    };
    app.broadcast();

    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let result = app.event_loop(&mut terminal);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    app.shutdown();
    result
}

impl App {
    fn event_loop(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        while !self.quit {
            terminal.draw(|f| self.draw(f))?;
            if event::poll(Duration::from_millis(33))? {
                match event::read()? {
                    Event::Key(k)
                        if matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                    {
                        self.on_key(k)
                    }
                    Event::Mouse(m) => self.on_mouse(m),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    fn shutdown(&mut self) {
        for (_, mut s) in self.sessions.drain() {
            s.kill();
        }
    }

    // ------------------------------------------------------------------ nav

    fn current_ids(&self) -> Vec<usize> {
        self.deck.slide(self.col, self.row).term_ids()
    }

    /// Theme for the slide being shown: its own override, or the deck theme.
    fn current_theme(&self) -> Theme {
        self.slide_themes
            .get(&(self.col, self.row))
            .copied()
            .unwrap_or(self.theme)
    }

    fn goto(&mut self, col: usize, row: usize) {
        let col = col.min(self.deck.columns.len() - 1);
        let row = row.min(self.deck.columns[col].slides.len() - 1);
        if (col, row) != (self.col, self.row) {
            self.col = col;
            self.row = row;
            self.depth_memory[col] = row;
            self.focus = None;
            self.broadcast();
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

    fn broadcast(&self) {
        let flat = self.deck.flat();
        let idx = flat
            .iter()
            .position(|&p| p == (self.col, self.row))
            .unwrap_or(0);
        let next_title = flat
            .get(idx + 1)
            .map(|&(c, r)| self.deck.slide(c, r).title.clone());
        let slide = self.deck.slide(self.col, self.row);
        self.server.broadcast(&NotesState {
            deck_title: self.deck.title.clone(),
            col: self.col,
            row: self.row,
            total_cols: self.deck.columns.len(),
            col_depth: self.deck.columns[self.col].slides.len(),
            slide_no: idx + 1,
            total_slides: flat.len(),
            title: slide.title.clone(),
            notes: slide.notes.clone(),
            next_title,
            started_at: self.started_at,
        });
    }

    // ----------------------------------------------------------------- keys

    fn on_key(&mut self, key: KeyEvent) {
        self.status = None;

        if let Some(id) = self.focus {
            if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                self.focus = None;
                return;
            }
            let exited = self.sessions.get(&id).map(|s| s.exited()).unwrap_or(true);
            if exited {
                self.focus = None;
                return;
            }
            if key.modifiers.contains(KeyModifiers::SHIFT)
                && let Some(s) = self.sessions.get_mut(&id)
            {
                match key.code {
                    KeyCode::PageUp => return s.scroll_page(true),
                    KeyCode::PageDown => return s.scroll_page(false),
                    KeyCode::Up => return s.scroll_lines(1),
                    KeyCode::Down => return s.scroll_lines(-1),
                    _ => {}
                }
            }
            if let (Some(s), Some(bytes)) = (self.sessions.get_mut(&id), key_to_bytes(&key)) {
                s.write_input(&bytes);
            }
            return;
        }

        match self.mode {
            Mode::Help => self.mode = Mode::Slide,
            Mode::Overview { sel } => self.on_key_overview(key, sel),
            Mode::Slide => self.on_key_slide(key),
        }
    }

    fn on_key_slide(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if ctrl => self.quit = true,
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Left | KeyCode::Char('h') => {
                if self.col > 0 {
                    self.goto(self.col - 1, self.depth_memory[self.col - 1]);
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.col + 1 < self.deck.columns.len() {
                    self.goto(self.col + 1, self.depth_memory[self.col + 1]);
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.goto(self.col, self.row + 1),
            KeyCode::Up | KeyCode::Char('k') => self.goto(self.col, self.row.saturating_sub(1)),
            KeyCode::Char(' ') | KeyCode::Char('n') | KeyCode::PageDown => self.next(),
            KeyCode::Backspace | KeyCode::Char('p') | KeyCode::PageUp => self.prev(),
            KeyCode::Char('g') | KeyCode::Home => self.goto(0, 0),
            KeyCode::Char('G') | KeyCode::End => self.goto(self.deck.columns.len() - 1, 0),
            KeyCode::Char('o') => {
                self.jump_input.clear();
                self.mode = Mode::Overview {
                    sel: (self.col, self.row),
                }
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Enter | KeyCode::Char('t') => self.focus_terminal(),
            KeyCode::Char(c @ '1'..='9') => self.select_terminal(c as usize - '0' as usize),
            KeyCode::Char('R') => self.restart_terminals(),
            _ => {}
        }
    }

    /// The terminal `t` will focus on this slide: the explicit selection if
    /// still valid, else the first terminal.
    fn selected_terminal(&self) -> Option<usize> {
        let ids = self.current_ids();
        self.selected
            .get(&(self.col, self.row))
            .copied()
            .filter(|id| ids.contains(id))
            .or_else(|| ids.first().copied())
    }

    fn select_terminal(&mut self, n: usize) {
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
        let Some(id) = self.selected_terminal() else {
            self.status = Some("no terminal on this slide".to_string());
            return;
        };
        self.selected.insert((self.col, self.row), id);
        self.focus = Some(id);
    }

    fn on_mouse(&mut self, m: MouseEvent) {
        let pos = Position::new(m.column, m.row);
        let term_at = |rects: &[(usize, Rect)]| {
            rects
                .iter()
                .find(|(_, r)| r.contains(pos))
                .map(|(id, _)| *id)
        };
        match self.mode {
            Mode::Help => {
                if matches!(m.kind, MouseEventKind::Down(_)) {
                    self.mode = Mode::Slide;
                }
            }
            Mode::Overview { .. } => {
                if matches!(m.kind, MouseEventKind::Down(MouseButton::Left))
                    && let Some(&(target, _)) =
                        self.overview_rects.iter().find(|(_, r)| r.contains(pos))
                {
                    self.mode = Mode::Slide;
                    self.goto(target.0, target.1);
                }
            }
            Mode::Slide => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => match term_at(&self.term_rects) {
                    Some(id) => {
                        self.selected.insert((self.col, self.row), id);
                        self.focus = Some(id);
                    }
                    None => self.focus = None,
                },
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    if let Some(id) = term_at(&self.term_rects)
                        && let Some(s) = self.sessions.get_mut(&id)
                    {
                        let delta = if m.kind == MouseEventKind::ScrollUp {
                            3
                        } else {
                            -3
                        };
                        s.scroll_lines(delta);
                    }
                }
                _ => {}
            },
        }
    }

    fn restart_terminals(&mut self) {
        let ids = self.current_ids();
        if ids.is_empty() {
            return;
        }
        for id in ids {
            if let Some(mut s) = self.sessions.remove(&id) {
                s.kill();
            }
            self.failed.remove(&id);
        }
        self.status = Some("terminals restarted".to_string());
    }

    /// (slide position, code) pairs in depth-first order.
    fn overview_codes(&self) -> Vec<((usize, usize), String)> {
        let flat = self.deck.flat();
        let codes = jump_codes(flat.len());
        flat.into_iter().zip(codes).collect()
    }

    fn on_key_overview(&mut self, key: KeyEvent, sel: (usize, usize)) {
        // Jump codes take priority: the alphabet excludes every key
        // bound below, so this can't shadow navigation.
        if let KeyCode::Char(ch) = key.code
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
        match key.code {
            // With a partial code pending, esc just cancels it.
            KeyCode::Esc if had_pending => return,
            KeyCode::Esc | KeyCode::Char('o') => {
                self.mode = Mode::Slide;
                return;
            }
            KeyCode::Char('q') => {
                self.quit = true;
                return;
            }
            KeyCode::Enter => {
                self.mode = Mode::Slide;
                self.goto(c, r);
                return;
            }
            KeyCode::Left | KeyCode::Char('h') => c = c.saturating_sub(1),
            KeyCode::Right | KeyCode::Char('l') => c = (c + 1).min(self.deck.columns.len() - 1),
            KeyCode::Up | KeyCode::Char('k') => r = r.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => r += 1,
            _ => {}
        }
        r = r.min(self.deck.columns[c].slides.len() - 1);
        self.mode = Mode::Overview { sel: (c, r) };
    }

    // ----------------------------------------------------------------- draw

    fn draw(&mut self, f: &mut Frame) {
        self.term_rects.clear();
        self.overview_rects.clear();
        let [content, status] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());
        match self.mode {
            Mode::Overview { .. } => self.draw_overview(f, content),
            _ => self.draw_slide(f, content),
        }
        if self.mode == Mode::Help {
            self.draw_help(f, content);
        }
        self.draw_status(f, status);
    }

    fn draw_slide(&mut self, f: &mut Frame, area: Rect) {
        let theme = self.current_theme();
        let w = area
            .width
            .saturating_sub(theme.margin.saturating_mul(2))
            .min(theme.max_width);
        if w < 10 || area.height < 2 {
            return;
        }
        let x = area.x + (area.width - w) / 2;

        // Build the render plan. `None` height marks a fill terminal that
        // expands into whatever the fixed items leave behind.
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

        // The content box: full area, capped by the theme's max_height.
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
                RenderItem::Text(text) => f.render_widget(Paragraph::new(text), rect),
                RenderItem::Term(block) => {
                    self.ensure_session(&block, w.saturating_sub(2), h.saturating_sub(2).max(1));
                    self.term_rects.push((block.id, rect));
                    self.draw_term(f, rect, &block);
                }
            }
            y += h + 1;
        }
    }

    fn ensure_session(&mut self, block: &TermBlock, cols: u16, rows: u16) {
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

    fn draw_term(&self, f: &mut Frame, rect: Rect, block: &TermBlock) {
        let focused = self.focus == Some(block.id);
        let failed = self.failed.get(&block.id);
        let exited = self
            .sessions
            .get(&block.id)
            .map(|s| s.exited())
            .unwrap_or(false);

        let label: String = block
            .command
            .as_deref()
            .and_then(|c| c.lines().next())
            .unwrap_or("shell")
            .chars()
            .take(40)
            .collect();
        let scrolled = self
            .sessions
            .get(&block.id)
            .map(|s| s.scroll_offset())
            .unwrap_or(0);

        // Pane numbering: only shown when the slide has several terminals.
        // `▸` marks the one `t` will focus.
        let ids = self.current_ids();
        let idx = ids.iter().position(|i| *i == block.id).unwrap_or(0);
        let is_next = self.selected_terminal() == Some(block.id);
        let title = if ids.len() > 1 {
            let marker = if is_next && !focused { "▸ " } else { "" };
            format!(" {marker}{} · {label} ", idx + 1)
        } else {
            format!(" {label} ")
        };

        let hint = if failed.is_some() {
            "spawn failed".to_string()
        } else if exited {
            "exited · R restarts".to_string()
        } else if scrolled > 0 {
            format!("history -{scrolled} · shift-pgdn returns")
        } else if focused {
            "Ctrl-q releases · shift-pgup history".to_string()
        } else if is_next {
            "t interacts".to_string()
        } else {
            format!("{} selects", idx + 1)
        };
        let theme = self.current_theme();
        let border_style = if focused {
            Style::default().fg(theme.accent)
        } else {
            Style::default().fg(theme.term_border)
        };
        let title_style = if focused {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else if is_next && ids.len() > 1 {
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
        f.render_widget(outer, rect);
        if inner.width == 0 || inner.height == 0 {
            return;
        }

        if let Some(err) = failed {
            f.render_widget(
                Paragraph::new(err.as_str()).style(Style::default().fg(Color::Red)),
                inner,
            );
        } else if let Some(session) = self.sessions.get(&block.id) {
            {
                let parser = session.parser.lock().unwrap();
                f.render_widget(PseudoTerminal::new(parser.screen()), inner);
            }
            // Scrollbar on the right border once there's history to scroll.
            let (offset, avail) = session.scroll_info();
            if avail > 0 && rect.height > 3 {
                let viewport = inner.height as usize;
                let mut state = ScrollbarState::new(avail + viewport)
                    .viewport_content_length(viewport)
                    .position(avail - offset.min(avail));
                let thumb = if focused { theme.accent } else { theme.muted };
                f.render_stateful_widget(
                    Scrollbar::default()
                        .orientation(ScrollbarOrientation::VerticalRight)
                        .begin_symbol(None)
                        .end_symbol(None)
                        .track_style(border_style)
                        .thumb_style(Style::default().fg(thumb)),
                    rect.inner(Margin {
                        vertical: 1,
                        horizontal: 0,
                    }),
                    &mut state,
                );
            }
        }
    }

    fn draw_overview(&mut self, f: &mut Frame, area: Rect) {
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
        f.render_widget(
            Paragraph::new(Line::from(header)),
            Rect { height: 1, ..area },
        );
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
                f.render_widget(block, rect);

                let mut label: String = slide.title.chars().take((bw as usize) - 4).collect();
                if !slide.term_ids().is_empty() {
                    label.push_str(" ⌨");
                }
                f.render_widget(Paragraph::new(label), inner);
            }
        }
    }

    fn draw_help(&self, f: &mut Frame, area: Rect) {
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
        let lines = vec![
            Line::from(Span::styled(" navigation", dim)),
            row("←/h  →/l", "previous / next column"),
            row("↓/j  ↑/k", "deeper / shallower"),
            row("space, n", "next slide (depth-first)"),
            row("bksp, p", "previous slide"),
            row("g / G", "first / last column"),
            Line::default(),
            Line::from(Span::styled(" terminals", dim)),
            row("1-9", "select terminal pane"),
            row("t, enter", "focus selected terminal"),
            row("click", "select + focus terminal"),
            row("ctrl-q", "release terminal focus (or click outside)"),
            row("shift-pgup/dn", "scroll terminal history (or wheel)"),
            row("R", "restart terminals on this slide"),
            Line::default(),
            Line::from(Span::styled(" modes", dim)),
            row("o", "overview (type a slide's code to jump)"),
            row("?", "this help"),
            row("q", "quit"),
            Line::default(),
            Line::from(vec![
                Span::styled(" notes: ", dim),
                Span::raw(format!(
                    "deckhand notes --socket {}",
                    self.socket_path.display()
                )),
            ]),
        ];
        let w = 66.min(area.width);
        let h = (lines.len() as u16 + 2).min(area.height);
        let rect = Rect {
            x: area.x + (area.width - w) / 2,
            y: area.y + (area.height - h) / 2,
            width: w,
            height: h,
        };
        f.render_widget(Clear, rect);
        f.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(self.theme.border_type)
                    .border_style(Style::default().fg(self.theme.accent))
                    .title(" help "),
            ),
            rect,
        );
    }

    fn draw_status(&self, f: &mut Frame, area: Rect) {
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
        f.render_widget(Paragraph::new(left).style(bar), area);

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
        } else {
            Span::styled(" space next · o overview · ? help ", bar)
        };
        f.render_widget(Paragraph::new(Line::from(right).right_aligned()), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_codes_are_unique_and_avoid_nav_keys() {
        for n in [1, 5, 20, 21, 100, 400] {
            let codes = jump_codes(n);
            assert_eq!(codes.len(), n);
            let unique: std::collections::HashSet<_> = codes.iter().collect();
            assert_eq!(unique.len(), codes.len());
            for code in &codes {
                assert!(
                    !code.chars().any(|c| "hjkloq".contains(c)),
                    "code {code:?} collides with overview navigation"
                );
            }
        }
    }

    #[test]
    fn jump_codes_shape() {
        assert_eq!(jump_codes(3), vec!["a", "s", "d"]);
        // beyond one letter per slide, all codes are uniform two-letter
        let codes = jump_codes(21);
        assert!(codes.iter().all(|c| c.chars().count() == 2));
        assert_eq!(codes[0], "aa");
    }
}
