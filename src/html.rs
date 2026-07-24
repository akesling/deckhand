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

use anyhow::Result;
use ratatui::buffer::Buffer;
use unicode_width::UnicodeWidthStr;

use crate::palette;
use crate::presenter::{Presenter, TerminalProvider};

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
            cols: crate::export::DEFAULT_COLS,
            rows: crate::export::DEFAULT_ROWS,
            snapshots: None,
            snapshot_opts: crate::snapshot::Options::default(),
        }
    }
}

/// CLI entry point: load `input` (file, URL, or gist), render every
/// slide, write a standalone HTML page to `output` (default: the
/// deck's file name with `.html`, in the current directory).
pub fn run(input: &str, output: Option<&Path>, opts: &Options) -> Result<()> {
    let loaded = crate::export::load(input, opts.snapshots, &opts.snapshot_opts)?;
    let mut presenter = crate::export::presenter(loaded)?;
    let html = render(&mut presenter, opts);
    let slides = presenter.deck().flat().len();
    crate::export::write(
        html.as_bytes(),
        output,
        input,
        "html",
        &format!("{slides} slides"),
    )
}

/// Render every slide of an already-built presenter to a standalone
/// HTML document.
pub fn render<P: TerminalProvider>(presenter: &mut Presenter<P>, opts: &Options) -> String {
    let crate::export::Pages {
        cols,
        rows,
        slides: pages,
    } = crate::export::render_pages(presenter, opts.cols, opts.rows);

    let mut styles = Styles::default();
    let mut slides = String::new();
    for ((c, r), buf) in &pages {
        let _ = write!(slides, "<pre class=\"slide\" id=\"s{c}-{r}\">");
        emit_slide(buf, cols, rows, &mut styles, &mut slides);
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
    use crate::replay::SnapshotProvider;

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

    /// The inline script's navigation, transcribed line for line. The
    /// parity test below pins the exact JS source this mirrors — edit
    /// one and the other must follow.
    struct JsNav {
        cols: Vec<usize>,
        col: usize,
        row: usize,
        mem: Vec<usize>,
    }

    impl JsNav {
        fn new(cols: Vec<usize>) -> Self {
            let mem = vec![0; cols.len()];
            JsNav {
                cols,
                col: 0,
                row: 0,
                mem,
            }
        }

        fn goto_(&mut self, c: isize, r: isize) {
            self.col = c.clamp(0, self.cols.len() as isize - 1) as usize;
            self.row = r.clamp(0, self.cols[self.col] as isize - 1) as usize;
            self.mem[self.col] = self.row;
        }

        fn key(&mut self, key: &str) {
            let (col, row) = (self.col as isize, self.row as isize);
            match key {
                "ArrowLeft" | "h" => {
                    if col > 0 {
                        self.goto_(col - 1, self.mem[self.col - 1] as isize);
                    }
                }
                "ArrowRight" | "l" => {
                    if self.col + 1 < self.cols.len() {
                        self.goto_(col + 1, self.mem[self.col + 1] as isize);
                    }
                }
                "ArrowDown" | "j" => self.goto_(col, row + 1),
                "ArrowUp" | "k" => self.goto_(col, row - 1),
                " " | "n" => {
                    if self.row + 1 < self.cols[self.col] {
                        self.goto_(col, row + 1);
                    } else if self.col + 1 < self.cols.len() {
                        self.goto_(col + 1, 0);
                    }
                }
                "Backspace" | "p" => {
                    if self.row > 0 {
                        self.goto_(col, row - 1);
                    } else if self.col > 0 {
                        self.goto_(col - 1, self.cols[self.col - 1] as isize - 1);
                    }
                }
                "g" => self.goto_(0, 0),
                "G" => self.goto_(self.cols.len() as isize - 1, 0),
                _ => {}
            }
        }
    }

    #[test]
    fn inline_nav_matches_the_presenter() {
        use crate::presenter::{Key, KeyPress};

        let src = "# a\n---\n# b\n--\n## b2\n--\n### b3\n---\n# c\n--\n## c2\n";
        let deck = deck::parse(src, "t").unwrap();
        let mut p = Presenter::new(deck, vec![], SnapshotProvider::default()).unwrap();
        let mut js = JsNav::new(vec![1, 3, 2]);

        let tui_key = |k: &str| match k {
            "ArrowLeft" => Key::Left,
            "ArrowRight" => Key::Right,
            "ArrowDown" => Key::Down,
            "ArrowUp" => Key::Up,
            "Backspace" => Key::Backspace,
            other => Key::Char(other.chars().next().unwrap()),
        };
        // Every bound key, exercised across clamps, depth memory, and
        // the depth-first walk — including a full space-walk to the end.
        let script = [
            "l",
            "j",
            "j",
            " ",
            "h",
            "k",
            "G",
            "p",
            "g",
            "ArrowRight",
            "ArrowDown",
            "ArrowUp",
            "l",
            "j",
            "Backspace",
            "h",
            "n",
            "n",
            "n",
            "n",
            "n",
            "n",
            "n",
            " ",
            "p",
            "G",
            "j",
            "k",
            "g",
            "h",
        ];
        for k in script {
            js.key(k);
            p.on_key(KeyPress::plain(tui_key(k)));
            assert_eq!(
                p.position(),
                (js.col, js.row),
                "presenter and inline script diverged after {k:?}"
            );
        }

        // Pin the JS lines JsNav transcribes; a nav edit must fail here.
        let html = html_for(src);
        for line in [
            r#"function goto_(c,r){"#,
            r#"col=Math.max(0,Math.min(c,COLS.length-1));"#,
            r#"row=Math.max(0,Math.min(r,COLS[col]-1));"#,
            r#"mem[col]=row;show();"#,
            r#"function next(){if(row+1<COLS[col])goto_(col,row+1);else if(col+1<COLS.length)goto_(col+1,0)}"#,
            r#"function prev(){if(row>0)goto_(col,row-1);else if(col>0)goto_(col-1,COLS[col-1]-1)}"#,
            r#"case"ArrowLeft":case"h":if(col>0)goto_(col-1,mem[col-1]);break;"#,
            r#"case"ArrowRight":case"l":if(col+1<COLS.length)goto_(col+1,mem[col+1]);break;"#,
            r#"case"ArrowDown":case"j":goto_(col,row+1);break;"#,
            r#"case"ArrowUp":case"k":goto_(col,row-1);break;"#,
            r#"case" ":case"n":next();break;"#,
            r#"case"Backspace":case"p":prev();break;"#,
            r#"case"g":goto_(0,0);break;"#,
            r#"case"G":goto_(COLS.length-1,0);break;"#,
        ] {
            assert!(
                html.contains(line),
                "nav script changed — update JsNav to match: missing {line:?}"
            );
        }
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
