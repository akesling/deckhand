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
use crate::theme::{HAlign, Theme, ThemeConfig, VAlign};

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
    /// The block hasn't been approved to run yet: its command is shown
    /// and the presenter confirms with `t`, then `y`.
    Disabled,
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
    /// A wheel tick over terminal `id`, at cell (col, row) inside it.
    /// Default: move the local history view three lines.
    fn wheel(&mut self, id: usize, up: bool, col: u16, row: u16) {
        let _ = (col, row);
        self.scroll(id, if up { 3 } else { -3 });
    }
    /// A left click at (col, row) inside the focused terminal `id` —
    /// true when the child program consumed it (it reports mouse),
    /// false to keep the presenter's own click semantics. Default:
    /// never consumed.
    fn click(&mut self, _id: usize, _col: u16, _row: u16) -> bool {
        false
    }
    /// Kill and forget these terminals so they respawn (the `R` key).
    fn restart(&mut self, ids: &[usize]);
    /// The deck was replaced and its terminal blocks changed: drop all
    /// live terminal state so blocks start cleanly. Default: no-op.
    fn reset(&mut self) {}
    /// The presenter confirmed running this [`TermState::Disabled`]
    /// block (the in-deck `y` confirmation). Default: no-op.
    fn enable(&mut self, _id: usize) {}
    /// Stop this terminal and withdraw its approval — it returns to
    /// [`TermState::Disabled`] (the `s` key). Default: no-op.
    fn stop(&mut self, _id: usize) {}
    /// Whether terminals can be focused and typed into in this context.
    fn interactive(&self) -> bool {
        true
    }
}

// -------------------------------------------------------------- presenter

/// All terminal blocks in deck order; two decks with equal lists have
/// identical terminal ids, so live sessions can safely carry over.
fn term_blocks(deck: &Deck) -> Vec<&TermBlock> {
    deck.columns
        .iter()
        .flat_map(|c| &c.slides)
        .flat_map(|s| s.leaf_segments())
        .filter_map(|seg| match seg {
            Segment::Terminal(b) => Some(b),
            _ => None,
        })
        .collect()
}

/// Columns of blank space between row cells.
const ROW_GAP: u16 = 2;

/// First visible index for a one-axis viewport of `vis` slots over
/// `total` items: keeps `sel` one slot in from the trailing edge while
/// more items exist beyond it, so the neighbor you're heading toward
/// is always in view.
fn viewport_offset(sel: usize, total: usize, vis: usize) -> usize {
    if total <= vis {
        return 0;
    }
    (sel + 2).saturating_sub(vis).min(total - vis).min(sel)
}

/// If markdown source opens with a heading (blank lines aside), split
/// it off: (heading line, everything after). Feeds `pin_title`.
fn split_leading_heading(src: &str) -> Option<(String, String)> {
    let mut consumed = 0usize;
    for part in src.split_inclusive('\n') {
        let t = part.trim();
        consumed += part.len();
        if t.is_empty() {
            continue;
        }
        if t.starts_with('#') {
            return Some((t.to_string(), src[consumed..].to_string()));
        }
        return None;
    }
    None
}

/// One fixed-height piece of a row cell (or a full-width segment —
/// the same builder serves both).
enum CellItem {
    Text(Text<'static>),
    Term(TermBlock),
}

/// Render segments into fixed-height items at `w` columns. Inside
/// cells, fill terminals fall back to their fixed row count — "the
/// rest of the slide" isn't a height a side-by-side cell can claim.
fn build_cell_items(
    segments: &[Segment],
    w: u16,
    box_h: u16,
    theme: &Theme,
) -> Vec<(u16, CellItem)> {
    let mut items = Vec::new();
    for seg in segments {
        match seg {
            Segment::Markdown(src) => {
                let text = markdown::render(src, w, theme);
                let h = text.height() as u16;
                if h > 0 {
                    items.push((h, CellItem::Text(text)));
                }
            }
            Segment::Terminal(b) => items.push((b.rows + 2, CellItem::Term(b.clone()))),
            Segment::Qr(q) => {
                let text = qr_text(q, w, theme);
                items.push((text.height() as u16, CellItem::Text(text)));
            }
            Segment::Image(img) => {
                let text = match &img.data {
                    Some(data) => crate::ascii_image::render(data, w, box_h),
                    None => image_placeholder(img, theme),
                };
                items.push((text.height() as u16, CellItem::Text(text)));
            }
            // The parser keeps rows one level deep.
            Segment::Row(_) => {}
        }
    }
    items
}

/// A QR block as centered lines, colored by the theme's
/// `qr_dark`/`qr_light` — true black-on-white by default, because
/// scanners want a dark code on a light background and a themed
/// terminal guarantees neither. Themes that override this own its
/// scannability.
fn qr_text(q: &crate::deck::QrBlock, w: u16, theme: &Theme) -> Text<'static> {
    let qw = q.rows.first().map(|r| r.chars().count()).unwrap_or(0);
    if qw > w as usize {
        return Text::from(Line::from(Span::styled(
            format!("[qr code needs {qw} columns]"),
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::ITALIC),
        )));
    }
    let style = Style::default().fg(theme.qr_dark).bg(theme.qr_light);
    let pad = " ".repeat((w as usize - qw) / 2);
    Text::from(
        q.rows
            .iter()
            .map(|row| {
                Line::from(vec![
                    Span::raw(pad.clone()),
                    Span::styled(row.clone(), style),
                ])
            })
            .collect::<Vec<_>>(),
    )
}

/// What an image block shows when its pixels aren't loaded: alt text,
/// plus the reason (load failure natively, no filesystem on the web).
fn image_placeholder(img: &crate::deck::ImageBlock, theme: &Theme) -> Text<'static> {
    let muted = Style::default().fg(theme.muted);
    let label = if img.alt.is_empty() {
        img.path.as_str()
    } else {
        img.alt.as_str()
    };
    let mut lines = vec![Line::from(Span::styled(format!("▦ image: {label}"), muted))];
    let detail = match &img.error {
        Some(e) => e.clone(),
        None => format!(
            "{} — renders as ASCII art in the native presenter",
            img.path
        ),
    };
    lines.push(Line::from(Span::styled(
        format!("  {detail}"),
        muted.add_modifier(Modifier::ITALIC),
    )));
    Text::from(lines)
}

