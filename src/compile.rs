//! `deckhand compile`: flatten a deck — typically a JSON manifest with
//! per-slide files — into a single-file markdown deck that presents
//! identically (columns as `---`, depth as `--`, notes as `???`,
//! terminals as `terminal` fences).
//!
//! The deck title and deck-level theme are preserved as frontmatter, and
//! per-slide themes as `theme` fences. Slide `title` overrides have no
//! single-file syntax and fall back to what the content implies. Lines
//! the parser would read as separators are defused (with a warning):
//! bare `---`/`--` in slide bodies become `***`, and bare `||` lines
//! inside row cells are backslash-escaped. Slides with no content at
//! all emit an HTML comment so they aren't dropped on re-parse.

use anyhow::{Context, Result};

use crate::deck::{self, Deck, Segment};

/// CLI entry point; native-only because it can load remote decks and
/// capture snapshots.
///
/// `snapshots`: `Some(true)` captures, `Some(false)` skips, and `None`
/// means the user didn't choose — an error if the deck has terminal
/// blocks without baked snapshots, since capturing would run their
/// commands and skipping would silently lose the output (see
/// [`crate::export::load`]).
#[cfg(not(target_arch = "wasm32"))]
pub fn run(
    input: &str,
    output: Option<&std::path::Path>,
    snapshots: Option<bool>,
    snapshot_opts: crate::snapshot::Options,
) -> Result<()> {
    let crate::source::Loaded {
        deck,
        theme: deck_theme,
        ..
    } = crate::export::load(input, snapshots, &snapshot_opts)?;

    let (body, rewrites) = compile(&deck)?;
    if rewrites > 0 {
        eprintln!(
            "warning: rewrote {rewrites} separator-lookalike line(s) inside slide content \
             (bare `---`/`--` to `***`, bare `||` in row cells escaped) \
             so they don't split slides on re-parse"
        );
    }

    // Frontmatter preserves the deck title and deck-level theme.
    let front = deck::FrontMatter {
        title: Some(deck.title.clone()),
        theme: deck_theme,
    };
    let mut md = String::from("---\n");
    md.push_str(&serde_yaml::to_string(&front).context("serializing frontmatter")?);
    md.push_str("---\n\n");
    md.push_str(&body);

    match output {
        Some(path) => {
            std::fs::write(path, &md).with_context(|| format!("writing {}", path.display()))?;
            eprintln!("wrote {}", path.display());
        }
        None => print!("{md}"),
    }
    Ok(())
}

/// Render the deck back to single-file markdown. Returns the markdown and
/// how many separator-lookalike lines were rewritten.
///
/// Compiled output re-parses to an equivalent deck:
///
/// ```
/// use deckhand::{compile, deck};
///
/// let original = deck::parse("# a\n---\n# b\n???\nnotes here\n", "talk")?;
/// let (markdown, rewrites) = compile::compile(&original)?;
/// assert_eq!(rewrites, 0);
///
/// let reparsed = deck::parse(&markdown, "talk")?;
/// assert_eq!(reparsed.columns.len(), 2);
/// assert_eq!(reparsed.slide(1, 0).notes, "notes here");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn compile(deck: &Deck) -> Result<(String, usize)> {
    let mut out = String::new();
    let mut rewrites = 0usize;
    for (c, column) in deck.columns.iter().enumerate() {
        if c > 0 {
            out.push_str("---\n\n");
        }
        for (r, slide) in column.slides.iter().enumerate() {
            if r > 0 {
                out.push_str("--\n\n");
            }
            if let Some(theme) = &slide.theme {
                out.push_str("```theme\n");
                out.push_str(&serde_yaml::to_string(theme).context("serializing slide theme")?);
                out.push_str("```\n\n");
            }
            let body = emit_segments(&slide.segments, &mut rewrites, false);
            // A slide that emits nothing at all (an intentionally blank
            // slide from a manifest) would be dropped on re-parse; an
            // HTML comment is non-empty to the parser and renders as
            // nothing.
            if body.trim().is_empty() && slide.theme.is_none() && slide.notes.is_empty() {
                out.push_str("<!-- blank slide -->\n\n");
            } else {
                out.push_str(&body);
            }
            if !slide.notes.is_empty() {
                out.push_str("???\n\n");
                out.push_str(&sanitize(slide.notes.trim(), &mut rewrites, false));
                out.push_str("\n\n");
            }
        }
    }
    Ok((out, rewrites))
}

