//! `deckhand compile`: flatten a deck — typically a JSON manifest with
//! per-slide files — into a single-file markdown deck that presents
//! identically (columns as `---`, depth as `--`, notes as `???`,
//! terminals as `terminal` fences).
//!
//! The deck title and deck-level theme are preserved as frontmatter, and
//! per-slide themes as `theme` fences. Slide `title` overrides have no
//! single-file syntax and fall back to what the content implies. Bare
//! `---`/`--` lines inside slide bodies would split slides on re-parse,
//! so they're rewritten to `***` (with a warning).

use anyhow::{Context, Result};

use crate::deck::{self, Deck, Segment};

/// CLI entry point; native-only because it can load remote decks.
#[cfg(not(target_arch = "wasm32"))]
pub fn run(input: &str, output: Option<&std::path::Path>) -> Result<()> {
    use crate::source;

    let source::Loaded {
        deck,
        theme: deck_theme,
        ..
    } = source::load(input)?;

    let (body, rewrites) = compile(&deck)?;
    if rewrites > 0 {
        eprintln!(
            "warning: rewrote {rewrites} bare `---`/`--` line(s) inside slide content to `***` \
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
    Ok((out, rewrites))
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
        run(dir.join("deck.json").to_str().unwrap(), Some(&out)).unwrap();

        let (deck, theme) = deck::load(&out).unwrap();
        assert_eq!(deck.title, "fancy talk");
        let theme = theme.unwrap();
        assert_eq!(theme.border_type.as_deref(), Some("rounded"));
        assert_eq!(deck.columns.len(), 1);
        assert_eq!(deck.slide(0, 0).title, "alpha");
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