/// Resolve per-slide theme overrides against the base config stack.
fn resolve_slide_themes(
    deck: &Deck,
    base: &[ThemeConfig],
) -> Result<HashMap<(usize, usize), Theme>> {
    let mut out = HashMap::new();
    for (c, column) in deck.columns.iter().enumerate() {
        for (r, slide) in column.slides.iter().enumerate() {
            if let Some(cfg) = &slide.theme {
                let mut cfgs = base.to_vec();
                cfgs.push(cfg.clone());
                let resolved = Theme::resolve(cfgs)
                    .with_context(|| format!("theme for slide {}.{}", c + 1, r + 1))?;
                out.insert((c, r), resolved);
            }
        }
    }
    Ok(out)
}

/// One search result: a slide plus where the query hit, for
/// highlighting in the results list.
struct SearchMatch {
    pos: (usize, usize),
    title: String,
    /// Byte range of the hit in `title`, when the title matched.
    title_hit: Option<(usize, usize)>,
    /// First matching body line (trimmed) and the hit's byte range.
    snippet: Option<(String, (usize, usize))>,
}

/// Byte range of the first case-insensitive occurrence of `needle` in
/// `haystack`, comparing per-char lowercase forms so the range is
/// always on char boundaries of the original string.
fn find_ci(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let needle: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return Some((0, 0));
    }
    for (start, _) in haystack.char_indices() {
        let mut matched = 0;
        for (i, c) in haystack[start..].char_indices() {
            let mut ok = true;
            for lc in c.to_lowercase() {
                if matched < needle.len() && lc == needle[matched] {
                    matched += 1;
                } else {
                    ok = false;
                    break;
                }
            }
            if !ok {
                break;
            }
            if matched == needle.len() {
                return Some((start, start + i + c.len_utf8()));
            }
        }
    }
    None
}

/// Re-cut a snippet whose hit sits deep in the line so the highlight
/// stays visible once the paragraph clips the tail: keep a little
/// context before the match behind a `…`.
fn clip_snippet(line: &str, range: (usize, usize)) -> (String, (usize, usize)) {
    const KEEP_BYTES: usize = 40;
    const LEAD_CHARS: usize = 16;
    if range.0 <= KEEP_BYTES {
        return (line.to_string(), range);
    }
    let starts: Vec<usize> = line.char_indices().map(|(i, _)| i).collect();
    let hit = starts.partition_point(|&i| i < range.0);
    let cut = starts[hit.saturating_sub(LEAD_CHARS)];
    let shift = |b: usize| b - cut + "…".len();
    (
        format!("…{}", &line[cut..]),
        (shift(range.0), shift(range.1)),
    )
}

