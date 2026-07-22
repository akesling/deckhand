//! Deck parsing.
//!
//! A deck is a single markdown file with two axes:
//!   `---` on its own line starts a new column (earlier/later)
//!   `--`  on its own line starts a deeper slide within the current column
//!
//! Within a slide:
//!   a line containing only `???` splits content from presenter notes
//!   a fenced code block whose info string starts with `terminal` becomes an
//!   embedded interactive PTY, e.g.:
//!
//! ````text
//! ```terminal rows=12
//! python3 -q
//! ```
//! ````
//!
//!   An empty body spawns an interactive shell ($SHELL).

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::theme::ThemeConfig;

/// A parsed presentation: columns left-to-right, slides top-to-bottom
/// within each column.
#[derive(Debug)]
pub struct Deck {
    /// Shown in the status bar and the notes client.
    pub title: String,
    /// The horizontal axis (earlier / later).
    pub columns: Vec<Column>,
}

/// One column of the 2-D grid; index 0 is the shallow slide, later
/// entries are deeper detail.
#[derive(Debug)]
pub struct Column {
    /// The vertical axis (shallower / deeper).
    pub slides: Vec<Slide>,
}

/// A single slide: rendered segments plus presentation metadata.
#[derive(Debug)]
pub struct Slide {
    /// From the first heading, first text line, or "col.row".
    pub title: String,
    /// Markdown and terminal blocks, in slide order.
    pub segments: Vec<Segment>,
    /// Presenter notes (the part after a `???` line); empty if none.
    pub notes: String,
    /// Per-slide theme overrides, from a manifest slide's `theme` field
    /// and/or a `theme` fence in the slide's markdown; layered over the
    /// deck/user theme when this slide is displayed.
    pub theme: Option<crate::theme::ThemeConfig>,
}

/// One vertically-stacked piece of a slide.
#[derive(Debug)]
pub enum Segment {
    /// Markdown source, rendered by [`crate::markdown`].
    Markdown(String),
    /// An embedded terminal.
    Terminal(TermBlock),
}

/// An embedded terminal block, as authored in the deck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermBlock {
    /// Globally unique across the deck; keys the live PTY session.
    pub id: usize,
    /// Script run via `$SHELL -c`; `None` spawns an interactive shell.
    pub command: Option<String>,
    /// Inner height of the terminal viewport (ignored when `fill` is set).
    pub rows: u16,
    /// Expand to all remaining slide height instead of a fixed row count.
    pub fill: bool,
    /// A captured "screenshot" of the running command, baked in by
    /// `deckhand compile --snapshots`. Contexts that can't run PTYs (the
    /// web presenter) replay it; native presenting ignores it.
    pub snapshot: Option<TermSnapshot>,
}

/// A terminal screen capture: raw ANSI bytes (vt100
/// `contents_formatted`) that replay into any terminal emulator, plus
/// the geometry they were captured at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermSnapshot {
    /// Width the capture was taken at.
    pub cols: u16,
    /// Height the capture was taken at.
    pub rows: u16,
    /// Raw ANSI bytes; feed to a terminal emulator to reproduce.
    pub data: Vec<u8>,
}

impl Deck {
    /// The slide at (column, depth). Panics if out of range; callers
    /// navigate via clamped coordinates.
    pub fn slide(&self, col: usize, row: usize) -> &Slide {
        &self.columns[col].slides[row]
    }

    /// First-line labels of every terminal command in the deck, in
    /// order (`"shell"` for blocks with no command). Used for consent
    /// prompts before anything executes.
    pub fn terminal_commands(&self) -> Vec<String> {
        self.columns
            .iter()
            .flat_map(|c| &c.slides)
            .flat_map(|s| &s.segments)
            .filter_map(|seg| match seg {
                Segment::Terminal(b) => Some(
                    b.command
                        .as_deref()
                        .and_then(|c| c.lines().next())
                        .unwrap_or("shell")
                        .to_string(),
                ),
                _ => None,
            })
            .collect()
    }

    /// Depth-first traversal order: all slides of column 0 top-to-bottom,
    /// then column 1, etc. This is the order `space` walks through.
    pub fn flat(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for (c, col) in self.columns.iter().enumerate() {
            for r in 0..col.slides.len() {
                out.push((c, r));
            }
        }
        out
    }
}

impl Slide {
    /// Ids of this slide's terminal blocks, in slide order.
    pub fn term_ids(&self) -> Vec<usize> {
        self.segments
            .iter()
            .filter_map(|s| match s {
                Segment::Terminal(b) => Some(b.id),
                _ => None,
            })
            .collect()
    }
}

