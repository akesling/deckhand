//! `deckhand compile`: flatten a deck — typically a JSON manifest with
//! per-slide files — into a single-file markdown deck that presents
//! identically (columns as `---`, depth as `--`, notes as `???`, terminals
//! as ```terminal fences).
//!
//! Not everything survives the trip: the single-file format has no theme
//! syntax, so deck- and slide-level themes are dropped (with a warning),
//! and manifest `title` overrides fall back to whatever the slide content
//! implies. Bare `---`/`--` lines inside slide bodies would split slides
//! on re-parse, so they're rewritten to `***` (also warned).

use std::path::Path;

use anyhow::{Context, Result};

use crate::deck::{self, Deck, Segment};
use crate::source;

pub fn run(input: &str, output: Option<&Path>) -> Result<()> {
    let source::Loaded {
        deck,
        theme: deck_theme,
        ..
    } = source::load(input)?;

    if deck_theme.is_some() {
        eprintln!("warning: deck-level theme dropped (no theme syntax in single-file decks)");
    }
    let themed: Vec<String> = deck
        .columns
        .iter()
        .enumerate()
        .flat_map(|(c, col)| {
            col.slides
                .iter()
                .enumerate()
                .filter(|(_, s)| s.theme.is_some())
                .map(move |(r, _)| format!("{}.{}", c + 1, r + 1))
        })
        .collect();
    if !themed.is_empty() {
        eprintln!(
            "warning: per-slide themes dropped on: {}",
            themed.join(", ")
        );
    }

    let (md, rewrites) = compile(&deck);
    if rewrites > 0 {
        eprintln!(
            "warning: rewrote {rewrites} bare `---`/`--` line(s) inside slide content to `***` \
             so they don't split slides on re-parse"
        );
    }

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
pub fn compile(deck: &Deck) -> (String, usize) {
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
            for seg in &slide.segments {
                match seg {
                    Segment::Markdown(src) => {
                        out.push_str(&sanitize(src.trim_matches('\n'), &mut rewrites));
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
                        out.push_str("```\n\n");
                    }
                }
            }
            if !slide.notes.is_empty() {
                out.push_str("???\n\n");
                out.push_str(&sanitize(slide.notes.trim(), &mut rewrites));
                out.push_str("\n\n");
            }
        }
    }
    (out, rewrites)
}

/// Rewrite lines that the single-file parser would read as slide
/// separators (bare runs of `-`, outside code fences) into `***`, which
/// renders as the same horizontal rule without splitting the deck.
fn sanitize(src: &str, rewrites: &mut usize) -> String {
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
        if t.len() >= 2 && t.chars().all(|c| c == '-') {
            *rewrites += 1;
            out.push("***".to_string());
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
        let (md, rewrites) = compile(&original);
        assert_eq!(rewrites, 0);

        let reparsed = deck::parse(&md, "t");
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
                        }),
                    ],
                    notes: String::new(),
                    theme: None,
                }],
            }],
        };
        let (md, rewrites) = compile(&deck);
        assert_eq!(rewrites, 1); // the fenced `---` is untouched
        let reparsed = deck::parse(&md, "t");
        assert_eq!(reparsed.columns.len(), 1);
        assert_eq!(reparsed.columns[0].slides.len(), 1);
        assert!(md.contains("***"));
        match &reparsed.slide(0, 0).segments[1] {
            Segment::Terminal(b) => assert_eq!(b.rows, 8),
            _ => panic!("expected terminal preserved"),
        }
    }
}
