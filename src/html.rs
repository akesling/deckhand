//! `deckhand compile -o talk.html`: export a deck as a single
//! self-contained static HTML page.
//!
//! Every slide is drawn by the shared [`crate::presenter::Presenter`]
//! into an offscreen cell grid — the exact layout, theming, and chrome
//! the TUI shows — and each grid becomes a `<pre>` of styled spans.
//! One file, no external assets, no wasm: colors resolve through
//! [`crate::palette`] (matching the web presenter), a few lines of
//! inline script give deckhand's 2-D navigation (arrows / hjkl /
//! space, with a `#col.row` hash), text scales to fit the window, and
//! a print stylesheet lays slides out one per page.
//!
//! Terminal blocks can't run in a static page: baked snapshots replay
//! via [`crate::replay::SnapshotProvider`]; decks with unsnapshotted
//! terminals require the same `--snapshots` / `--no-snapshots` choice
//! as every other compile format.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use unicode_width::UnicodeWidthStr;

use crate::palette;
use crate::presenter::{Presenter, TerminalProvider};
use crate::replay::SnapshotProvider;
use crate::{source, theme};

/// Settings for [`run`].
pub struct Options {
    /// Slide grid width, in terminal columns.
    pub cols: u16,
    /// Slide grid height, in terminal rows (the status bar isn't
    /// drawn).
    pub rows: u16,
    /// Terminal blocks without snapshots: `Some(true)` captures now
    /// (running the deck's commands), `Some(false)` renders
    /// placeholders, `None` errors if any exist.
    pub snapshots: Option<bool>,
    /// Capture settings when `snapshots` is `Some(true)`.
    pub snapshot_opts: crate::snapshot::Options,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            cols: 100,
            rows: 30,
            snapshots: None,
            snapshot_opts: crate::snapshot::Options::default(),
        }
    }
}

/// CLI entry point: load `input` (file, URL, or gist), render every
/// slide, write a standalone HTML page to `output` (default: the
/// deck's file name with `.html`, in the current directory).
pub fn run(input: &str, output: Option<&Path>, opts: &Options) -> Result<()> {
    let source::Loaded {
        mut deck,
        theme: deck_theme,
        base_dir,
    } = source::load(input)?;

    let missing = deck.unsnapshotted_commands();
    if !missing.is_empty() {
        match opts.snapshots {
            Some(true) => {
                crate::snapshot::capture(&mut deck, &base_dir, &opts.snapshot_opts)
                    .context("capturing terminal snapshots")?;
            }
            Some(false) => {}
            None => anyhow::bail!(
                "this deck has {} terminal block(s) without baked snapshots: {}\n\
                 capturing snapshots runs those commands in PTYs on this machine.\n\
                 pass --snapshots to capture their output (only for decks you trust),\n\
                 or --no-snapshots to render placeholders",
                missing.len(),
                missing.join(", "),
            ),
        }
    }
    deck.resolve_images(&base_dir);

    let cfgs: Vec<theme::ThemeConfig> = [theme::user_config()?, deck_theme]
        .into_iter()
        .flatten()
        .collect();
    let mut presenter = Presenter::new(deck, cfgs, SnapshotProvider::default())?;

    let html = render(&mut presenter, opts);
    let path = match output {
        Some(p) => p.to_path_buf(),
        None => source::output_name(input, "html"),
    };
    std::fs::write(&path, &html).with_context(|| format!("writing {}", path.display()))?;
    eprintln!(
        "wrote {} ({} slides)",
        path.display(),
        presenter.deck().flat().len()
    );
    Ok(())
}

/// Render every slide of an already-built presenter to a standalone
/// HTML document.
pub fn render<P: TerminalProvider>(presenter: &mut Presenter<P>, opts: &Options) -> String {
    let cols = opts.cols.max(20);
    let rows = opts.rows.max(4);
    // One extra row for the status bar the presenter always reserves;
    // it's never emitted.
    let area = Rect::new(0, 0, cols, rows + 1);

    let order = presenter.deck().flat();
    let mut styles = Styles::default();
    let mut slides = String::new();
    for &(c, r) in &order {
        presenter.goto(c, r);
        let mut buf = Buffer::empty(area);
        presenter.draw(area, &mut buf);
        let _ = write!(slides, "<pre class=\"slide\" id=\"s{c}-{r}\">");
        emit_slide(&buf, cols, rows, &mut styles, &mut slides);
        slides.push_str("</pre>\n");
    }

    let column_lens: Vec<usize> = presenter
        .deck()
        .columns
        .iter()
        .map(|c| c.slides.len())
        .collect();
    document(
        &presenter.deck().title,
        &styles.css(),
        &slides,
        &column_lens,
        cols,
        rows,
    )
}

// ------------------------------------------------------------- styling