/// Load a deck plus its embedded theme config (from a JSON manifest's
/// `theme` field, or a markdown deck's frontmatter).
pub fn load(path: &Path) -> Result<(Deck, Option<ThemeConfig>)> {
    if path.extension().and_then(|e| e.to_str()) == Some("json") {
        return crate::config::load(path);
    }
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("reading deck file {}", path.display()))?;
    let fallback = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "deck".to_string());
    let (deck, theme) = parse_full(&source, &fallback)?;
    if deck.columns.is_empty() {
        anyhow::bail!("deck {} contains no slides", path.display());
    }
    Ok((deck, theme))
}

/// YAML frontmatter for single-file markdown decks:
///
/// ```markdown
/// ---
/// title: my talk
/// theme:
///   border_type: rounded
///   accent: magenta
/// ---
///
/// # first slide
/// ```
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrontMatter {
    /// Deck title override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Deck-level theme overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeConfig>,
}

/// Parse a markdown deck, honoring optional frontmatter. A leading
/// `---` block is only treated as frontmatter when it parses as a YAML
/// mapping — otherwise it stays deck content (`---` also separates
/// columns, and an empty leading column was always legal).
pub fn parse_full(source: &str, fallback_title: &str) -> Result<(Deck, Option<ThemeConfig>)> {
    let mut theme = None;
    let mut title = None;
    let body = match split_frontmatter(source) {
        Some((raw, rest)) => match serde_yaml::from_str::<serde_yaml::Value>(&raw) {
            Ok(serde_yaml::Value::Mapping(_)) => {
                let fm: FrontMatter = serde_yaml::from_str(&raw)
                    .context("parsing deck frontmatter (supported keys: title, theme)")?;
                theme = fm.theme;
                title = fm.title;
                rest
            }
            _ => source.to_string(),
        },
        None => source.to_string(),
    };
    let mut deck = parse(&body, fallback_title)?;
    if let Some(t) = title {
        deck.title = t;
    }
    Ok((deck, theme))
}

/// If the source opens with a `---` line and a later `---` line closes
/// it, return (frontmatter text, remainder).
fn split_frontmatter(source: &str) -> Option<(String, String)> {
    let mut iter = source.split_inclusive('\n');
    let first = iter.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let mut fm = String::new();
    let mut consumed = first.len();
    for line in iter {
        consumed += line.len();
        if line.trim_end() == "---" {
            return Some((fm, source[consumed..].to_string()));
        }
        fm.push_str(line);
    }
    None
}

/// Parse single-file markdown deck *content* (no frontmatter handling —
/// use [`parse_full`] for that). `title` is the fallback deck title.
///
/// ```
/// use deckhand::deck;
///
/// let deck = deck::parse("# intro\n---\n# demo\n--\n## details\n", "talk")?;
/// assert_eq!(deck.columns.len(), 2);
/// assert_eq!(deck.columns[1].slides.len(), 2);
/// assert_eq!(deck.slide(1, 1).title, "details");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn parse(source: &str, title: &str) -> Result<Deck> {
    let mut next_term_id = 0usize;
    let mut columns = Vec::new();
    for (c, sources) in split_source(source).into_iter().enumerate() {
        let mut slides = Vec::new();
        for (r, src) in sources.into_iter().enumerate() {
            slides.push(parse_slide(&src, c, r, &mut next_term_id)?);
        }
        columns.push(Column { slides });
    }
    Ok(Deck {
        title: title.to_string(),
        columns,
    })
}

/// Fence state: `Some((char, len))` while inside a fenced code block.
pub(crate) type Fence = Option<(char, usize)>;

pub(crate) fn opens_fence(trimmed: &str) -> Fence {
    let ch = trimmed.chars().next()?;
    if ch != '`' && ch != '~' {
        return None;
    }
    let n = trimmed.chars().take_while(|c| *c == ch).count();
    if n >= 3 { Some((ch, n)) } else { None }
}

pub(crate) fn closes_fence(trimmed: &str, ch: char, len: usize) -> bool {
    !trimmed.is_empty() && trimmed.chars().all(|c| c == ch) && trimmed.chars().count() >= len
}

