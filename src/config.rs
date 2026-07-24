//! JSON deck manifests: explicit 2-D layout, one markdown file per slide,
//! and slides that are nothing but a terminal.
//!
//! ```json
//! {
//!   "title": "my talk",
//!   "columns": [
//!     "slides/intro.md",
//!     ["slides/topic.md", "slides/topic-deep.md"],
//!     { "terminal": { "command": "htop" }, "title": "live demo" },
//!     { "file": "slides/fin.md", "notes_file": "slides/fin-notes.md" }
//!   ]
//! }
//! ```
//!
//! A column is either one slide or an array of slides (top = shallow,
//! later entries = deeper). A slide is either a markdown path (string) or
//! an object. Paths are relative to the manifest. Referenced markdown files
//! are one slide each — `---`/`--` are not split — but `???` notes and
//! `terminal` fences inside them still work.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::deck::{Column, Deck, Segment, Slide, TermBlock};

/// The top level of a `deck.json` manifest.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckConfig {
    /// Deck title; defaults to the manifest's file stem.
    pub title: Option<String>,
    /// The presentation, left to right.
    pub columns: Vec<ColumnSpec>,
    /// Optional theme overrides; see `theme.rs`. Wins over the user-level
    /// `~/.config/deckhand/theme.json`.
    pub theme: Option<crate::theme::ThemeConfig>,
}

/// One entry of `columns`: a single slide or a stack with depth.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ColumnSpec {
    /// A column with a single (shallow) slide.
    Single(SlideSpec),
    /// A column with depth: index 0 is the top, later entries are deeper.
    Stack(Vec<SlideSpec>),
}

/// One slide in the manifest.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SlideSpec {
    /// Shorthand: a path to a markdown file.
    Path(PathBuf),
    /// The full object form.
    Full(Box<SlideConfig>),
}

/// The object form of a slide: exactly one content source plus optional
/// metadata overrides.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlideConfig {
    /// Markdown file rendered as the slide body.
    pub file: Option<PathBuf>,
    /// Make the whole slide a live terminal instead of markdown.
    pub terminal: Option<TermConfig>,
    /// Compose the slide from a vertical stack of panes, each a markdown
    /// file or a terminal. Exactly one of `file`/`terminal`/`panes`.
    pub panes: Option<Vec<PaneSpec>>,
    /// Override the slide title (shown in overview and notes).
    pub title: Option<String>,
    /// Inline presenter notes (markdown). Overrides notes from the file.
    pub notes: Option<String>,
    /// Presenter notes loaded from a file. `notes` wins if both are set.
    pub notes_file: Option<PathBuf>,
    /// Theme overrides for this slide only, layered over the deck theme.
    /// Applies to the slide's content (margins, width, borders, colors);
    /// global chrome like the status bar keeps the deck theme.
    pub theme: Option<crate::theme::ThemeConfig>,
}

/// One pane of a `panes` slide.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum PaneSpec {
    /// Shorthand: a path to a markdown file.
    Path(PathBuf),
    /// The full object form.
    Full(PaneConfig),
}

/// The object form of a pane: exactly one of `file` or `terminal`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaneConfig {
    /// Markdown file rendered as this pane.
    pub file: Option<PathBuf>,
    /// A live terminal as this pane.
    pub terminal: Option<TermConfig>,
}

/// A terminal in the manifest (whole-slide or pane).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermConfig {
    /// Run via `$SHELL -c`; omit for an interactive shell.
    pub command: Option<String>,
    /// Viewport height; omit to fill the whole slide.
    pub rows: Option<u16>,
}

/// Fetches a slide/notes file referenced by a manifest, given its
/// manifest-relative path. Local decks read the filesystem; remote decks
/// fetch over HTTP (see `source.rs`).
pub type Reader<'a> = &'a dyn Fn(&Path) -> Result<String>;

/// Load a manifest from disk, resolving referenced files relative to it.
pub fn load(path: &Path) -> Result<(Deck, Option<crate::theme::ThemeConfig>)> {
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("reading deck config {}", path.display()))?;
    let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let reader = move |p: &Path| -> Result<String> {
        let full = base.join(p);
        std::fs::read_to_string(&full).with_context(|| format!("reading {}", full.display()))
    };
    parse_manifest(&src, &path.display().to_string(), &reader)
}

