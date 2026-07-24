//! `deckhand compile -o talk.pdf`: render a deck to a PDF file, one
//! page per slide.
//!
//! Every slide is drawn by the shared [`crate::presenter::Presenter`]
//! into an offscreen cell grid — the exact layout, theming, and chrome
//! the TUI shows — and each grid is then typeset onto a PDF page:
//!
//! - text becomes real, extractable text in an embedded, subsetted
//!   DejaVu Sans Mono;
//! - box-drawing borders, block glyphs (banner headings, QR codes),
//!   and bullets are painted as vector paths so borders connect
//!   seamlessly and QR codes stay scannable at any zoom;
//! - colors resolve against the same palette the web presenter shows
//!   (xterm defaults on the site's dark surface);
//! - slide titles become PDF outline bookmarks, one top-level entry
//!   per column with its deeper slides nested beneath.
//!
//! Terminal blocks can't run in a PDF: blocks with baked snapshots
//! (from `deckhand compile --snapshots`) render their captured screen
//! via [`crate::replay::SnapshotProvider`]; decks with unsnapshotted
//! terminals require choosing `--snapshots` (captures now, running the
//! deck's commands) or `--no-snapshots` (placeholders) — the same
//! consent rule as every other compile format.

mod font;
mod glyphs;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::Result;
use ratatui::buffer::Buffer;
use unicode_width::UnicodeWidthStr;

use crate::deck::Deck;
use crate::palette::{self, Resolved};
use crate::presenter::{Presenter, TerminalProvider};
use font::{FaceId, Fonts};

/// Settings for [`run`].
pub struct Options {
    /// Page grid width, in terminal columns.
    pub cols: u16,
    /// Page grid height, in terminal rows (the status bar isn't drawn).
    pub rows: u16,
    /// Font size in points; with the grid, this sets the page size.
    /// Clamped to 4–96 when rendering (non-finite values fall back to
    /// the default) — a zero or NaN size would emit a broken page box.
    pub font_size: f32,
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
            font_size: crate::export::DEFAULT_FONT_SIZE,
            snapshots: None,
            snapshot_opts: crate::snapshot::Options::default(),
        }
    }
}

/// CLI entry point: load `input` (file, URL, or gist), render every
/// slide, write a PDF to `output` (default: the deck's file name with
/// `.pdf`, in the current directory).
pub fn run(input: &str, output: Option<&Path>, opts: &Options) -> Result<()> {
    let loaded = crate::export::load(input, opts.snapshots, &opts.snapshot_opts)?;
    let mut presenter = crate::export::presenter(loaded)?;
    let bytes = render(&mut presenter, opts)?;
    let pages = presenter.deck().flat().len();
    crate::export::write(&bytes, output, input, "pdf", &format!("{pages} pages"))
}

/// Render every slide of an already-built presenter to PDF bytes.
pub fn render<P: TerminalProvider>(
    presenter: &mut Presenter<P>,
    opts: &Options,
) -> Result<Vec<u8>> {
    let font_size = if opts.font_size.is_finite() {
        opts.font_size.clamp(4.0, 96.0)
    } else {
        crate::export::DEFAULT_FONT_SIZE
    };
    let crate::export::Pages { cols, rows, slides } =
        crate::export::render_pages(presenter, opts.cols, opts.rows);
    let pages: Vec<Buffer> = slides.into_iter().map(|(_, buf)| buf).collect();

    let mut fonts = Fonts::new()?;
    // Pass 1: walk every page to learn which glyphs the deck uses.
    for buf in &pages {
        emit_page(buf, cols, rows, font_size, &mut fonts, None);
    }
    let subsets = Subsets::build(&fonts)?;
    // Pass 2: emit the real content streams with subset glyph ids.
    let contents: Vec<String> = pages
        .iter()
        .map(|buf| emit_page(buf, cols, rows, font_size, &mut fonts, Some(&subsets)))
        .collect();

    let order = presenter.deck().flat();
    let outline = outline_nodes(presenter.deck(), &order);
    assemble(
        &contents,
        &fonts,
        &subsets,
        &outline,
        &presenter.deck().title,
        cols,
        rows,
        font_size,
    )
}

// ----------------------------------------------------------- emission