/// Emit segments as single-file markdown; row cells recurse (one
/// level, matching the parser). `in_row` marks cell content, where a
/// bare `||` line would additionally split cells on re-parse.
fn emit_segments(segments: &[Segment], rewrites: &mut usize, in_row: bool) -> String {
    let mut out = String::new();
    for seg in segments {
        match seg {
            Segment::Markdown(src) => {
                out.push_str(&sanitize(src.trim_matches('\n'), rewrites, in_row));
                out.push_str("\n\n");
            }
            Segment::Terminal(block) => {
                let rows = if block.fill {
                    "fill".to_string()
                } else {
                    block.rows.to_string()
                };
                out.push_str(&format!("```terminal rows={rows}\n"));
                if let Some(cmd) = &block.command {
                    out.push_str(cmd);
                    out.push('\n');
                }
                if let Some(snap) = &block.snapshot {
                    use base64::Engine as _;
                    out.push_str(&format!("%%snapshot {}x{}\n", snap.cols, snap.rows));
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&snap.data);
                    for chunk in b64.as_bytes().chunks(76) {
                        out.push_str(std::str::from_utf8(chunk).expect("base64 is ascii"));
                        out.push('\n');
                    }
                }
                out.push_str("```\n\n");
            }
            Segment::Qr(q) => {
                out.push_str("```qr\n");
                out.push_str(&q.data);
                out.push_str("\n```\n\n");
            }
            Segment::Image(img) => {
                out.push_str(&format!("![{}]({})\n\n", img.alt, img.path));
            }
            Segment::Row(cells) => {
                let bodies: Vec<String> = cells
                    .iter()
                    .map(|cell| {
                        emit_segments(cell, rewrites, true)
                            .trim_matches('\n')
                            .to_string()
                    })
                    .collect();
                let body = bodies.join("\n||\n");
                // The outer fence must outrun any fence in the cells.
                let ticks = "`".repeat(fence_len(&body));
                out.push_str(&format!("{ticks}row\n{body}\n{ticks}\n\n"));
            }
        }
    }
    out
}

/// A backtick fence long enough to wrap `body` without a cell fence
/// closing it early: one longer than the longest backtick run opening
/// any line, and at least three.
fn fence_len(body: &str) -> usize {
    body.lines()
        .map(|l| l.trim_start().chars().take_while(|c| *c == '`').count())
        .max()
        .unwrap_or(0)
        .max(2)
        + 1
}