/// `text` as spans with the hit range (if any) restyled — falls back
/// to one plain span if the range isn't on char boundaries.
fn split_hit(
    text: &str,
    range: Option<(usize, usize)>,
    base: Style,
    hit: Style,
) -> Vec<Span<'static>> {
    match range {
        Some((s, e))
            if s < e && e <= text.len() && text.is_char_boundary(s) && text.is_char_boundary(e) =>
        {
            vec![
                Span::styled(text[..s].to_string(), base),
                Span::styled(text[s..e].to_string(), hit),
                Span::styled(text[e..].to_string(), base),
            ]
        }
        _ => vec![Span::styled(text.to_string(), base)],
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Slide,
    Overview { sel: (usize, usize) },
    Help,
    Search { sel: usize },
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
    search_input: String,
    /// A disabled terminal awaiting the presenter's `y` to run.
    pending_enable: Option<usize>,
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
        let slide_themes = resolve_slide_themes(&deck, &theme_cfgs)?;
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
            search_input: String::new(),
            pending_enable: None,
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

    /// Show a transient message in the status bar (cleared on the next
    /// key press). Front ends use it for live-reload feedback.
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
    }

    /// Swap in a rebuilt deck — e.g. after its files changed on disk —
    /// preserving position (clamped) and, when the terminal blocks are
    /// unchanged, live terminal state and focus. When they changed, the
    /// provider is [`reset`](TerminalProvider::reset) so ids can't attach
    /// to the wrong sessions.
    pub fn replace_deck(&mut self, deck: Deck, theme_cfgs: Vec<ThemeConfig>) -> Result<()> {
        anyhow::ensure!(!deck.columns.is_empty(), "deck contains no slides");
        let theme = Theme::resolve(theme_cfgs.clone())?;
        let slide_themes = resolve_slide_themes(&deck, &theme_cfgs)?;

        let same_terms = term_blocks(&self.deck) == term_blocks(&deck);
        if !same_terms {
            self.provider.reset();
            self.focus = None;
            self.selected.clear();
        }

        self.deck = deck;
        self.theme = theme;
        self.slide_themes = slide_themes;

        let ncols = self.deck.columns.len();
        self.depth_memory.resize(ncols, 0);
        for (c, depth) in self.depth_memory.iter_mut().enumerate() {
            *depth = (*depth).min(self.deck.columns[c].slides.len() - 1);
        }
        self.col = self.col.min(ncols - 1);
        self.row = self.row.min(self.deck.columns[self.col].slides.len() - 1);
        if let Mode::Overview { sel: (c, r) } = self.mode {
            let c = c.min(ncols - 1);
            let r = r.min(self.deck.columns[c].slides.len() - 1);
            self.mode = Mode::Overview { sel: (c, r) };
        }
        self.jump_input.clear();
        self.pending_enable = None;
        Ok(())
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
            self.pending_enable = None;
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

        // A disabled terminal is awaiting confirmation: `y` runs it,
        // anything else cancels (take() clears the pending state) and
        // is then handled normally.
        if let Some(id) = self.pending_enable.take()
            && matches!(key.key, Key::Char('y') | Key::Char('Y'))
        {
            self.provider.enable(id);
            self.focus = Some(id);
            return;
        }

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
            Mode::Search { sel } => self.on_key_search(key, sel),
            Mode::Slide => self.on_key_slide(key),
        }
    }

    fn block_by_id(&self, id: usize) -> TermBlock {
        for column in &self.deck.columns {
            for slide in &column.slides {
                for seg in slide.leaf_segments() {
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
            Key::Char('/') => {
                self.search_input.clear();
                self.mode = Mode::Search { sel: 0 };
            }
            Key::Char('?') => self.mode = Mode::Help,
            Key::Enter | Key::Char('t') => self.focus_terminal(),
            Key::Char(c @ '1'..='9') => self.select_terminal(c as usize - '0' as usize),
            Key::Char('s') => self.stop_terminal(),
            Key::Char('R') => self.restart_terminal(),
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
        // Unapproved blocks ask before running anything.
        if self.provider.state(&self.block_by_id(id)) == TermState::Disabled {
            let command = self.block_label(id);
            self.pending_enable = Some(id);
            self.status = Some(format!(
                "run `{command}`? y confirms · any other key cancels"
            ));
            return;
        }
        self.focus = Some(id);
    }

    fn block_label(&self, id: usize) -> String {
        let block = self.block_by_id(id);
        block
            .command
            .as_deref()
            .and_then(|c| c.lines().next())
            .unwrap_or("shell")
            .to_string()
    }

    fn stop_terminal(&mut self) {
        let Some(id) = self.selected_terminal() else {
            return;
        };
        match self.provider.state(&self.block_by_id(id)) {
            TermState::Running { .. } | TermState::Exited | TermState::Failed(_) => {
                self.provider.stop(id);
                self.focus = None;
                self.status = Some(format!("stopped `{}`", self.block_label(id)));
            }
            _ => {}
        }
    }

    fn restart_terminal(&mut self) {
        let Some(id) = self.selected_terminal() else {
            return;
        };
        match self.provider.state(&self.block_by_id(id)) {
            TermState::Running { .. } | TermState::Exited | TermState::Failed(_) => {
                self.provider.restart(&[id]);
                self.status = Some(format!("restarted `{}`", self.block_label(id)));
            }
            TermState::Disabled => {
                self.status = Some("not running — t runs it".to_string());
            }
            _ => {}
        }
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

    fn on_key_search(&mut self, key: KeyPress, sel: usize) {
        let count = self.search_matches().len();
        let clamp = |s: usize| s.min(count.saturating_sub(1));
        match key.key {
            Key::Char('c') if key.ctrl => self.quit = true,
            Key::Esc => self.mode = Mode::Slide,
            Key::Enter => {
                if count > 0 {
                    let (c, r) = self.search_matches()[clamp(sel)].pos;
                    self.mode = Mode::Slide;
                    self.goto(c, r);
                }
            }
            Key::Backspace => {
                if self.search_input.pop().is_none() {
                    self.mode = Mode::Slide;
                } else {
                    self.mode = Mode::Search { sel: 0 };
                }
            }
            Key::Tab if key.shift => {
                self.mode = Mode::Search {
                    sel: sel.saturating_sub(1),
                }
            }
            Key::Down | Key::Tab => {
                self.mode = Mode::Search {
                    sel: clamp(sel + 1),
                }
            }
            Key::Up | Key::BackTab => {
                self.mode = Mode::Search {
                    sel: sel.saturating_sub(1),
                }
            }
            Key::Char('n') if key.ctrl => {
                self.mode = Mode::Search {
                    sel: clamp(sel + 1),
                }
            }
            Key::Char('p') if key.ctrl => {
                self.mode = Mode::Search {
                    sel: sel.saturating_sub(1),
                }
            }
            Key::Char(ch) if !key.ctrl && !key.alt => {
                self.search_input.push(ch);
                self.mode = Mode::Search { sel: 0 };
            }
            _ => {}
        }
    }

    /// Slides matching the current query (case-insensitive substring
    /// over [`Slide::search_text`]), in traversal order. An empty query
    /// matches every slide, so `/` doubles as a title picker.
    fn search_matches(&self) -> Vec<SearchMatch> {
        let query = self.search_input.as_str();
        let mut out = Vec::new();
        for (c, r) in self.deck.flat() {
            let slide = self.deck.slide(c, r);
            if query.is_empty() {
                out.push(SearchMatch {
                    pos: (c, r),
                    title: slide.title.clone(),
                    title_hit: None,
                    snippet: None,
                });
                continue;
            }
            let title_hit = find_ci(&slide.title, query);
            let text = slide.search_text();
            let snippet = text
                .lines()
                .skip(1)
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .find_map(|l| find_ci(l, query).map(|range| clip_snippet(l, range)));
            if title_hit.is_none() && snippet.is_none() {
                continue;
            }
            out.push(SearchMatch {
                pos: (c, r),
                title: slide.title.clone(),
                title_hit,
                snippet,
            });
        }
        out
    }

    /// Whether a terminal lies under (x, y) on the current slide — the
    /// web front end asks before claiming wheel events for the deck.
    pub fn terminal_at(&self, x: u16, y: u16) -> bool {
        matches!(self.mode, Mode::Slide)
            && self
                .term_rects
                .iter()
                .any(|(_, r)| r.contains(Position::new(x, y)))
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
            Mode::Help | Mode::Search { .. } => {
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
                    // A focused, mouse-aware program gets the click
                    // itself; otherwise clicks manage focus/selection.
                    if let Some(fid) = self.focus
                        && let Some(&(_, r)) = self.term_rects.iter().find(|(id, _)| *id == fid)
                        && r.contains(pos)
                        && self.provider.click(fid, m.x - r.x, m.y - r.y)
                    {
                        return;
                    }
                    self.pending_enable = None;
                    match term_at(&self.term_rects) {
                        Some(id) => {
                            self.selected.insert((self.col, self.row), id);
                            if self.provider.state(&self.block_by_id(id)) == TermState::Disabled {
                                let command = self.block_label(id);
                                self.pending_enable = Some(id);
                                self.status = Some(format!(
                                    "run `{command}`? y confirms · any other key cancels"
                                ));
                            } else {
                                self.focus = Some(id);
                            }
                        }
                        None => self.focus = None,
                    }
                }
                MouseAction::ScrollUp | MouseAction::ScrollDown => {
                    if let Some(&(id, r)) = self.term_rects.iter().find(|(_, r)| r.contains(pos)) {
                        self.provider.wheel(
                            id,
                            m.action == MouseAction::ScrollUp,
                            m.x - r.x,
                            m.y - r.y,
                        );
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
        if let Mode::Search { sel } = self.mode {
            self.draw_search(content, buf, sel);
        }
        self.draw_status(status, buf);
    }

    fn draw_slide(&mut self, area: Rect, buf: &mut Buffer) {
        let theme = self.current_theme();

        // One content box, two axes, same rules on both: margins are
        // minimum air (they always win), caps bound everything drawn,
        // and alignment only places what's left.
        let usable = Rect {
            x: area.x + theme.margin_x.min(area.width / 2),
            y: area.y + theme.margin_y.min(area.height / 2),
            width: area.width.saturating_sub(theme.margin_x.saturating_mul(2)),
            height: area.height.saturating_sub(theme.margin_y.saturating_mul(2)),
        };
        let w = usable.width.min(theme.max_width);
        if w < 10 || usable.height < 1 {
            return;
        }
        let x = usable.x
            + match theme.align_x {
                HAlign::Left => 0,
                HAlign::Center => (usable.width - w) / 2,
                HAlign::Right => usable.width - w,
            };

        // Pinned title: carve the leading heading off the first
        // segment and anchor it to the top of the usable area; the
        // body below still follows `align_y`.
        let mut title: Option<Text<'static>> = None;
        let mut first_md_body: Option<String> = None;
        if theme.pin_title
            && let Some(Segment::Markdown(src)) =
                self.deck.slide(self.col, self.row).segments.first()
            && let Some((head, rest)) = split_leading_heading(src)
        {
            title = Some(markdown::render(&head, w, &theme));
            first_md_body = Some(rest);
        }
        let title_h = title
            .as_ref()
            .map(|t| (t.height() as u16).min(usable.height))
            .unwrap_or(0);
        if let Some(t) = title {
            let rect = Rect {
                x,
                y: usable.y,
                width: w,
                height: title_h,
            };
            Paragraph::new(t).render(rect, buf);
        }
        // The body lays out below the title and its gap; the height
        // cap covers title, gap, and body together.
        let carved = if title_h > 0 {
            title_h + theme.title_gap
        } else {
            0
        };
        let body = Rect {
            y: usable.y + carved.min(usable.height),
            height: usable.height.saturating_sub(carved),
            ..usable
        };
        if body.height == 0 {
            return;
        }

        enum RenderItem {
            Text(Text<'static>),
            Term(TermBlock),
            /// Side-by-side cells, each a fixed-height stack.
            Row(Vec<Vec<(u16, CellItem)>>, u16),
        }
        let box_h = body
            .height
            .min(theme.max_height.saturating_sub(carved).max(1));
        let mut items: Vec<(Option<u16>, RenderItem)> = Vec::new();
        for (i, seg) in self
            .deck
            .slide(self.col, self.row)
            .segments
            .iter()
            .enumerate()
        {
            if i == 0
                && let Some(rest) = &first_md_body
            {
                let text = markdown::render(rest, w, &theme);
                let h = text.height() as u16;
                if h > 0 {
                    items.push((Some(h), RenderItem::Text(text)));
                }
                continue;
            }
            match seg {
                // Fill sizing only exists at the top level; everything
                // else routes through the shared cell builder.
                Segment::Terminal(b) if b.fill => {
                    items.push((None, RenderItem::Term(b.clone())));
                }
                Segment::Row(cells) => {
                    let n = cells.len().max(1) as u16;
                    let cw = (w.saturating_sub(ROW_GAP * (n - 1)) / n).max(10);
                    let built: Vec<Vec<(u16, CellItem)>> = cells
                        .iter()
                        .map(|cell| build_cell_items(cell, cw, box_h, &theme))
                        .collect();
                    let height = built
                        .iter()
                        .map(|cell| {
                            cell.iter()
                                .map(|(h, _)| h + 1)
                                .sum::<u16>()
                                .saturating_sub(1)
                        })
                        .max()
                        .unwrap_or(0);
                    if height > 0 {
                        items.push((Some(height), RenderItem::Row(built, cw)));
                    }
                }
                other => {
                    for (h, item) in build_cell_items(std::slice::from_ref(other), w, box_h, &theme)
                    {
                        let item = match item {
                            CellItem::Text(t) => RenderItem::Text(t),
                            CellItem::Term(b) => RenderItem::Term(b),
                        };
                        items.push((Some(h), item));
                    }
                }
            }
        }
        if items.is_empty() {
            return;
        }

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
        let mut y = body.y
            + match theme.align_y {
                VAlign::Top => 0,
                VAlign::Center => body.height.saturating_sub(visible) / 2,
                VAlign::Bottom => body.height.saturating_sub(visible),
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
                RenderItem::Row(cells, cw) => {
                    let mut cx = rect.x;
                    for cell in cells {
                        let mut cy = rect.y;
                        let cell_bottom = rect.y + rect.height;
                        for (ih, item) in cell {
                            if cy >= cell_bottom {
                                break;
                            }
                            let ih = ih.min(cell_bottom - cy);
                            let r = Rect {
                                x: cx,
                                y: cy,
                                width: cw,
                                height: ih,
                            };
                            match item {
                                CellItem::Text(text) => Paragraph::new(text).render(r, buf),
                                CellItem::Term(block) => {
                                    self.provider.prepare(
                                        &block,
                                        cw.saturating_sub(2).max(4),
                                        ih.saturating_sub(2).max(1),
                                    );
                                    self.term_rects.push((block.id, r));
                                    self.draw_term(r, &block, &theme, buf);
                                }
                            }
                            cy += ih + 1;
                        }
                        cx += cw + ROW_GAP;
                    }
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
            TermState::Disabled if self.pending_enable == Some(block.id) => {
                "run? y confirms · any other key cancels".to_string()
            }
            TermState::Disabled => "t to run".to_string(),
            TermState::Unavailable => "runs live in the real TUI".to_string(),
            TermState::Running { scroll_offset, .. } if *scroll_offset > 0 => {
                format!("history -{scroll_offset} · shift-pgdn returns")
            }
            TermState::Running { .. } if focused => {
                "Ctrl-q releases · shift-pgup history".to_string()
            }
            TermState::Running { .. } if is_next => "t interacts · s stops".to_string(),
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

        // One row under the grid is always reserved for the clip bar
        // (markers for slides scrolled out of view) so the grid never
        // reflows when scrolling starts.
        let grid = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: area.height.saturating_sub(2),
        };
        if grid.width < bw || grid.height < bh {
            return;
        }
        let vis_cols = (grid.width / bw).max(1) as usize;
        let vis_rows = (grid.height / bh).max(1) as usize;
        let n_cols = self.deck.columns.len();
        let max_depth = self
            .deck
            .columns
            .iter()
            .map(|c| c.slides.len())
            .max()
            .unwrap_or(0);
        let off_c = viewport_offset(sel.0, n_cols, vis_cols);
        let off_r = viewport_offset(sel.1, max_depth, vis_rows);

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

        // Clip bar: how many slides are scrolled out of view, per
        // direction, sitting right under the drawn rows. Depth counts
        // only consider visible columns — a column you can't see
        // shouldn't advertise its deep slides.
        let hidden_left = off_c;
        let hidden_right = n_cols.saturating_sub(off_c + vis_cols);
        let hidden_above = off_r;
        let vis_depth = self
            .deck
            .columns
            .iter()
            .skip(off_c)
            .take(vis_cols)
            .map(|c| c.slides.len())
            .max()
            .unwrap_or(0);
        let hidden_below = vis_depth.saturating_sub(off_r + vis_rows);
        if hidden_left + hidden_right + hidden_above + hidden_below == 0 {
            return;
        }
        let rows_drawn = vis_depth.saturating_sub(off_r).clamp(1, vis_rows) as u16;
        let bar = Rect {
            x: grid.x,
            y: grid.y + rows_drawn * bh,
            width: grid.width,
            height: 1,
        };
        let style = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        if hidden_left > 0 {
            Paragraph::new(Span::styled(format!("◂ {hidden_left}"), style)).render(bar, buf);
        }
        if hidden_right > 0 {
            Paragraph::new(Line::from(Span::styled(format!("{hidden_right} ▸"), style)))
                .alignment(ratatui::layout::Alignment::Right)
                .render(bar, buf);
        }
        if hidden_above + hidden_below > 0 {
            let mut mid = String::new();
            if hidden_above > 0 {
                mid.push_str(&format!("▴ {hidden_above}"));
            }
            if hidden_below > 0 {
                if !mid.is_empty() {
                    mid.push_str(" · ");
                }
                mid.push_str(&format!("▾ {hidden_below}"));
            }
            Paragraph::new(Line::from(Span::styled(mid, style)))
                .alignment(ratatui::layout::Alignment::Center)
                .render(bar, buf);
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
                row("t, enter", "run (after y) / focus the selected terminal"),
                row("click", "same as t, on the clicked terminal"),
                row("ctrl-q", "release terminal focus (or click outside)"),
                row("shift-pgup/dn", "scroll terminal history (or wheel)"),
                row("s / R", "stop / restart the selected terminal"),
                Line::default(),
            ]);
        }
        lines.extend([
            Line::from(Span::styled(" modes", dim)),
            row("o", "overview (type a slide's code to jump)"),
            row("/", "search slides (type to filter, ↵ jumps)"),
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

    fn draw_search(&self, area: Rect, buf: &mut Buffer, sel: usize) {
        let matches = self.search_matches();
        let sel = sel.min(matches.len().saturating_sub(1));
        let dim = Style::default().fg(self.theme.muted);
        let accent = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let hit = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);

        let summary = if matches.is_empty() {
            "  no matches · esc closes".to_string()
        } else {
            let n = matches.len();
            format!("  {n} match{} · ↵ jumps", if n == 1 { "" } else { "es" })
        };
        let mut lines = vec![
            Line::from(vec![
                Span::styled(format!(" /{}", self.search_input), accent),
                Span::styled("▏", Style::default().fg(self.theme.accent)),
                Span::styled(summary, dim),
            ]),
            Line::default(),
        ];

        let visible = (area.height.saturating_sub(4) as usize).min(12);
        let offset = viewport_offset(sel, matches.len(), visible.max(1));
        for (i, m) in matches.iter().enumerate().skip(offset).take(visible) {
            let marker = if i == sel { " ▸ " } else { "   " };
            let title_base = if i == sel {
                accent
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            let mut spans = vec![
                Span::styled(marker.to_string(), accent),
                Span::styled(format!("{}.{}  ", m.pos.0 + 1, m.pos.1 + 1), dim),
            ];
            spans.extend(split_hit(&m.title, m.title_hit, title_base, hit));
            if let Some((snippet, range)) = &m.snippet {
                spans.push(Span::styled(" — ".to_string(), dim));
                spans.extend(split_hit(snippet, Some(*range), dim, hit));
            }
            lines.push(Line::from(spans));
        }

        let w = 72.min(area.width);
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
                    .title(" search "),
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
    fn search_jumps_to_match() {
        let mut p = presenter();
        for key in ['/', 'b', '2'] {
            p.on_key(KeyPress::plain(Key::Char(key)));
        }
        p.on_key(KeyPress::plain(Key::Enter));
        assert_eq!(p.position(), (1, 1));
    }

    #[test]
    fn search_esc_cancels_and_restores_slide_mode() {
        let mut p = presenter();
        for key in ['/', 'z'] {
            p.on_key(KeyPress::plain(Key::Char(key)));
        }
        p.on_key(KeyPress::plain(Key::Esc));
        assert_eq!(p.position(), (0, 0));
        // Back in slide mode: space navigates again.
        p.on_key(KeyPress::plain(Key::Char(' ')));
        assert_eq!(p.position(), (1, 0));
    }

    #[test]
    fn empty_search_lists_all_slides_as_picker() {
        let mut p = presenter();
        p.on_key(KeyPress::plain(Key::Char('/')));
        p.on_key(KeyPress::plain(Key::Down));
        p.on_key(KeyPress::plain(Key::Down));
        p.on_key(KeyPress::plain(Key::Enter));
        // Third entry in traversal order: (0,0), (1,0), (1,1), …
        assert_eq!(p.position(), (1, 1));
    }

    #[test]
    fn find_ci_matches_case_insensitively_on_byte_ranges() {
        assert_eq!(find_ci("Hello World", "world"), Some((6, 11)));
        assert_eq!(find_ci("Hello", "xyz"), None);
        // Multibyte haystack before the hit keeps ranges on boundaries.
        assert_eq!(find_ci("héllo wörld", "WÖR"), Some((7, 11)));
        assert_eq!(find_ci("abc", ""), Some((0, 0)));
    }

    #[test]
    fn clip_snippet_keeps_deep_hits_visible() {
        let line = format!("{}needle after", "x".repeat(60));
        let (clipped, (s, e)) = clip_snippet(&line, (60, 66));
        assert!(clipped.starts_with('…'));
        assert_eq!(&clipped[s..e], "needle");
    }

    #[test]
    fn viewport_offset_keeps_a_margin() {
        // Fits: never scrolls.
        assert_eq!(viewport_offset(3, 4, 4), 0);
        // Selection stays one slot in from the trailing edge…
        assert_eq!(viewport_offset(3, 10, 4), 1);
        assert_eq!(viewport_offset(5, 10, 4), 3);
        // …except at the very ends, where there's nothing beyond.
        assert_eq!(viewport_offset(0, 10, 4), 0);
        assert_eq!(viewport_offset(9, 10, 4), 6);
        // Degenerate one-slot viewport still shows the selection.
        assert_eq!(viewport_offset(7, 10, 1), 7);
    }

    #[test]
    fn overview_clip_bar_counts_hidden_slides() {
        // 8 columns; the first is 3 deep. 60x14 fits 2 columns × 2 rows
        // of boxes (26x4 each) plus header and clip bar.
        let mut src = String::from("# c1\n--\n# c1b\n--\n# c1c\n");
        for i in 2..=8 {
            src.push_str(&format!("---\n# c{i}\n"));
        }
        let deck = crate::deck::parse(&src, "t").unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        p.on_key(KeyPress::plain(Key::Char('o')));
        let area = Rect::new(0, 0, 60, 14);
        let screen = |p: &mut Presenter<NullProvider>| {
            let mut buf = Buffer::empty(area);
            p.draw(area, &mut buf);
            (0..area.height)
                .map(|y| {
                    (0..area.width)
                        .map(|x| buf[(x, y)].symbol().to_string())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n")
        };

        // At the start: 6 columns hidden right, 1 slide hidden below.
        let s = screen(&mut p);
        assert!(s.contains("6 ▸"), "screen:\n{s}");
        assert!(s.contains("▾ 1"), "screen:\n{s}");
        assert!(!s.contains('◂'), "screen:\n{s}");

        // At the last column: everything hidden is to the left, and the
        // shallow visible columns advertise no hidden depth.
        p.on_key(KeyPress::plain(Key::Esc));
        p.on_key(KeyPress::plain(Key::Char('G')));
        p.on_key(KeyPress::plain(Key::Char('o')));
        let s = screen(&mut p);
        assert!(s.contains("◂ 6"), "screen:\n{s}");
        assert!(!s.contains('▸'), "screen:\n{s}");
        assert!(!s.contains('▾'), "screen:\n{s}");
    }

    #[test]
    fn overview_clip_bar_absent_when_everything_fits() {
        let deck = crate::deck::parse("# a\n---\n# b\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        p.on_key(KeyPress::plain(Key::Char('o')));
        let area = Rect::new(0, 0, 60, 14);
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        let mut screen = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                screen.push_str(buf[(x, y)].symbol());
            }
        }
        for glyph in ["◂", "▸", "▴", "▾"] {
            assert!(!screen.contains(glyph), "unexpected {glyph}");
        }
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
    fn qr_slide_draws_black_on_white() {
        let deck = crate::deck::parse("```qr\nhttps://deckhand.sh\n```\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        let area = Rect::new(0, 0, 80, 40);
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        let cells = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .map(|pos| &buf[pos]);
        assert!(
            cells
                .clone()
                .any(|c| c.bg == Color::Indexed(231) && c.fg == Color::Indexed(16)),
            "no black-on-white QR cells drawn"
        );
        assert!(cells.clone().any(|c| c.symbol().contains('█')));
    }

    #[test]
    fn qr_colors_follow_slide_theme() {
        let deck = crate::deck::parse(
            "```theme\nqr_dark: red\nqr_light: white\n```\n```qr\nhi\n```\n",
            "t",
        )
        .unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        let themed = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .any(|pos| buf[pos].fg == Color::Red && buf[pos].bg == Color::White);
        assert!(themed, "qr_dark/qr_light overrides not applied");
    }

    #[test]
    fn pinned_title_sticks_while_body_centers() {
        let deck =
            crate::deck::parse("```theme\npin_title: true\n```\n# TITLE\n\nbody\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        let area = Rect::new(0, 0, 40, 20);
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        let row = |y: u16| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        // Heading anchored to the very top…
        assert!(row(0).contains("TITLE"), "row 0: {:?}", row(0));
        // …while the body centers in the space below (19 content rows,
        // title+underline+gap = 3, one body row → y = 3 + 15/2 = 10).
        let body_y = (0..19).find(|&y| row(y).contains("body")).unwrap();
        assert!(
            (8..=12).contains(&body_y),
            "body at y={body_y}, expected centered"
        );

        // Without pin_title the title floats down with the body.
        let deck = crate::deck::parse("# TITLE\n\nbody\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        let row = |y: u16| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        let title_y = (0..19).find(|&y| row(y).contains("TITLE")).unwrap();
        assert!(
            title_y > 3,
            "unpinned title at y={title_y}, expected floating"
        );
    }

    #[test]
    fn box_model_composes_margins_caps_and_pin() {
        // pin_title + margin_y + title_gap + max_height, all at once:
        // margins inset the title, the gap spaces the body, and the
        // cap bounds title+gap+body together.
        let deck = crate::deck::parse(
            "```theme\npin_title: true\nmargin_y: 2\ntitle_gap: 2\nmax_height: 9\nalign_y: top\n```\n# TITLE\n\na\n\nb\n\nc\n\nd\n\ne\n",
            "t",
        )
        .unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        let area = Rect::new(0, 0, 40, 24);
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        let row = |y: u16| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        // Title at margin_y, not row 0.
        assert!(!row(0).contains("TITLE"));
        assert!(row(2).contains("TITLE"), "row 2: {:?}", row(2));
        // title_gap: 2 → body starts at y = 2 (margin) + 2 (title) + 2.
        assert!(row(6).contains('a'), "row 6: {:?}", row(6));
        // max_height 9 bounds title(2)+gap(2)+body → body ≤ 5 rows:
        // "a", blank, "b", blank, "c" fit; "d" (row 12) does not.
        assert!(row(10).contains('c'), "row 10: {:?}", row(10));
        assert!(
            !(0..24).any(|y| row(y).contains('d')),
            "cap should clip past max_height"
        );
    }

    #[test]
    fn align_x_and_bottom_place_the_box() {
        let deck = crate::deck::parse(
            "```theme\nalign_x: right\nalign_y: bottom\nmargin: 0\nmax_width: 20\n```\nhi\n",
            "t",
        )
        .unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        let row = |y: u16| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        // Content box is 20 wide, right-aligned in 60 → "hi" at x=40;
        // one row tall, bottom-aligned in 9 content rows → y=8.
        assert!(row(8).contains("hi"), "row 8: {:?}", row(8));
        assert!(row(8).find("hi").unwrap() >= 40);
        assert!(!(0..8).any(|y| row(y).contains("hi")));
    }

    #[test]
    fn row_draws_cells_side_by_side() {
        let deck =
            crate::deck::parse("````row\n```qr\nhi\n```\n||\n```qr\nhi\n```\n````\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        let area = Rect::new(0, 0, 80, 30);
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        // Two QR codes on the same line: white-bg cells in both the
        // left and right halves of some row.
        let qr_cols: Vec<u16> = (0..area.width)
            .filter(|&x| (0..area.height).any(|y| buf[(x, y)].bg == Color::Indexed(231)))
            .collect();
        assert!(!qr_cols.is_empty(), "no QR cells drawn");
        assert!(
            qr_cols.iter().any(|&x| x < 40) && qr_cols.iter().any(|&x| x >= 40),
            "QR cells all on one side: {qr_cols:?}"
        );
    }

    #[test]
    fn row_terminals_are_focusable_and_hit_testable() {
        let deck = crate::deck::parse(
            "````row\n```terminal\n```\n||\n```terminal\n```\n````\n",
            "t",
        )
        .unwrap();
        let mut p = Presenter::new(deck, vec![], FakeTerms { resets: 0 }).unwrap();
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        assert_eq!(p.term_rects.len(), 2);
        let (left, right) = (p.term_rects[0].1, p.term_rects[1].1);
        assert_eq!(left.y, right.y, "cells should share a row");
        assert!(left.x + left.width <= right.x, "cells should not overlap");
        // Focus cycles across both cells.
        p.on_key(KeyPress::plain(Key::Char('t')));
        p.on_key(KeyPress::plain(Key::Char('y')));
        assert_eq!(p.focus, Some(0));
    }

    #[test]
    fn image_slide_draws_ascii_and_placeholder() {
        let mut deck = crate::deck::parse("![pic](p.png)\n", "t").unwrap();
        // Placeholder first (no pixels loaded)…
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        let area = Rect::new(0, 0, 60, 20);
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        let row = |buf: &Buffer, y: u16| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        let screen = (0..area.height)
            .map(|y| row(&buf, y))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains("image: pic"), "screen:\n{screen}");

        // …then real pixels render as a colored ramp.
        deck = crate::deck::parse("![pic](p.png)\n", "t").unwrap();
        if let crate::deck::Segment::Image(img) = &mut deck.columns[0].slides[0].segments[0] {
            img.data = Some(crate::ascii_image::ImageData {
                width: 8,
                height: 8,
                rgba: [200u8, 200, 200, 255].repeat(64),
            });
        }
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        let mut buf = Buffer::empty(area);
        p.draw(area, &mut buf);
        let screen = (0..area.height)
            .map(|y| row(&buf, y))
            .collect::<Vec<_>>()
            .join("\n");
        // Luminance 200/255 lands on '#' in the ramp.
        assert!(screen.contains('#'), "no ramp glyphs drawn:\n{screen}");
    }

    #[test]
    fn non_interactive_context_refuses_focus() {
        let deck = crate::deck::parse("```terminal\n```\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], NullProvider).unwrap();
        p.on_key(KeyPress::plain(Key::Char('t')));
        assert!(p.focus.is_none());
    }

    /// A provider that runs "terminals" only enough to test focus and
    /// reset behavior.
    struct FakeTerms {
        resets: usize,
    }
    impl TerminalProvider for FakeTerms {
        fn prepare(&mut self, _: &TermBlock, _: u16, _: u16) {}
        fn state(&self, _: &TermBlock) -> TermState {
            TermState::Running {
                scroll_offset: 0,
                scroll_total: 0,
            }
        }
        fn draw(&mut self, _: &TermBlock, _: Rect, _: &mut Buffer) {}
        fn input(&mut self, _: usize, _: &[u8]) {}
        fn scroll(&mut self, _: usize, _: isize) {}
        fn restart(&mut self, _: &[usize]) {}
        fn reset(&mut self) {
            self.resets += 1;
        }
    }

    #[test]
    fn replace_deck_clamps_position_and_keeps_place() {
        let mut p = presenter();
        p.on_key(KeyPress::plain(Key::Char('G'))); // column 3
        assert_eq!(p.position(), (2, 0));

        // Same shape: position survives.
        let same = crate::deck::parse("# a2\n---\n# b\n--\n# b2\n---\n# c\n", "t").unwrap();
        p.replace_deck(same, vec![]).unwrap();
        assert_eq!(p.position(), (2, 0));

        // Deck shrank to one column: position clamps.
        let smaller = crate::deck::parse("# only\n", "t").unwrap();
        p.replace_deck(smaller, vec![]).unwrap();
        assert_eq!(p.position(), (0, 0));
    }

    /// A provider with per-block consent, like the native PtyProvider.
    #[derive(Default)]
    struct GatedTerms {
        approved: std::collections::HashSet<usize>,
        stopped: Vec<usize>,
    }
    impl TerminalProvider for GatedTerms {
        fn prepare(&mut self, _: &TermBlock, _: u16, _: u16) {}
        fn state(&self, block: &TermBlock) -> TermState {
            if self.approved.contains(&block.id) {
                TermState::Running {
                    scroll_offset: 0,
                    scroll_total: 0,
                }
            } else {
                TermState::Disabled
            }
        }
        fn draw(&mut self, _: &TermBlock, _: Rect, _: &mut Buffer) {}
        fn input(&mut self, _: usize, _: &[u8]) {}
        fn scroll(&mut self, _: usize, _: isize) {}
        fn restart(&mut self, _: &[usize]) {}
        fn enable(&mut self, id: usize) {
            self.approved.insert(id);
        }
        fn stop(&mut self, id: usize) {
            self.approved.remove(&id);
            self.stopped.push(id);
        }
    }

    #[test]
    fn disabled_terminals_need_in_deck_confirmation() {
        let deck = crate::deck::parse("```terminal\nhtop\n```\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], GatedTerms::default()).unwrap();

        // t arms the confirmation but runs nothing.
        p.on_key(KeyPress::plain(Key::Char('t')));
        assert_eq!(p.focus, None);
        assert!(p.provider.approved.is_empty());

        // y approves and focuses.
        p.on_key(KeyPress::plain(Key::Char('y')));
        assert!(p.provider.approved.contains(&0));
        assert_eq!(p.focus, Some(0));
    }

    #[test]
    fn any_other_key_cancels_the_confirmation() {
        let deck = crate::deck::parse("```terminal\nhtop\n```\n---\n# b\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], GatedTerms::default()).unwrap();

        p.on_key(KeyPress::plain(Key::Char('t')));
        // Space cancels — and still navigates.
        p.on_key(KeyPress::plain(Key::Char(' ')));
        assert!(p.provider.approved.is_empty());
        assert_eq!(p.position(), (1, 0));
        // A later y is just an inert key, not an approval.
        p.on_key(KeyPress::plain(Key::Char('y')));
        assert!(p.provider.approved.is_empty());
    }

    #[test]
    fn stop_withdraws_approval() {
        let deck = crate::deck::parse("```terminal\nhtop\n```\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], GatedTerms::default()).unwrap();
        p.on_key(KeyPress::plain(Key::Char('t')));
        p.on_key(KeyPress::plain(Key::Char('y')));
        assert_eq!(p.focus, Some(0));

        p.on_key(KeyPress {
            ctrl: true,
            ..KeyPress::plain(Key::Char('q'))
        });
        p.on_key(KeyPress::plain(Key::Char('s')));
        assert_eq!(p.provider.stopped, vec![0]);
        assert_eq!(p.focus, None);
        // Back to Disabled: running again requires re-confirmation.
        p.on_key(KeyPress::plain(Key::Char('t')));
        assert_eq!(p.focus, None);
    }

    #[test]
    fn replace_deck_resets_terminals_only_when_they_change() {
        let deck = crate::deck::parse("```terminal\nhtop\n```\n", "t").unwrap();
        let mut p = Presenter::new(deck, vec![], FakeTerms { resets: 0 }).unwrap();
        p.on_key(KeyPress::plain(Key::Char('t')));
        assert_eq!(p.focus, Some(0));

        // Markdown-only edit: terminals identical → focus and sessions live on.
        let same_terms = crate::deck::parse("new words\n```terminal\nhtop\n```\n", "t").unwrap();
        p.replace_deck(same_terms, vec![]).unwrap();
        assert_eq!(p.provider.resets, 0);
        assert_eq!(p.focus, Some(0));

        // Command changed: provider resets, focus drops.
        let changed = crate::deck::parse("```terminal\nbtop\n```\n", "t").unwrap();
        p.replace_deck(changed, vec![]).unwrap();
        assert_eq!(p.provider.resets, 1);
        assert_eq!(p.focus, None);
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