/// One face's subset font program and its old → new glyph-id map.
type Subset = (Vec<u8>, BTreeMap<u16, u16>);

/// Subset font programs and their glyph-id remappings.
struct Subsets {
    regular: Option<Subset>,
    bold: Option<Subset>,
}

impl Subsets {
    fn build(fonts: &Fonts) -> Result<Subsets> {
        let sub = |face: &font::Face| -> Result<Option<Subset>> {
            if face.is_used() {
                Ok(Some(face.subset()?))
            } else {
                Ok(None)
            }
        };
        Ok(Subsets {
            regular: sub(&fonts.regular)?,
            bold: sub(&fonts.bold)?,
        })
    }

    fn remap(&self, face: FaceId, gid: u16) -> u16 {
        let map = match face {
            FaceId::Regular => &self.regular,
            FaceId::Bold => &self.bold,
        };
        map.as_ref()
            .and_then(|(_, m)| m.get(&gid).copied())
            .unwrap_or(gid)
    }
}

/// Italic slant, as terminals synthesize it: tan(~12°).
const ITALIC_SKEW: f32 = 0.213;

/// Write `0.xxx 0.xxx 0.xxx` for an RGB color.
fn push_rgb(out: &mut String, c: [u8; 3]) {
    let _ = write!(
        out,
        "{:.3} {:.3} {:.3}",
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0
    );
}

/// A pending same-style run of text glyphs on one row.
struct Run {
    face: FaceId,
    color: [u8; 3],
    italic: bool,
    col: u16,
    gids: Vec<u16>,
}