/// A span's full style, deduplicated into one CSS class per distinct
/// combination.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct StyleKey {
    fg: [u8; 3],
    bg: Option<[u8; 3]>,
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
}

impl StyleKey {
    /// The default style renders as bare text with no span at all.
    fn is_default(&self) -> bool {
        *self
            == StyleKey {
                fg: palette::DEFAULT_FG,
                bg: None,
                bold: false,
                italic: false,
                underline: false,
                strike: false,
            }
    }
}

/// Interns [`StyleKey`]s as `.s0`, `.s1`, … classes.
#[derive(Default)]
struct Styles {
    classes: BTreeMap<StyleKey, usize>,
}

impl Styles {
    fn class(&mut self, key: StyleKey) -> usize {
        let next = self.classes.len();
        *self.classes.entry(key).or_insert(next)
    }

    /// One rule per interned style.
    fn css(&self) -> String {
        let mut rules: Vec<(usize, &StyleKey)> =
            self.classes.iter().map(|(k, &i)| (i, k)).collect();
        rules.sort_unstable();
        let mut out = String::new();
        for (i, key) in rules {
            let _ = write!(out, ".s{i}{{color:{}", hex(key.fg));
            if let Some(bg) = key.bg {
                let _ = write!(out, ";background:{}", hex(bg));
            }
            if key.bold {
                out.push_str(";font-weight:bold");
            }
            if key.italic {
                out.push_str(";font-style:italic");
            }
            match (key.underline, key.strike) {
                (true, true) => out.push_str(";text-decoration:underline line-through"),
                (true, false) => out.push_str(";text-decoration:underline"),
                (false, true) => out.push_str(";text-decoration:line-through"),
                (false, false) => {}
            }
            out.push_str("}\n");
        }
        out
    }
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

// ------------------------------------------------------------ emission

/// Append `text` with HTML metacharacters escaped.
fn escape_into(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

/// Serialize one rendered slide: rows of same-style spans, default-
/// style text bare, trailing unstyled blanks trimmed.
fn emit_slide(buf: &Buffer, cols: u16, rows: u16, styles: &mut Styles, out: &mut String) {
    for y in 0..rows {
        let mut spans: Vec<(StyleKey, String)> = Vec::new();
        let mut x = 0u16;
        while x < cols {
            let cell = &buf[(x, y)];
            let symbol = cell.symbol();
            let width = (symbol.width().max(1) as u16).min(cols - x);
            let r = palette::resolve(cell.fg, cell.bg, cell.modifier);
            // A hidden cell keeps its background but shows no text.
            let symbol = if r.hidden { " " } else { symbol };
            let key = StyleKey {
                fg: r.fg,
                bg: r.bg,
                bold: r.bold,
                italic: r.italic,
                underline: r.underline,
                strike: r.strike,
            };
            match spans.last_mut() {
                Some((k, text)) if *k == key => text.push_str(symbol),
                _ => spans.push((key, symbol.to_string())),
            }
            x += width;
        }
        // Trailing unstyled whitespace only pads the file.
        if let Some((key, text)) = spans.last_mut()
            && key.is_default()
        {
            let trimmed = text.trim_end();
            if trimmed.is_empty() {
                spans.pop();
            } else {
                *text = trimmed.to_string();
            }
        }
        for (key, text) in spans {
            if key.is_default() {
                escape_into(out, &text);
            } else {
                let _ = write!(out, "<span class=\"s{}\">", styles.class(key));
                escape_into(out, &text);
                out.push_str("</span>");
            }
        }
        out.push('\n');
    }
}

// ------------------------------------------------------------ document

/// Wrap slides, styles, and the navigation script into a page.
fn document(
    title: &str,
    style_rules: &str,
    slides: &str,
    column_lens: &[usize],
    cols: u16,
    rows: u16,
) -> String {
    let mut escaped_title = String::new();
    escape_into(&mut escaped_title, title);
    let bg = hex(palette::DEFAULT_BG);
    let fg = hex(palette::DEFAULT_FG);
    let columns = column_lens
        .iter()
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="generator" content="deckhand {version}">
<title>{escaped_title}</title>
<style>
*{{margin:0;padding:0}}
html,body{{height:100%}}
body{{background:{bg};color:{fg};display:flex;align-items:center;justify-content:center;overflow:hidden}}
pre{{font-family:"DejaVu Sans Mono",ui-monospace,Menlo,Consolas,"Liberation Mono",monospace;font-size:var(--fs,16px);line-height:1.19}}
.slide{{display:none}}
.slide.active{{display:block}}
{style_rules}@media print{{
html,body{{height:auto}}
body{{display:block;overflow:visible;print-color-adjust:exact;-webkit-print-color-adjust:exact}}
.slide{{display:block;break-after:page;font-size:12px}}
}}
</style>
</head>
<body>
{slides}<script>
"use strict";
const COLS=[{columns}],GRID=[{cols},{rows}];
const slides=document.querySelectorAll(".slide");
const idx=(c,r)=>{{let i=r;for(let k=0;k<c;k++)i+=COLS[k];return i}};
let col=0,row=0;const mem=COLS.map(()=>0);
function show(){{
  slides.forEach(s=>s.classList.remove("active"));
  slides[idx(col,row)].classList.add("active");
  history.replaceState(null,"","#"+(col+1)+"."+(row+1));
}}
function goto_(c,r){{
  col=Math.max(0,Math.min(c,COLS.length-1));
  row=Math.max(0,Math.min(r,COLS[col]-1));
  mem[col]=row;show();
}}
function next(){{if(row+1<COLS[col])goto_(col,row+1);else if(col+1<COLS.length)goto_(col+1,0)}}
function prev(){{if(row>0)goto_(col,row-1);else if(col>0)goto_(col-1,COLS[col-1]-1)}}
addEventListener("keydown",e=>{{
  if(e.ctrlKey||e.metaKey||e.altKey)return;
  switch(e.key){{
    case"ArrowLeft":case"h":if(col>0)goto_(col-1,mem[col-1]);break;
    case"ArrowRight":case"l":if(col+1<COLS.length)goto_(col+1,mem[col+1]);break;
    case"ArrowDown":case"j":goto_(col,row+1);break;
    case"ArrowUp":case"k":goto_(col,row-1);break;
    case" ":case"n":next();break;
    case"Backspace":case"p":prev();break;
    case"g":goto_(0,0);break;
    case"G":goto_(COLS.length-1,0);break;
    case"f":document.fullscreenElement?document.exitFullscreen():document.documentElement.requestFullscreen();break;
    default:return;
  }}
  e.preventDefault();
}});
function fromHash(){{
  const m=location.hash.match(/^#(\d+)\.(\d+)$/);
  if(m)goto_(m[1]-1,m[2]-1);else show();
}}
addEventListener("hashchange",fromHash);
const probe=document.createElement("pre");
probe.style.cssText="position:absolute;visibility:hidden;font-size:100px";
probe.textContent="M";document.body.appendChild(probe);
function fit(){{
  const r=probe.getBoundingClientRect();
  const fs=Math.min(innerWidth/(GRID[0]*r.width/100),innerHeight/(GRID[1]*r.height/100));
  document.documentElement.style.setProperty("--fs",Math.max(4,Math.floor(fs))+"px");
}}
addEventListener("resize",fit);
fit();fromHash();
</script>
</body>
</html>
"##,
        version = env!("CARGO_PKG_VERSION"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck;

    fn html_for(src: &str) -> String {
        let deck = deck::parse(src, "test deck").unwrap();
        let mut presenter = Presenter::new(deck, vec![], SnapshotProvider::default()).unwrap();
        render(&mut presenter, &Options::default())
    }

    #[test]
    fn renders_a_standalone_page() {
        let html = html_for("# alpha\nhello *world*\n---\n# beta\n- one\n--\n## deeper\n");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<title>test deck</title>"));
        // Three slides, wired for 2-D nav: two columns of 1 and 2.
        assert_eq!(html.matches("<pre class=\"slide\"").count(), 3);
        assert!(html.contains("const COLS=[1,2]"));
        // No external references — the page is self-contained.
        assert!(!html.contains("src=") && !html.contains("href="));
    }

    #[test]
    fn styles_are_interned_and_escaped() {
        let html = html_for("# t\n`a < b & c`\n*same style*\n\n*same style*\n");
        // Escapes.
        assert!(html.contains("a &lt; b &amp; c"));
        // The two italic lines share one class; the rule exists once.
        let class: Vec<&str> = html.matches("font-style:italic").collect();
        assert_eq!(class.len(), 1, "italic rule not interned:\n{html}");
    }

    #[test]
    fn slide_grid_trims_trailing_blanks() {
        let html = html_for("# t\nhi\n");
        for line in html.lines() {
            assert!(
                !line.ends_with(' ') || line.contains("</span>"),
                "unstyled trailing spaces survived: {line:?}"
            );
        }
    }

    #[test]
    fn terminal_decks_require_a_snapshot_choice() {
        let dir = std::env::temp_dir().join(format!("deckhand-html-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deck.md");
        std::fs::write(&path, "# a\n```terminal\nhtop\n```\n").unwrap();
        let out = dir.join("deck.html");

        let err = run(path.to_str().unwrap(), Some(&out), &Options::default()).unwrap_err();
        assert!(format!("{err:#}").contains("htop"));

        let opts = Options {
            snapshots: Some(false),
            ..Options::default()
        };
        run(path.to_str().unwrap(), Some(&out), &opts).unwrap();
        let html = std::fs::read_to_string(&out).unwrap();
        assert!(html.contains("interactive terminal"));
    }
}
