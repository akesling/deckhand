//! wasm-bindgen front end for the website's in-browser presenter.
//!
//! Wraps the shared [`crate::presenter::Presenter`] with a
//! [`SnapshotProvider`] (browsers can't spawn PTYs), translates DOM
//! key names, and serializes each frame to ANSI for xterm.js.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::anyhow;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier};
use unicode_width::UnicodeWidthStr;
use wasm_bindgen::prelude::*;

use crate::deck;
use crate::presenter::{Key, KeyPress, Mouse, MouseAction, Presenter};
use crate::replay::SnapshotProvider;
use crate::{compile, config};

fn err(e: anyhow::Error) -> JsError {
    JsError::new(&format!("{e:#}"))
}

/// A deck being presented in the browser: the JS-facing handle around
/// the shared [`Presenter`]. Feed it DOM key events via [`WebDeck::key`]
/// and write [`WebDeck::render`]'s ANSI output into xterm.js.
#[wasm_bindgen]
pub struct WebDeck {
    presenter: Presenter<SnapshotProvider>,
    cols: u16,
    rows: u16,
}

#[wasm_bindgen]
impl WebDeck {
    /// Load a single-file markdown deck (frontmatter honored).
    #[wasm_bindgen(constructor)]
    pub fn new(
        source: &str,
        fallback_title: &str,
        cols: u16,
        rows: u16,
    ) -> Result<WebDeck, JsError> {
        let (deck, theme) = deck::parse_full(source, fallback_title).map_err(err)?;
        let presenter = Presenter::new(
            deck,
            theme.into_iter().collect(),
            SnapshotProvider::default(),
        )
        .map_err(err)?;
        Ok(WebDeck {
            presenter,
            cols: cols.max(20),
            rows: rows.max(4),
        })
    }

    /// Load a deck from a file bundle (e.g. a fetched gist): `entry` names
    /// the deck file inside `files_json`, a JSON object of path → content.
    /// A `.json` entry is a manifest whose paths resolve in the bundle.
    pub fn from_bundle(
        entry: &str,
        files_json: &str,
        cols: u16,
        rows: u16,
    ) -> Result<WebDeck, JsError> {
        let files: HashMap<String, String> =
            serde_json::from_str(files_json).map_err(|e| JsError::new(&e.to_string()))?;
        let entry_src = files
            .get(entry)
            .ok_or_else(|| JsError::new(&format!("bundle has no file named {entry:?}")))?
            .clone();
        let (deck, theme) = if entry.ends_with(".json") {
            let reader = |p: &Path| {
                let key = p.to_string_lossy();
                files
                    .get(key.as_ref())
                    .cloned()
                    .ok_or_else(|| anyhow!("bundle has no file named {key:?}"))
            };
            config::parse_manifest(&entry_src, entry, &reader).map_err(err)?
        } else {
            let fallback = entry.trim_end_matches(".md");
            deck::parse_full(&entry_src, fallback).map_err(err)?
        };
        let presenter = Presenter::new(
            deck,
            theme.into_iter().collect(),
            SnapshotProvider::default(),
        )
        .map_err(err)?;
        Ok(WebDeck {
            presenter,
            cols: cols.max(20),
            rows: rows.max(4),
        })
    }

    /// Match the render size to the hosting xterm.js grid.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols.max(20);
        self.rows = rows.max(4);
    }

    /// Handle a DOM `KeyboardEvent`: its `.key` plus modifier flags.
    pub fn key(&mut self, key: &str, ctrl: bool, alt: bool, shift: bool) {
        if let Some(key) = dom_key(key) {
            self.presenter.on_key(KeyPress {
                key,
                ctrl,
                alt,
                shift,
            });
        }
    }

    /// Route a wheel tick at cell (x, y). True when the deck consumed
    /// it (an embedded terminal is under the cursor) — false means the
    /// page should keep the scroll.
    pub fn wheel(&mut self, x: u16, y: u16, up: bool) -> bool {
        if !self.presenter.terminal_at(x, y) {
            return false;
        }
        self.presenter.on_mouse(Mouse {
            x,
            y,
            action: if up {
                MouseAction::ScrollUp
            } else {
                MouseAction::ScrollDown
            },
        });
        true
    }

    /// Whether an embedded terminal sits at cell (x, y) — the wheel
    /// handler asks per-event to decide deck-vs-page ownership.
    pub fn terminal_at(&self, x: u16, y: u16) -> bool {
        self.presenter.terminal_at(x, y)
    }

    /// Render the current frame as ANSI escape sequences for xterm.js.
    pub fn render(&mut self) -> String {
        let area = Rect::new(0, 0, self.cols, self.rows);
        let mut buf = Buffer::empty(area);
        self.presenter.draw(area, &mut buf);
        buffer_to_ansi(&buf)
    }

    /// Jump to a slide (coordinates clamp) — lets the playground keep
    /// your place while the deck is rebuilt around you.
    pub fn goto(&mut self, col: usize, row: usize) {
        self.presenter.goto(col, row);
    }

    /// Current column (0-based).
    pub fn col(&self) -> usize {
        self.presenter.position().0
    }

    /// Current depth within the column (0-based).
    pub fn row(&self) -> usize {
        self.presenter.position().1
    }

    /// Title of the slide being shown.
    pub fn slide_title(&self) -> String {
        self.presenter.current_slide().title.clone()
    }

    /// Presenter notes of the slide being shown (may be empty).
    pub fn notes(&self) -> String {
        self.presenter.current_slide().notes.clone()
    }

    /// Human-readable position, e.g. `"2.1 · 3/9"`.
    pub fn position(&self) -> String {
        let deck = self.presenter.deck();
        let (col, row) = self.presenter.position();
        let flat = deck.flat();
        let idx = flat.iter().position(|&p| p == (col, row)).unwrap_or(0);
        format!("{}.{} · {}/{}", col + 1, row + 1, idx + 1, flat.len())
    }

    /// Flatten the loaded deck to single-file markdown (for "download").
    pub fn to_markdown(&self) -> Result<String, JsError> {
        let (body, _) = compile::compile(self.presenter.deck()).map_err(err)?;
        Ok(body)
    }
}