/// Typeset one rendered slide as a PDF content stream. With
/// `subsets == None` this is the glyph-collection pass and the output
/// is discarded; the two passes must walk identically.
fn emit_page(
    buf: &Buffer,
    cols: u16,
    rows: u16,
    size: f32,
    fonts: &mut Fonts,
    subsets: Option<&Subsets>,
) -> String {
    let m = fonts.metrics;
    let cw = m.cell_w(size);
    let ch = m.cell_h(size);
    let page_w = cols as f32 * cw;
    let page_h = rows as f32 * ch;

    let mut bgs = String::new();
    let mut paths = String::new();
    let mut text = String::new();
    let mut deco = String::new();

    // The page itself.
    push_rgb(&mut bgs, palette::DEFAULT_BG);
    let _ = writeln!(bgs, " rg 0 0 {page_w:.2} {page_h:.2} re f");

    let mut path_color: Option<[u8; 3]> = None;
    let mut text_color: Option<[u8; 3]> = None;
    let mut text_font: Option<FaceId> = None;

    for y in 0..rows {
        let top = page_h - y as f32 * ch;
        let baseline = top - m.baseline(size);

        // Resolve the row once; every later pass walks this.
        let mut cells: Vec<(u16, u16, char, Resolved)> = Vec::new();
        let mut x = 0u16;
        while x < cols {
            let cell = &buf[(x, y)];
            let symbol = cell.symbol();
            // A multi-char symbol is a grapheme cluster; the base char
            // is the best a font-shaping-free typesetter can do.
            let chr = symbol.chars().next().unwrap_or(' ');
            let width = (symbol.width().max(1) as u16).min(cols - x);
            cells.push((
                x,
                width,
                chr,
                palette::resolve(cell.fg, cell.bg, cell.modifier),
            ));
            x += width;
        }

        // Backgrounds, merged into horizontal runs.
        let mut bg_run: Option<(u16, u16, [u8; 3])> = None;
        for (x, width, _, r) in &cells {
            let flush = |run: Option<(u16, u16, [u8; 3])>, out: &mut String| {
                if let Some((start, len, color)) = run {
                    push_rgb(out, color);
                    let _ = writeln!(
                        out,
                        " rg {:.2} {:.2} {:.2} {:.2} re f",
                        start as f32 * cw,
                        top - ch,
                        len as f32 * cw,
                        ch
                    );
                }
            };
            match (r.bg, &mut bg_run) {
                (Some(c), Some((start, len, run_c))) if *run_c == c && *start + *len == *x => {
                    *len += width;
                }
                (Some(c), run) => {
                    flush(run.take(), &mut bgs);
                    *run = Some((*x, *width, c));
                }
                (None, run @ Some(_)) => flush(run.take(), &mut bgs),
                (None, None) => {}
            }
        }
        if let Some((start, len, color)) = bg_run {
            push_rgb(&mut bgs, color);
            let _ = writeln!(
                &mut bgs,
                " rg {:.2} {:.2} {:.2} {:.2} re f",
                start as f32 * cw,
                top - ch,
                len as f32 * cw,
                ch
            );
        }

        // Glyphs: vector paths for geometry, text runs for the rest.
        let mut run: Option<Run> = None;
        let mut flush_run = |run: &mut Option<Run>, text: &mut String| {
            let Some(r) = run.take() else { return };
            if r.gids.is_empty() {
                return;
            }
            if text_color != Some(r.color) {
                push_rgb(text, r.color);
                text.push_str(" rg\n");
                text_color = Some(r.color);
            }
            if text_font != Some(r.face) {
                let name = match r.face {
                    FaceId::Regular => "F0",
                    FaceId::Bold => "F1",
                };
                let _ = writeln!(text, "/{name} {size} Tf");
                text_font = Some(r.face);
            }
            let skew = if r.italic { ITALIC_SKEW } else { 0.0 };
            let _ = write!(
                text,
                "1 0 {skew:.3} 1 {:.2} {baseline:.2} Tm <",
                r.col as f32 * cw
            );
            for gid in &r.gids {
                let gid = match subsets {
                    Some(s) => s.remap(r.face, *gid),
                    None => *gid,
                };
                let _ = write!(text, "{gid:04X}");
            }
            text.push_str("> Tj\n");
        };

        for (x, width, chr, r) in &cells {
            if r.hidden || *chr == ' ' {
                flush_run(&mut run, &mut text);
                continue;
            }
            let vector_color = glyphs::shade(*chr)
                .map(|level| palette::blend(r.bg.unwrap_or(palette::DEFAULT_BG), r.fg, level))
                .unwrap_or(r.fg);
            // Try the vector set first; anything else is a font glyph.
            let mut probe = String::new();
            if glyphs::draw(
                *chr,
                *x as f32 * cw,
                top - ch,
                *width as f32 * cw,
                ch,
                &mut probe,
            ) {
                flush_run(&mut run, &mut text);
                if path_color != Some(vector_color) {
                    push_rgb(&mut paths, vector_color);
                    paths.push_str(" rg ");
                    push_rgb(&mut paths, vector_color);
                    paths.push_str(" RG\n");
                    path_color = Some(vector_color);
                }
                paths.push_str(&probe);
                continue;
            }
            let face = if r.bold {
                FaceId::Bold
            } else {
                FaceId::Regular
            };
            let Some(gid) = fonts.face_mut(face).glyph(*chr) else {
                // No coverage in the embedded font: an empty cell is
                // less misleading than a wrong glyph.
                flush_run(&mut run, &mut text);
                continue;
            };
            let standard = *width == 1 && fonts.face_mut(face).standard_advance(gid, &m);
            let extends = matches!(
                &run,
                Some(p) if p.face == face
                    && p.color == r.fg
                    && p.italic == r.italic
                    && p.col + p.gids.len() as u16 == *x
            );
            if standard && extends {
                run.as_mut().expect("checked above").gids.push(gid);
            } else {
                flush_run(&mut run, &mut text);
                run = Some(Run {
                    face,
                    color: r.fg,
                    italic: r.italic,
                    col: *x,
                    gids: vec![gid],
                });
                if !standard {
                    // Position it alone; the next glyph re-anchors.
                    flush_run(&mut run, &mut text);
                }
            }
        }
        flush_run(&mut run, &mut text);

        // Underlines and strikethroughs, merged like backgrounds.
        for (kind_y, pick) in [
            (
                baseline + m.to_pdf_units(m.under_pos) / 1000.0 * size,
                Box::new(|r: &Resolved| r.underline) as Box<dyn Fn(&Resolved) -> bool>,
            ),
            (
                baseline + 0.25 * size,
                Box::new(|r: &Resolved| r.strike) as Box<dyn Fn(&Resolved) -> bool>,
            ),
        ] {
            let thick = (m.to_pdf_units(m.under_thick) / 1000.0 * size).max(0.4);
            let mut line: Option<(u16, u16, [u8; 3])> = None;
            for (x, width, _, r) in &cells {
                let want = pick(r).then_some(r.fg);
                match (want, &mut line) {
                    (Some(c), Some((start, len, run_c))) if *run_c == c && *start + *len == *x => {
                        *len += width;
                    }
                    (want, line_slot) => {
                        if let Some((start, len, color)) = line_slot.take() {
                            push_rgb(&mut deco, color);
                            let _ = writeln!(
                                deco,
                                " rg {:.2} {:.2} {:.2} {thick:.2} re f",
                                start as f32 * cw,
                                kind_y - thick / 2.0,
                                len as f32 * cw
                            );
                        }
                        *line_slot = want.map(|c| (*x, *width, c));
                    }
                }
            }
            if let Some((start, len, color)) = line {
                push_rgb(&mut deco, color);
                let _ = writeln!(
                    deco,
                    " rg {:.2} {:.2} {:.2} {thick:.2} re f",
                    start as f32 * cw,
                    kind_y - thick / 2.0,
                    len as f32 * cw
                );
            }
        }
    }

    format!("q\n{bgs}{paths}BT\n{text}ET\n{deco}Q\n")
}