/// Parse a manifest from source text, resolving referenced files through
/// `reader`. `label` names the manifest in errors and title fallbacks.
///
/// The reader abstraction is what lets manifests load from the
/// filesystem, over HTTP, or from an in-memory bundle (as the browser
/// presenter does with fetched gists):
///
/// ```
/// use deckhand::config;
///
/// let reader = |p: &std::path::Path| match p.to_str() {
///     Some("intro.md") => Ok("# hello\n".to_string()),
///     other => anyhow::bail!("no such file: {other:?}"),
/// };
/// let (deck, _theme) =
///     config::parse_manifest(r#"{ "columns": ["intro.md"] }"#, "deck.json", &reader)?;
/// assert_eq!(deck.slide(0, 0).title, "hello");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn parse_manifest(
    src: &str,
    label: &str,
    reader: Reader,
) -> Result<(Deck, Option<crate::theme::ThemeConfig>)> {
    let mut cfg: DeckConfig =
        serde_json::from_str(src).with_context(|| format!("parsing deck config {label}"))?;
    let theme = cfg.theme.take();

    if cfg.columns.is_empty() {
        bail!("{label}: deck has no columns");
    }
    let mut next_term_id = 0usize;
    let mut columns = Vec::new();
    for (c, colspec) in cfg.columns.into_iter().enumerate() {
        let specs = match colspec {
            ColumnSpec::Single(s) => vec![s],
            ColumnSpec::Stack(v) => v,
        };
        if specs.is_empty() {
            bail!("{label}: column {} is empty", c + 1);
        }
        let mut slides = Vec::new();
        for (r, spec) in specs.into_iter().enumerate() {
            slides.push(build_slide(spec, reader, c, r, &mut next_term_id)?);
        }
        columns.push(Column { slides });
    }

    let title = cfg.title.unwrap_or_else(|| {
        Path::new(label)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "deck".to_string())
    });
    Ok((Deck { title, columns }, theme))
}