/// Rewrite lines the parser would read as separators (outside code
/// fences). Each context has exactly one hazard: outside rows, bare
/// runs of `-` would split slides, so they become `***` (the same
/// horizontal rule); inside row cells (`in_row`) the emitted row fence
/// already shields dashes, but bare runs of `|` would split cells, so
/// they're backslash-escaped (the same rendered pipes).
fn sanitize(src: &str, rewrites: &mut usize, in_row: bool) -> String {
    let mut out = Vec::new();
    let mut fence: deck::Fence = None;
    for line in src.lines() {
        let t = line.trim();
        if let Some((ch, n)) = fence {
            if deck::closes_fence(t, ch, n) {
                fence = None;
            }
            out.push(line.to_string());
            continue;
        }
        if let Some(f) = deck::opens_fence(t) {
            fence = Some(f);
            out.push(line.to_string());
            continue;
        }
        if !in_row && t.len() >= 2 && t.chars().all(|c| c == '-') {
            *rewrites += 1;
            out.push("***".to_string());
            continue;
        }
        if in_row && t.len() >= 2 && t.chars().all(|c| c == '|') {
            *rewrites += 1;
            out.push(format!("\\{t}"));
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::{Column, Slide, TermBlock};
    use std::path::PathBuf;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("deckhand-test-{}", std::process::id()))
            .join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn json_deck_round_trips() {
        let dir = scratch("compile");
        std::fs::write(
            dir.join("a.md"),
            "# alpha\nbody text\n```terminal rows=6\nls\n```\n???\nnote a\n",
        )
        .unwrap();
        std::fs::write(dir.join("b.md"), "# beta\n").unwrap();
        std::fs::write(
            dir.join("deck.json"),
            r#"{ "columns": [
                "a.md",
                [ "b.md", { "terminal": { "command": "top" }, "notes": "watch it" } ]
            ] }"#,
        )
        .unwrap();

        let (original, _) = deck::load(&dir.join("deck.json")).unwrap();
        let (md, rewrites) = compile(&original).unwrap();
        assert_eq!(rewrites, 0);

        let reparsed = deck::parse(&md, "t").unwrap();
        assert_eq!(reparsed.columns.len(), 2);
        assert_eq!(reparsed.columns[0].slides.len(), 1);
        assert_eq!(reparsed.columns[1].slides.len(), 2);
        assert_eq!(reparsed.slide(0, 0).title, "alpha");
        assert_eq!(reparsed.slide(0, 0).notes, "note a");
        assert_eq!(reparsed.slide(0, 0).term_ids().len(), 1);
        let term = reparsed.slide(1, 1);
        assert_eq!(term.notes, "watch it");
        match &term.segments[0] {
            Segment::Terminal(b) => {
                assert_eq!(b.command.as_deref(), Some("top"));
                assert!(b.fill);
            }
            _ => panic!("expected terminal"),
        }
    }

    #[test]
    fn qr_and_image_round_trip() {
        let src = "# a\n```qr\nhttps://deckhand.sh\n```\n![diagram](arch.png)\n";
        let deck = deck::parse(src, "t").unwrap();
        let (md, rewrites) = compile(&deck).unwrap();
        assert_eq!(rewrites, 0);
        let reparsed = deck::parse(&md, "t").unwrap();
        let slide = reparsed.slide(0, 0);
        match (&slide.segments[1], &slide.segments[2]) {
            (Segment::Qr(q), Segment::Image(img)) => {
                assert_eq!(q.data, "https://deckhand.sh");
                assert_eq!(img.path, "arch.png");
                assert_eq!(img.alt, "diagram");
            }
            other => panic!("unexpected segments: {other:?}"),
        }
    }

    #[test]
    fn rows_round_trip() {
        let src =
            "````row\n```qr\nhi\n```\n||\n- right *cell*\n\n```terminal rows=5\nls\n```\n````\n";
        let deck = deck::parse(src, "t").unwrap();
        let (md, _) = compile(&deck).unwrap();
        // The emitted fence must outrun the 3-tick fences inside.
        assert!(md.contains("````row"), "compiled:\n{md}");
        let reparsed = deck::parse(&md, "t").unwrap();
        match &reparsed.slide(0, 0).segments[0] {
            Segment::Row(cells) => {
                assert_eq!(cells.len(), 2);
                assert!(matches!(cells[0][0], Segment::Qr(_)));
                assert_eq!(cells[1].len(), 2);
                match &cells[1][1] {
                    Segment::Terminal(b) => assert_eq!(b.rows, 5),
                    _ => panic!("expected terminal in right cell"),
                }
            }
            other => panic!("expected row, got {other:?}"),
        }
    }

    #[test]
    fn slide_themes_round_trip() {
        let dir = scratch("compile-slide-theme");
        std::fs::write(dir.join("a.md"), "# alpha\n").unwrap();
        std::fs::write(
            dir.join("deck.json"),
            r#"{ "columns": [ [
                "a.md",
                { "terminal": { "command": "htop" },
                  "theme": { "margin": 0, "accent": "red" } }
            ] ] }"#,
        )
        .unwrap();

        let (original, _) = deck::load(&dir.join("deck.json")).unwrap();
        let (md, _) = compile(&original).unwrap();
        assert!(md.contains("```theme"));

        let reparsed = deck::parse(&md, "t").unwrap();
        assert!(reparsed.slide(0, 0).theme.is_none());
        let theme = reparsed.slide(0, 1).theme.as_ref().unwrap();
        assert_eq!(theme.margin, Some(0));
        assert!(theme.accent.is_some());
    }

    #[test]
    fn run_preserves_title_and_theme_via_frontmatter() {
        let dir = scratch("compile-fm");
        std::fs::write(dir.join("a.md"), "# alpha\n").unwrap();
        std::fs::write(
            dir.join("deck.json"),
            r#"{ "title": "fancy talk",
                 "theme": { "accent": "magenta", "border_type": "rounded" },
                 "columns": [ "a.md" ] }"#,
        )
        .unwrap();
        let out = dir.join("out.md");
        run(
            dir.join("deck.json").to_str().unwrap(),
            Some(&out),
            None,
            crate::snapshot::Options::default(),
        )
        .unwrap();

        let (deck, theme) = deck::load(&out).unwrap();
        assert_eq!(deck.title, "fancy talk");
        let theme = theme.unwrap();
        assert_eq!(theme.border_type.as_deref(), Some("rounded"));
        assert_eq!(deck.columns.len(), 1);
        assert_eq!(deck.slide(0, 0).title, "alpha");
    }

    #[test]
    fn terminal_decks_require_a_snapshot_choice() {
        let dir = scratch("compile-choice");
        std::fs::write(dir.join("deck.md"), "# a\n```terminal\nhtop\n```\n").unwrap();
        let path = dir.join("deck.md");
        let path = path.to_str().unwrap();
        let out = dir.join("out.md");
        let opts = || crate::snapshot::Options::default();

        // No choice: refuse, and name the commands that would run.
        let err = run(path, Some(&out), None, opts()).unwrap_err();
        assert!(format!("{err:#}").contains("htop"));

        // Explicit opt-out compiles without captures.
        run(path, Some(&out), Some(false), opts()).unwrap();
        let md = std::fs::read_to_string(&out).unwrap();
        assert!(md.contains("```terminal"));
        assert!(!md.contains("%%snapshot"));
    }

    #[test]
    fn snapshots_round_trip() {
        use crate::deck::TermSnapshot;
        let mut deck = deck::parse("```terminal rows=4\nhtop\n```\n", "t").unwrap();
        let Segment::Terminal(block) = &mut deck.columns[0].slides[0].segments[0] else {
            panic!("expected terminal");
        };
        block.snapshot = Some(TermSnapshot {
            cols: 40,
            rows: 4,
            data: b"\x1b[1;1Hhello \x1b[31mred\x1b[0m".to_vec(),
        });

        let (md, _) = compile(&deck).unwrap();
        assert!(md.contains("%%snapshot 40x4"));

        let reparsed = deck::parse(&md, "t").unwrap();
        let Segment::Terminal(block) = &reparsed.slide(0, 0).segments[0] else {
            panic!("expected terminal");
        };
        assert_eq!(block.command.as_deref(), Some("htop"));
        let snap = block.snapshot.as_ref().unwrap();
        assert_eq!((snap.cols, snap.rows), (40, 4));
        assert_eq!(snap.data, b"\x1b[1;1Hhello \x1b[31mred\x1b[0m");
    }

    #[test]
    fn pipes_in_row_cells_round_trip() {
        // A literal `||` line inside a row cell must not split the cell
        // on re-parse. Unreachable via parse (it would have split), so
        // build the deck by hand.
        let deck = Deck {
            title: "t".to_string(),
            columns: vec![Column {
                slides: vec![Slide {
                    title: "s".to_string(),
                    segments: vec![Segment::Row(vec![
                        vec![Segment::Markdown("left".to_string())],
                        vec![Segment::Markdown("above\n||\nbelow".to_string())],
                    ])],
                    notes: String::new(),
                    theme: None,
                }],
            }],
        };
        let (md, rewrites) = compile(&deck).unwrap();
        assert_eq!(rewrites, 1);
        let reparsed = deck::parse(&md, "t").unwrap();
        match &reparsed.slide(0, 0).segments[0] {
            Segment::Row(cells) => {
                assert_eq!(cells.len(), 2, "cell split leaked: {md}");
                match &cells[1][0] {
                    Segment::Markdown(src) => {
                        assert!(src.contains("\\||"), "pipes not escaped: {src:?}")
                    }
                    other => panic!("expected markdown cell, got {other:?}"),
                }
            }
            other => panic!("expected row, got {other:?}"),
        }
    }

    #[test]
    fn blank_slides_survive_round_trip() {
        // A manifest can produce a slide with no content at all — an
        // intentional blank. Compiling must not silently drop it.
        let dir = scratch("compile-blank");
        std::fs::write(dir.join("a.md"), "# alpha\n").unwrap();
        std::fs::write(dir.join("blank.md"), "\n").unwrap();
        std::fs::write(
            dir.join("deck.json"),
            r#"{ "columns": [ "a.md", "blank.md" ] }"#,
        )
        .unwrap();

        let (original, _) = deck::load(&dir.join("deck.json")).unwrap();
        assert_eq!(original.columns.len(), 2);
        let (md, _) = compile(&original).unwrap();
        let reparsed = deck::parse(&md, "t").unwrap();
        assert_eq!(reparsed.columns.len(), 2, "blank slide dropped:\n{md}");
        // The marker itself renders as nothing.
        let text = crate::markdown::render(
            match &reparsed.slide(1, 0).segments[0] {
                Segment::Markdown(src) => src,
                other => panic!("expected markdown, got {other:?}"),
            },
            40,
            &crate::theme::Theme::default(),
        );
        assert!(
            text.lines
                .iter()
                .all(|l| l.spans.iter().all(|s| s.content.trim().is_empty())),
            "blank-slide marker rendered visibly: {text:?}"
        );
    }

    #[test]
    fn separator_lookalikes_rewritten() {
        let deck = Deck {
            title: "t".to_string(),
            columns: vec![Column {
                slides: vec![Slide {
                    title: "s".to_string(),
                    segments: vec![
                        Segment::Markdown("above\n\n---\n\nbelow\n```\n---\n```\n".to_string()),
                        Segment::Terminal(TermBlock {
                            id: 0,
                            command: None,
                            rows: 8,
                            fill: false,
                            snapshot: None,
                        }),
                    ],
                    notes: String::new(),
                    theme: None,
                }],
            }],
        };
        let (md, rewrites) = compile(&deck).unwrap();
        assert_eq!(rewrites, 1); // the fenced `---` is untouched
        let reparsed = deck::parse(&md, "t").unwrap();
        assert_eq!(reparsed.columns.len(), 1);
        assert_eq!(reparsed.columns[0].slides.len(), 1);
        assert!(md.contains("***"));
        match &reparsed.slide(0, 0).segments[1] {
            Segment::Terminal(b) => assert_eq!(b.rows, 8),
            _ => panic!("expected terminal preserved"),
        }
    }
}