// ----------------------------------------------------------- assembly

/// One outline entry: a column's first slide, with deeper slides
/// nested beneath it.
struct Node {
    title: String,
    page: usize,
    children: Vec<(String, usize)>,
}

fn outline_nodes(deck: &Deck, order: &[(usize, usize)]) -> Vec<Node> {
    let page_of = |c: usize, r: usize| {
        order
            .iter()
            .position(|&p| p == (c, r))
            .expect("flat() covers every slide")
    };
    deck.columns
        .iter()
        .enumerate()
        .map(|(c, column)| Node {
            title: column.slides[0].title.clone(),
            page: page_of(c, 0),
            children: column
                .slides
                .iter()
                .enumerate()
                .skip(1)
                .map(|(r, s)| (s.title.clone(), page_of(c, r)))
                .collect(),
        })
        .collect()
}

fn deflate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 7)
}

/// Build the PDF file: pages, compressed content streams, embedded
/// subset fonts, document outline, and info dictionary.
#[allow(clippy::too_many_arguments)]
fn assemble(
    contents: &[String],
    fonts: &Fonts,
    subsets: &Subsets,
    outline: &[Node],
    title: &str,
    cols: u16,
    rows: u16,
    size: f32,
) -> Result<Vec<u8>> {
    use pdf_writer::types::{CidFontType, FontFlags, SystemInfo};
    use pdf_writer::{Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

    let m = fonts.metrics;
    let page_w = cols as f32 * m.cell_w(size);
    let page_h = rows as f32 * m.cell_h(size);

    let mut next = 1;
    let mut alloc = || {
        let r = Ref::new(next);
        next += 1;
        r
    };

    let catalog_id = alloc();
    let tree_id = alloc();
    let info_id = alloc();
    let outline_id = alloc();
    let page_ids: Vec<Ref> = contents.iter().map(|_| alloc()).collect();
    let content_ids: Vec<Ref> = contents.iter().map(|_| alloc()).collect();

    // (resource name, type0, cid, descriptor, file, tounicode, subset)
    let mut faces = Vec::new();
    for (face_id, name, face, subset) in [
        (FaceId::Regular, "F0", &fonts.regular, &subsets.regular),
        (FaceId::Bold, "F1", &fonts.bold, &subsets.bold),
    ] {
        if let Some((data, remap)) = subset {
            faces.push((
                face_id,
                name,
                face,
                data,
                remap,
                alloc(),
                alloc(),
                alloc(),
                alloc(),
                alloc(),
            ));
        }
    }

    let outline_ids: Vec<(Ref, Vec<Ref>)> = outline
        .iter()
        .map(|n| (alloc(), n.children.iter().map(|_| alloc()).collect()))
        .collect();

    let mut pdf = Pdf::new();
    pdf.catalog(catalog_id).pages(tree_id).outlines(outline_id);
    pdf.pages(tree_id)
        .kids(page_ids.iter().copied())
        .count(page_ids.len() as i32);
    pdf.document_info(info_id)
        .title(TextStr(title))
        .producer(TextStr(concat!("deckhand ", env!("CARGO_PKG_VERSION"))));

    for ((page_id, content_id), content) in page_ids.iter().zip(&content_ids).zip(contents) {
        let mut page = pdf.page(*page_id);
        page.media_box(Rect::new(0.0, 0.0, page_w, page_h));
        page.parent(tree_id);
        page.contents(*content_id);
        let mut resources = page.resources();
        let mut res_fonts = resources.fonts();
        for (_, name, .., type0_id, _, _, _, _) in &faces {
            res_fonts.pair(Name(name.as_bytes()), *type0_id);
        }
        res_fonts.finish();
        resources.finish();
        page.finish();
        pdf.stream(*content_id, &deflate(content.as_bytes()))
            .filter(Filter::FlateDecode);
    }

    for (face_id, _, face, data, remap, type0_id, cid_id, desc_id, file_id, uni_id) in &faces {
        let base = match face_id {
            FaceId::Regular => "DECKHD+DejaVuSansMono",
            FaceId::Bold => "DECKHD+DejaVuSansMono-Bold",
        };
        let base = Name(base.as_bytes());
        pdf.type0_font(*type0_id)
            .base_font(base)
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(*cid_id)
            .to_unicode(*uni_id);
        let mut cid = pdf.cid_font(*cid_id);
        cid.subtype(CidFontType::Type2);
        cid.base_font(base);
        cid.system_info(SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"Identity"),
            supplement: 0,
        });
        cid.font_descriptor(*desc_id);
        cid.default_width(m.to_pdf_units(m.advance));
        cid.cid_to_gid_map_predefined(Name(b"Identity"));
        cid.finish();
        let d = face.descriptor(&m);
        let mut desc = pdf.font_descriptor(*desc_id);
        desc.name(base);
        desc.flags(FontFlags::FIXED_PITCH | FontFlags::NON_SYMBOLIC);
        desc.bbox(Rect::new(d.bbox[0], d.bbox[1], d.bbox[2], d.bbox[3]));
        desc.italic_angle(0.0);
        desc.ascent(d.ascent);
        desc.descent(d.descent);
        desc.cap_height(d.cap_height);
        desc.stem_v(90.0);
        desc.font_file2(*file_id);
        desc.finish();
        pdf.stream(*file_id, &deflate(data))
            .filter(Filter::FlateDecode)
            .pair(Name(b"Length1"), data.len() as i32);
        pdf.stream(*uni_id, &deflate(face.to_unicode(remap).as_bytes()))
            .filter(Filter::FlateDecode);
    }

    if !outline_ids.is_empty() {
        let open: i32 = outline_ids
            .iter()
            .map(|(_, kids)| 1 + kids.len() as i32)
            .sum();
        pdf.outline(outline_id)
            .first(outline_ids[0].0)
            .last(outline_ids[outline_ids.len() - 1].0)
            .count(open);
        for (i, ((item_id, kid_ids), node)) in outline_ids.iter().zip(outline).enumerate() {
            let mut item = pdf.outline_item(*item_id);
            item.title(TextStr(&node.title));
            item.parent(outline_id);
            if i > 0 {
                item.prev(outline_ids[i - 1].0);
            }
            if i + 1 < outline_ids.len() {
                item.next(outline_ids[i + 1].0);
            }
            if !kid_ids.is_empty() {
                item.first(kid_ids[0])
                    .last(kid_ids[kid_ids.len() - 1])
                    .count(kid_ids.len() as i32);
            }
            item.dest().page(page_ids[node.page]).xyz(0.0, page_h, None);
            item.finish();
            for (k, (kid_id, (kid_title, kid_page))) in
                kid_ids.iter().zip(&node.children).enumerate()
            {
                let mut kid = pdf.outline_item(*kid_id);
                kid.title(TextStr(kid_title));
                kid.parent(*item_id);
                if k > 0 {
                    kid.prev(kid_ids[k - 1]);
                }
                if k + 1 < kid_ids.len() {
                    kid.next(kid_ids[k + 1]);
                }
                kid.dest().page(page_ids[*kid_page]).xyz(0.0, page_h, None);
            }
        }
    }

    Ok(pdf.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck;
    use crate::replay::SnapshotProvider;
    use ratatui::layout::Rect;

    fn pdf_for(src: &str) -> Vec<u8> {
        let deck = deck::parse(src, "test deck").unwrap();
        let mut presenter = Presenter::new(deck, vec![], SnapshotProvider::default()).unwrap();
        render(&mut presenter, &Options::default()).unwrap()
    }

    #[test]
    fn renders_a_deck_to_pdf_pages() {
        let bytes = pdf_for("# alpha\nhello *world*\n---\n# beta\n- one\n- two\n--\n## deeper\n");
        assert!(bytes.starts_with(b"%PDF-"));
        let text = String::from_utf8_lossy(&bytes);
        // Three slides → three pages.
        assert_eq!(text.matches("/Type /Page").count() - 1, 3, "page count");
        // An embedded, subsetted font and its ToUnicode map.
        assert!(text.contains("/FontFile2"));
        assert!(text.contains("/ToUnicode"));
        assert!(text.contains("/Outlines"));
    }

    #[test]
    fn content_stream_typesets_text_and_geometry() {
        let deck = deck::parse("# t\nhi\n> quoted\n```qr\nx\n```\n", "t").unwrap();
        let mut presenter = Presenter::new(deck, vec![], SnapshotProvider::default()).unwrap();
        let order = presenter.deck().flat();
        let area = Rect::new(0, 0, 60, 21);
        presenter.goto(order[0].0, order[0].1);
        let mut buf = Buffer::empty(area);
        presenter.draw(area, &mut buf);
        let mut fonts = Fonts::new().unwrap();
        let content = emit_page(&buf, 60, 20, 10.0, &mut fonts, None);
        assert!(content.starts_with("q\n"));
        assert!(content.contains("Tj"), "no text typeset:\n{content}");
        // The QR block paints vector rects with its light background.
        assert!(content.contains(" re f"));
        assert!(fonts.regular.is_used());
    }

    #[test]
    fn degenerate_font_sizes_are_clamped() {
        // `--font-size 0`, negatives, and NaN (all accepted by clap's
        // f32 parser) must not reach the page geometry.
        let deck = deck::parse("# a\nhi\n", "t").unwrap();
        let mut presenter = Presenter::new(deck, vec![], SnapshotProvider::default()).unwrap();
        for bad in [0.0, -5.0, f32::NAN, f32::INFINITY] {
            let opts = Options {
                font_size: bad,
                ..Options::default()
            };
            let bytes = render(&mut presenter, &opts).unwrap();
            assert!(bytes.starts_with(b"%PDF-"), "font_size {bad} broke render");
            let text = String::from_utf8_lossy(&bytes);
            assert!(
                !text.contains("NaN") && !text.contains("MediaBox [0 0 0"),
                "font_size {bad} leaked into geometry"
            );
        }
    }

    #[test]
    fn terminal_decks_require_a_snapshot_choice() {
        let dir = std::env::temp_dir().join(format!("deckhand-pdf-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("deck.md");
        std::fs::write(&path, "# a\n```terminal\nhtop\n```\n").unwrap();
        let out = dir.join("deck.pdf");

        let err = run(path.to_str().unwrap(), Some(&out), &Options::default()).unwrap_err();
        assert!(format!("{err:#}").contains("htop"));

        let opts = Options {
            snapshots: Some(false),
            ..Options::default()
        };
        run(path.to_str().unwrap(), Some(&out), &opts).unwrap();
        assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF-"));
    }

    #[test]
    fn snapshot_terminals_need_no_choice() {
        use crate::deck::{Segment, TermSnapshot};
        let mut deck = deck::parse("```terminal rows=4\nls\n```\n", "t").unwrap();
        let Segment::Terminal(block) = &mut deck.columns[0].slides[0].segments[0] else {
            panic!("expected terminal");
        };
        block.snapshot = Some(TermSnapshot {
            cols: 40,
            rows: 4,
            data: b"\x1b[1;1Hsnapped \x1b[31mred\x1b[0m".to_vec(),
        });
        assert!(deck.unsnapshotted_commands().is_empty());

        let mut presenter = Presenter::new(deck, vec![], SnapshotProvider::default()).unwrap();
        let bytes = render(&mut presenter, &Options::default()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
    }
}