/// Every file path a manifest references (slides, notes files, panes),
/// relative to the manifest — used to build live-reload watch lists.
/// Best-effort: an unparseable manifest yields an empty list.
pub fn referenced_files(src: &str) -> Vec<PathBuf> {
    fn from_slide(spec: &SlideSpec, out: &mut Vec<PathBuf>) {
        match spec {
            SlideSpec::Path(p) => out.push(p.clone()),
            SlideSpec::Full(cfg) => {
                if let Some(f) = &cfg.file {
                    out.push(f.clone());
                }
                if let Some(nf) = &cfg.notes_file {
                    out.push(nf.clone());
                }
                for pane in cfg.panes.iter().flatten() {
                    match pane {
                        PaneSpec::Path(p) => out.push(p.clone()),
                        PaneSpec::Full(pc) => {
                            if let Some(f) = &pc.file {
                                out.push(f.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    let Ok(cfg) = serde_json::from_str::<DeckConfig>(src) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for column in &cfg.columns {
        match column {
            ColumnSpec::Single(s) => from_slide(s, &mut out),
            ColumnSpec::Stack(v) => {
                for s in v {
                    from_slide(s, &mut out);
                }
            }
        }
    }
    out
}

fn build_slide(
    spec: SlideSpec,
    reader: Reader,
    col: usize,
    row: usize,
    next_term_id: &mut usize,
) -> Result<Slide> {
    let mut cfg = match spec {
        SlideSpec::Path(p) => SlideConfig {
            file: Some(p),
            terminal: None,
            panes: None,
            title: None,
            notes: None,
            notes_file: None,
            theme: None,
        },
        SlideSpec::Full(c) => *c,
    };

    let picked =
        cfg.file.is_some() as u8 + cfg.terminal.is_some() as u8 + cfg.panes.is_some() as u8;
    if picked != 1 {
        bail!(
            "slide {}.{}: needs exactly one of `file`, `terminal`, or `panes`",
            col + 1,
            row + 1
        );
    }

    let mut slide = if let Some(file) = &cfg.file {
        let src = reader(file).with_context(|| format!("slide {}.{}", col + 1, row + 1))?;
        crate::deck::parse_slide(&src, col, row, next_term_id)?
    } else if let Some(term) = &cfg.terminal {
        let title = cfg
            .title
            .clone()
            .or_else(|| {
                term.command
                    .as_deref()
                    .and_then(|c| c.lines().next())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "terminal".to_string());
        Slide {
            title,
            segments: vec![term_segment(term, next_term_id)],
            notes: String::new(),
            theme: None,
        }
    } else {
        let panes = cfg.panes.take().expect("checked above");
        build_panes(panes, reader, col, row, next_term_id)?
    };

    if let Some(title) = cfg.title {
        slide.title = title;
    }
    if let Some(nf) = cfg.notes_file {
        slide.notes = reader(&nf)
            .with_context(|| format!("slide {}.{}: notes_file", col + 1, row + 1))?
            .trim()
            .to_string();
    }
    if let Some(notes) = cfg.notes {
        slide.notes = notes;
    }
    // A slide file's own `theme` block merges under the manifest's
    // per-slide theme (manifest wins field by field).
    slide.theme = match (slide.theme.take(), cfg.theme) {
        (Some(from_file), Some(from_manifest)) => Some(from_file.merged(from_manifest)),
        (from_file, from_manifest) => from_manifest.or(from_file),
    };
    Ok(slide)
}

fn term_segment(term: &TermConfig, next_term_id: &mut usize) -> Segment {
    let id = *next_term_id;
    *next_term_id += 1;
    Segment::Terminal(TermBlock {
        id,
        command: term.command.clone(),
        rows: term
            .rows
            .unwrap_or(12)
            .clamp(crate::deck::TERM_ROWS_MIN, crate::deck::TERM_ROWS_MAX),
        fill: term.rows.is_none(),
        snapshot: None,
    })
}

/// A slide assembled from stacked panes. Markdown panes contribute their
/// segments and `???` notes (concatenated); the first markdown pane's
/// title names the slide unless the slide config overrides it.
fn build_panes(
    panes: Vec<PaneSpec>,
    reader: Reader,
    col: usize,
    row: usize,
    next_term_id: &mut usize,
) -> Result<Slide> {
    if panes.is_empty() {
        bail!("slide {}.{}: `panes` is empty", col + 1, row + 1);
    }
    let mut segments = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut title: Option<String> = None;
    let mut theme: Option<crate::theme::ThemeConfig> = None;
    for (i, pane) in panes.into_iter().enumerate() {
        let pc = match pane {
            PaneSpec::Path(p) => PaneConfig {
                file: Some(p),
                terminal: None,
            },
            PaneSpec::Full(c) => c,
        };
        match (&pc.file, &pc.terminal) {
            (Some(file), None) => {
                let src = reader(file)
                    .with_context(|| format!("slide {}.{} pane {}", col + 1, row + 1, i + 1))?;
                let parsed = crate::deck::parse_slide(&src, col, row, next_term_id)
                    .with_context(|| format!("slide {}.{} pane {}", col + 1, row + 1, i + 1))?;
                title = title.or(Some(parsed.title));
                if !parsed.notes.is_empty() {
                    notes.push(parsed.notes);
                }
                if let Some(pane_theme) = parsed.theme {
                    theme = Some(match theme.take() {
                        Some(prev) => prev.merged(pane_theme),
                        None => pane_theme,
                    });
                }
                segments.extend(parsed.segments);
            }
            (None, Some(term)) => segments.push(term_segment(term, next_term_id)),
            _ => bail!(
                "slide {}.{} pane {}: needs exactly one of `file` or `terminal`",
                col + 1,
                row + 1,
                i + 1
            ),
        }
    }
    Ok(Slide {
        title: title.unwrap_or_else(|| format!("{}.{}", col + 1, row + 1)),
        segments,
        notes: notes.join("\n\n"),
        theme,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("deckhand-test-{}", std::process::id()))
            .join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn manifest_shapes_parse() {
        let cfg: DeckConfig = serde_json::from_str(
            r#"{
                "title": "t",
                "columns": [
                    "a.md",
                    ["b.md", { "file": "c.md", "notes": "n" }],
                    { "terminal": { "command": "htop", "rows": 20 } },
                    { "terminal": {} }
                ]
            }"#,
        )
        .unwrap();
        assert_eq!(cfg.columns.len(), 4);
        assert!(matches!(
            cfg.columns[0],
            ColumnSpec::Single(SlideSpec::Path(_))
        ));
        assert!(matches!(&cfg.columns[1], ColumnSpec::Stack(v) if v.len() == 2));
    }

    #[test]
    fn json_deck_end_to_end() {
        let dir = scratch("e2e");
        std::fs::write(dir.join("a.md"), "# alpha\nhello\n???\nnote a\n").unwrap();
        std::fs::write(dir.join("b.md"), "# beta\n").unwrap();
        std::fs::write(dir.join("n.md"), "external note\n").unwrap();
        std::fs::write(
            dir.join("deck.json"),
            r#"{
                "theme": { "accent": "magenta", "border_type": "rounded" },
                "columns": [
                    "a.md",
                    [ { "file": "b.md", "title": "renamed", "notes_file": "n.md" },
                      { "terminal": { "command": "top" }, "theme": { "margin": 0 } } ]
                ]
            }"#,
        )
        .unwrap();

        let (deck, theme) = load(&dir.join("deck.json")).unwrap();
        assert!(theme.is_some());
        assert_eq!(deck.title, "deck");
        assert_eq!(deck.columns.len(), 2);
        assert_eq!(deck.slide(0, 0).title, "alpha");
        assert_eq!(deck.slide(0, 0).notes, "note a");
        assert_eq!(deck.slide(1, 0).title, "renamed");
        assert_eq!(deck.slide(1, 0).notes, "external note");
        let term_slide = deck.slide(1, 1);
        assert_eq!(term_slide.title, "top");
        assert_eq!(term_slide.theme.as_ref().unwrap().margin, Some(0));
        assert!(deck.slide(1, 0).theme.is_none());
        match &term_slide.segments[0] {
            Segment::Terminal(b) => {
                assert_eq!(b.command.as_deref(), Some("top"));
                assert!(b.fill);
            }
            _ => panic!("expected terminal slide"),
        }
    }

    #[test]
    fn manifest_with_in_memory_reader() {
        // The reader abstraction is what lets manifests load over HTTP —
        // prove the manifest path never touches the filesystem directly.
        let reader = |p: &std::path::Path| -> anyhow::Result<String> {
            match p.to_string_lossy().as_ref() {
                "a.md" => Ok("# remote\n???\nnote\n".to_string()),
                other => anyhow::bail!("unexpected read: {other}"),
            }
        };
        let (deck, _) = parse_manifest(
            r#"{ "columns": [ "a.md" ] }"#,
            "https://x/deck.json",
            &reader,
        )
        .unwrap();
        assert_eq!(deck.title, "deck");
        assert_eq!(deck.slide(0, 0).title, "remote");
        assert_eq!(deck.slide(0, 0).notes, "note");
    }

    #[test]
    fn referenced_files_covers_every_shape() {
        let files = referenced_files(
            r#"{ "columns": [
                "a.md",
                ["b.md", { "file": "c.md", "notes_file": "n.md" }],
                { "panes": ["p.md", { "terminal": {} }, { "file": "q.md" }] }
            ] }"#,
        );
        let names: Vec<_> = files.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(names, ["a.md", "b.md", "c.md", "n.md", "p.md", "q.md"]);
        assert!(referenced_files("not json").is_empty());
    }

    #[test]
    fn panes_slide() {
        let dir = scratch("panes");
        std::fs::write(dir.join("a.md"), "# top pane\nhello\n???\nnote one\n").unwrap();
        std::fs::write(dir.join("b.md"), "bottom pane\n???\nnote two\n").unwrap();
        std::fs::write(
            dir.join("deck.json"),
            r#"{ "columns": [ { "panes": [
                "a.md",
                { "terminal": { "rows": 8 } },
                { "terminal": { "command": "top" } },
                "b.md"
            ] } ] }"#,
        )
        .unwrap();

        let (deck, _) = load(&dir.join("deck.json")).unwrap();
        let slide = deck.slide(0, 0);
        assert_eq!(slide.title, "top pane");
        assert_eq!(slide.notes, "note one\n\nnote two");
        assert_eq!(slide.segments.len(), 4);
        assert_eq!(slide.term_ids(), vec![0, 1]);
        match (&slide.segments[1], &slide.segments[2]) {
            (Segment::Terminal(a), Segment::Terminal(b)) => {
                assert!(!a.fill);
                assert_eq!(a.rows, 8);
                assert!(b.fill);
            }
            _ => panic!("expected stacked terminals"),
        }
    }

    #[test]
    fn bad_panes_rejected() {
        let dir = scratch("bad-panes");
        std::fs::write(
            dir.join("deck.json"),
            r#"{ "columns": [ { "file": "x.md", "panes": [] } ] }"#,
        )
        .unwrap();
        assert!(load(&dir.join("deck.json")).is_err());

        std::fs::write(
            dir.join("deck2.json"),
            r#"{ "columns": [ { "panes": [] } ] }"#,
        )
        .unwrap();
        assert!(load(&dir.join("deck2.json")).is_err());
    }

    #[test]
    fn bad_slides_rejected() {
        let dir = scratch("bad");
        std::fs::write(
            dir.join("deck.json"),
            r#"{ "columns": [ { "file": "x.md", "terminal": {} } ] }"#,
        )
        .unwrap();
        assert!(load(&dir.join("deck.json")).is_err());

        std::fs::write(dir.join("deck2.json"), r#"{ "columns": [ {} ] }"#).unwrap();
        assert!(load(&dir.join("deck2.json")).is_err());
    }
}