/// Split the raw file into columns of slide sources, respecting code fences
/// so a `---` inside a code block never splits a slide.
fn split_source(source: &str) -> Vec<Vec<String>> {
    let mut columns: Vec<Vec<String>> = Vec::new();
    let mut column: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut fence: Fence = None;

    for line in source.lines() {
        let t = line.trim();
        if let Some((ch, n)) = fence {
            cur.push_str(line);
            cur.push('\n');
            if closes_fence(t, ch, n) {
                fence = None;
            }
            continue;
        }
        if let Some(f) = opens_fence(t) {
            fence = Some(f);
            cur.push_str(line);
            cur.push('\n');
            continue;
        }
        if t.len() >= 2 && t.chars().all(|c| c == '-') {
            column.push(std::mem::take(&mut cur));
            if t.len() >= 3 {
                columns.push(std::mem::take(&mut column));
            }
            continue;
        }
        cur.push_str(line);
        cur.push('\n');
    }
    column.push(cur);
    columns.push(column);

    columns
        .into_iter()
        .map(|col| {
            col.into_iter()
                .filter(|s| !s.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|col| !col.is_empty())
        .collect()
}

/// Parse one slide's markdown source: split off `???` notes, extract
/// `terminal` and `theme` blocks, pick a title. Used for slices of a
/// single-file deck and for whole per-slide files referenced from a JSON
/// manifest.
pub fn parse_slide(src: &str, col: usize, row: usize, next_term_id: &mut usize) -> Result<Slide> {
    // Split off presenter notes at a bare `???` line (outside fences).
    let mut body = String::new();
    let mut notes = String::new();
    let mut in_notes = false;
    let mut fence: Fence = None;
    for line in src.lines() {
        let t = line.trim();
        match fence {
            Some((ch, n)) => {
                if closes_fence(t, ch, n) {
                    fence = None;
                }
            }
            None => {
                if let Some(f) = opens_fence(t) {
                    fence = Some(f);
                } else if t == "???" && !in_notes {
                    in_notes = true;
                    continue;
                }
            }
        }
        let target = if in_notes { &mut notes } else { &mut body };
        target.push_str(line);
        target.push('\n');
    }

    let (segments, theme) = parse_segments(&body, next_term_id)
        .with_context(|| format!("slide {}.{}", col + 1, row + 1))?;
    let title = extract_title(&body).unwrap_or_else(|| format!("{}.{}", col + 1, row + 1));

    Ok(Slide {
        title,
        segments,
        notes: notes.trim().to_string(),
        theme,
    })
}

/// A special fence being collected: a `terminal` block's body, or a
/// `theme` block's YAML.
enum Pending {
    Term(TermBlock, String),
    Theme(String),
}

fn parse_segments(
    body: &str,
    next_term_id: &mut usize,
) -> Result<(Vec<Segment>, Option<ThemeConfig>)> {
    let mut segments = Vec::new();
    let mut theme: Option<ThemeConfig> = None;
    let mut md = String::new();
    let mut fence: Fence = None;
    let mut pending: Option<Pending> = None;

    let flush_md = |md: &mut String, segments: &mut Vec<Segment>| {
        if !md.trim().is_empty() {
            segments.push(Segment::Markdown(std::mem::take(md)));
        } else {
            md.clear();
        }
    };
    // Body layout: command lines, then optionally a `%%snapshot COLSxROWS`
    // marker followed by base64-encoded ANSI screen bytes.
    let finish_term = |block: &mut TermBlock, tbody: &str| -> Result<()> {
        use base64::Engine as _;
        let mut command = String::new();
        let mut dims: Option<(u16, u16)> = None;
        let mut b64 = String::new();
        for line in tbody.lines() {
            if dims.is_none() {
                if let Some(rest) = line.trim().strip_prefix("%%snapshot") {
                    let (c, r) = rest
                        .trim()
                        .split_once('x')
                        .context("`%%snapshot` header needs COLSxROWS")?;
                    dims = Some((
                        c.trim().parse().context("snapshot cols")?,
                        r.trim().parse().context("snapshot rows")?,
                    ));
                } else {
                    command.push_str(line);
                    command.push('\n');
                }
            } else {
                b64.push_str(line.trim());
            }
        }
        let script = command.trim();
        block.command = (!script.is_empty()).then(|| script.to_string());
        if let Some((cols, rows)) = dims {
            let data = base64::engine::general_purpose::STANDARD
                .decode(b64.as_bytes())
                .context("decoding `%%snapshot` data")?;
            block.snapshot = Some(TermSnapshot { cols, rows, data });
        }
        Ok(())
    };
    let merge_theme = |theme: &mut Option<ThemeConfig>, tbody: &str| -> Result<()> {
        let cfg: ThemeConfig = serde_yaml::from_str(tbody).context("parsing `theme` block")?;
        *theme = Some(match theme.take() {
            Some(prev) => prev.merged(cfg),
            None => cfg,
        });
        Ok(())
    };

    for line in body.lines() {
        let t = line.trim();
        if let Some((ch, n)) = fence {
            if closes_fence(t, ch, n) {
                fence = None;
                match pending.take() {
                    Some(Pending::Term(mut block, tbody)) => {
                        finish_term(&mut block, &tbody)?;
                        flush_md(&mut md, &mut segments);
                        segments.push(Segment::Terminal(block));
                    }
                    Some(Pending::Theme(tbody)) => merge_theme(&mut theme, &tbody)?,
                    None => {
                        md.push_str(line);
                        md.push('\n');
                    }
                }
            } else if let Some(Pending::Term(_, tbody) | Pending::Theme(tbody)) = pending.as_mut() {
                tbody.push_str(line);
                tbody.push('\n');
            } else {
                md.push_str(line);
                md.push('\n');
            }
            continue;
        }
        if let Some(f) = opens_fence(t) {
            fence = Some(f);
            let info = t.trim_start_matches(['`', '~']).trim();
            let mut words = info.split_whitespace();
            match words.next() {
                Some("terminal") => {
                    let mut rows: u16 = 12;
                    let mut fill = false;
                    for w in words {
                        if let Some(v) = w.strip_prefix("rows=") {
                            if v == "fill" {
                                fill = true;
                            } else {
                                rows = v.parse().unwrap_or(12);
                            }
                        }
                    }
                    let id = *next_term_id;
                    *next_term_id += 1;
                    pending = Some(Pending::Term(
                        TermBlock {
                            id,
                            command: None,
                            rows: rows.clamp(3, 40),
                            fill,
                            snapshot: None,
                        },
                        String::new(),
                    ));
                }
                Some("theme") => pending = Some(Pending::Theme(String::new())),
                _ => {
                    md.push_str(line);
                    md.push('\n');
                }
            }
            continue;
        }
        md.push_str(line);
        md.push('\n');
    }
    // Unclosed special fence at end of slide.
    match pending.take() {
        Some(Pending::Term(mut block, tbody)) => {
            finish_term(&mut block, &tbody)?;
            flush_md(&mut md, &mut segments);
            segments.push(Segment::Terminal(block));
        }
        Some(Pending::Theme(tbody)) => merge_theme(&mut theme, &tbody)?,
        None => {}
    }
    flush_md(&mut md, &mut segments);
    Ok((segments, theme))
}

fn extract_title(body: &str) -> Option<String> {
    let mut fence: Fence = None;
    let mut first_text: Option<String> = None;
    for line in body.lines() {
        let t = line.trim();
        if let Some((ch, n)) = fence {
            if closes_fence(t, ch, n) {
                fence = None;
            }
            continue;
        }
        if let Some(f) = opens_fence(t) {
            fence = Some(f);
            continue;
        }
        if t.starts_with('#') {
            let title = t.trim_start_matches('#').trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
        if first_text.is_none() && !t.is_empty() {
            let cleaned: String = t
                .trim_start_matches(['>', '-', '*', ' '])
                .chars()
                .take(48)
                .collect();
            if !cleaned.is_empty() {
                first_text = Some(cleaned);
            }
        }
    }
    first_text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_and_depth() {
        let deck = parse("# a\n---\n# b\n--\n# b2\n---\n# c\n", "t").unwrap();
        assert_eq!(deck.columns.len(), 3);
        assert_eq!(deck.columns[0].slides.len(), 1);
        assert_eq!(deck.columns[1].slides.len(), 2);
        assert_eq!(deck.columns[1].slides[1].title, "b2");
        assert_eq!(deck.flat().len(), 4);
    }

    #[test]
    fn separators_inside_fences_are_ignored() {
        let deck = parse("# one\n```\n---\n--\n```\nafter\n", "t").unwrap();
        assert_eq!(deck.columns.len(), 1);
        assert_eq!(deck.columns[0].slides.len(), 1);
        // fence content preserved in the markdown segment
        match &deck.columns[0].slides[0].segments[0] {
            Segment::Markdown(md) => assert!(md.contains("---")),
            _ => panic!("expected markdown segment"),
        }
    }

    #[test]
    fn notes_split() {
        let deck = parse("# a\nvisible\n???\nsecret note\nmore\n", "t").unwrap();
        let slide = deck.slide(0, 0);
        assert_eq!(slide.notes, "secret note\nmore");
        match &slide.segments[0] {
            Segment::Markdown(md) => {
                assert!(md.contains("visible"));
                assert!(!md.contains("secret"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn terminal_block() {
        let deck = parse("# a\nbefore\n```terminal rows=8\nhtop\n```\nafter\n", "t").unwrap();
        let slide = deck.slide(0, 0);
        assert_eq!(slide.segments.len(), 3);
        match &slide.segments[1] {
            Segment::Terminal(b) => {
                assert_eq!(b.command.as_deref(), Some("htop"));
                assert_eq!(b.rows, 8);
                assert_eq!(b.id, 0);
            }
            _ => panic!("expected terminal segment"),
        }
    }

    #[test]
    fn terminal_snapshot_parses() {
        // "aGk=" is base64 for "hi"
        let deck = parse(
            "```terminal rows=4\nhtop\n%%snapshot 80x4\naGk=\n```\n",
            "t",
        )
        .unwrap();
        match &deck.slide(0, 0).segments[0] {
            Segment::Terminal(b) => {
                assert_eq!(b.command.as_deref(), Some("htop"));
                let snap = b.snapshot.as_ref().unwrap();
                assert_eq!((snap.cols, snap.rows), (80, 4));
                assert_eq!(snap.data, b"hi");
            }
            _ => panic!("expected terminal"),
        }
    }

    #[test]
    fn bad_snapshot_header_errors() {
        assert!(parse("```terminal\n%%snapshot nope\n```\n", "t").is_err());
    }

    #[test]
    fn empty_terminal_block_is_shell() {
        let deck = parse("```terminal\n```\n", "t").unwrap();
        match &deck.slide(0, 0).segments[0] {
            Segment::Terminal(b) => assert!(b.command.is_none()),
            _ => panic!(),
        }
    }

    #[test]
    fn slide_theme_block() {
        let deck = parse(
            "# a\n```theme\nmargin: 0\naccent: red\n```\nbody\n---\n# b\n",
            "t",
        )
        .unwrap();
        let slide = deck.slide(0, 0);
        let theme = slide.theme.as_ref().unwrap();
        assert_eq!(theme.margin, Some(0));
        // the theme block doesn't leak into rendered content
        match &slide.segments[0] {
            Segment::Markdown(md) => assert!(!md.contains("margin")),
            _ => panic!(),
        }
        assert!(deck.slide(1, 0).theme.is_none());
    }

    #[test]
    fn bad_slide_theme_block_errors() {
        let err = parse("# a\n```theme\nbogus_key: 1\n```\n", "t").unwrap_err();
        assert!(format!("{err:#}").contains("slide 1.1"));
    }

    #[test]
    fn frontmatter_theme_and_title() {
        let src = "---\ntitle: my talk\ntheme:\n  accent: magenta\n  max_width: 80\n---\n\n# a\n---\n# b\n";
        let (deck, theme) = parse_full(src, "fallback").unwrap();
        assert_eq!(deck.title, "my talk");
        assert_eq!(deck.columns.len(), 2);
        let theme = theme.unwrap();
        assert_eq!(theme.max_width, Some(80));
        let resolved = crate::theme::Theme::resolve(vec![theme]).unwrap();
        assert_eq!(resolved.accent, ratatui::style::Color::Magenta);
    }

    #[test]
    fn no_frontmatter_is_unchanged() {
        let (deck, theme) = parse_full("# a\nhello\n", "fallback").unwrap();
        assert!(theme.is_none());
        assert_eq!(deck.title, "fallback");
        assert_eq!(deck.columns.len(), 1);
    }

    #[test]
    fn non_mapping_leading_block_stays_content() {
        // `---` has always also meant "column separator"; only a YAML
        // mapping is claimed as frontmatter.
        let src = "---\njust some text\n---\nreal slide\n";
        let (deck, theme) = parse_full(src, "fallback").unwrap();
        assert!(theme.is_none());
        assert_eq!(deck.columns.len(), 2);
    }

    #[test]
    fn bad_frontmatter_errors() {
        let src = "---\ntitle: x\nbogus_key: 1\n---\n# a\n";
        assert!(parse_full(src, "fallback").is_err());
    }

    #[test]
    fn term_ids_are_global() {
        let deck = parse("```terminal\n```\n---\n```terminal\n```\n", "t").unwrap();
        assert_eq!(deck.slide(0, 0).term_ids(), vec![0]);
        assert_eq!(deck.slide(1, 0).term_ids(), vec![1]);
    }
}