/// Flatten a fetched bundle into single-file markdown, preserving the
/// deck title and theme as frontmatter — used when loading a
/// manifest-based gist into the playground editor.
#[wasm_bindgen]
pub fn bundle_to_markdown(entry: &str, files_json: &str) -> Result<String, JsError> {
    let files: HashMap<String, String> =
        serde_json::from_str(files_json).map_err(|e| JsError::new(&e.to_string()))?;
    let entry_src = files
        .get(entry)
        .ok_or_else(|| JsError::new(&format!("bundle has no file named {entry:?}")))?;
    if !entry.ends_with(".json") {
        return Ok(entry_src.clone());
    }
    let reader = |p: &Path| {
        let key = p.to_string_lossy();
        files
            .get(key.as_ref())
            .cloned()
            .ok_or_else(|| anyhow!("bundle has no file named {key:?}"))
    };
    let (deck, theme) = config::parse_manifest(entry_src, entry, &reader).map_err(err)?;
    let (body, _) = compile::compile(&deck).map_err(err)?;
    let front = deck::FrontMatter {
        title: Some(deck.title.clone()),
        theme,
    };
    let mut md = String::from("---\n");
    md.push_str(&serde_yaml::to_string(&front).map_err(|e| JsError::new(&e.to_string()))?);
    md.push_str("---\n\n");
    md.push_str(&body);
    Ok(md)
}

fn dom_key(key: &str) -> Option<Key> {
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(Key::Char(c));
    }
    Some(match key {
        "Enter" => Key::Enter,
        "Escape" => Key::Esc,
        "Backspace" => Key::Backspace,
        "Tab" => Key::Tab,
        "ArrowUp" => Key::Up,
        "ArrowDown" => Key::Down,
        "ArrowLeft" => Key::Left,
        "ArrowRight" => Key::Right,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Insert" => Key::Insert,
        "Delete" => Key::Delete,
        _ => return None,
    })
}

// ------------------------------------------------------------------- ansi

/// Serialize a full frame: home the cursor and overwrite every cell,
/// emitting SGR codes only when the style changes.
fn buffer_to_ansi(buf: &Buffer) -> String {
    let area = buf.area;
    let mut out = String::with_capacity((area.width as usize + 8) * area.height as usize * 2);
    out.push_str("\x1b[H\x1b[0m");
    let mut cur_fg = Color::Reset;
    let mut cur_bg = Color::Reset;
    let mut cur_mod = Modifier::empty();
    for y in 0..area.height {
        if y > 0 {
            out.push_str("\x1b[0m\r\n");
            cur_fg = Color::Reset;
            cur_bg = Color::Reset;
            cur_mod = Modifier::empty();
        }
        let mut x = 0;
        while x < area.width {
            let cell = &buf[(x, y)];
            if cell.fg != cur_fg || cell.bg != cur_bg || cell.modifier != cur_mod {
                out.push_str("\x1b[0m");
                push_color(&mut out, cell.fg, false);
                push_color(&mut out, cell.bg, true);
                push_modifiers(&mut out, cell.modifier);
                cur_fg = cell.fg;
                cur_bg = cell.bg;
                cur_mod = cell.modifier;
            }
            let symbol = cell.symbol();
            out.push_str(symbol);
            // Wide glyphs cover the following buffer cell(s); skip them.
            x += (symbol.width().max(1)) as u16;
        }
    }
    out.push_str("\x1b[0m");
    out
}

fn push_color(out: &mut String, color: Color, bg: bool) {
    let offset = if bg { 10 } else { 0 };
    let simple = |out: &mut String, n: u16| {
        let _ = write!(out, "\x1b[{}m", n + offset);
    };
    match color {
        Color::Reset => {}
        Color::Black => simple(out, 30),
        Color::Red => simple(out, 31),
        Color::Green => simple(out, 32),
        Color::Yellow => simple(out, 33),
        Color::Blue => simple(out, 34),
        Color::Magenta => simple(out, 35),
        Color::Cyan => simple(out, 36),
        Color::Gray => simple(out, 37),
        Color::DarkGray => simple(out, 90),
        Color::LightRed => simple(out, 91),
        Color::LightGreen => simple(out, 92),
        Color::LightYellow => simple(out, 93),
        Color::LightBlue => simple(out, 94),
        Color::LightMagenta => simple(out, 95),
        Color::LightCyan => simple(out, 96),
        Color::White => simple(out, 97),
        Color::Indexed(n) => {
            let _ = write!(out, "\x1b[{};5;{n}m", if bg { 48 } else { 38 });
        }
        Color::Rgb(r, g, b) => {
            let _ = write!(out, "\x1b[{};2;{r};{g};{b}m", if bg { 48 } else { 38 });
        }
    }
}

fn push_modifiers(out: &mut String, m: Modifier) {
    for (flag, code) in [
        (Modifier::BOLD, 1),
        (Modifier::DIM, 2),
        (Modifier::ITALIC, 3),
        (Modifier::UNDERLINED, 4),
        (Modifier::SLOW_BLINK, 5),
        (Modifier::REVERSED, 7),
        (Modifier::HIDDEN, 8),
        (Modifier::CROSSED_OUT, 9),
    ] {
        if m.contains(flag) {
            let _ = write!(out, "\x1b[{code}m");
        }
    }
}
