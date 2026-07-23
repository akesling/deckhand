//! The monospace faces embedded in exported PDFs.
//!
//! Text cells render in DejaVu Sans Mono (regular and bold; italics are
//! synthesized by slanting the text matrix, as terminals do). Each face
//! is subset to the glyphs the deck actually uses before embedding, so
//! a PDF carries kilobytes of font, not the full family. Geometric
//! characters never reach the font — see [`super::glyphs`].

use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::{Context, Result};

/// DejaVu Sans Mono, Book. Public-domain-style license: see
/// `fonts/LICENSE` (Bitstream Vera + public domain additions).
const REGULAR: &[u8] = include_bytes!("fonts/DejaVuSansMono.ttf");
/// DejaVu Sans Mono, Bold.
const BOLD: &[u8] = include_bytes!("fonts/DejaVuSansMono-Bold.ttf");

/// Which of the two embedded faces a cell uses.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FaceId {
    /// DejaVu Sans Mono Book.
    Regular,
    /// DejaVu Sans Mono Bold.
    Bold,
}

/// Font-unit metrics shared by the grid (both faces of a monospace
/// family agree on these).
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    /// Font units per em.
    pub upem: f32,
    /// The uniform glyph advance (cell width), font units.
    pub advance: f32,
    /// hhea ascender, font units.
    pub ascent: f32,
    /// hhea descender (negative), font units.
    pub descent: f32,
    /// Underline position relative to the baseline (negative), font
    /// units.
    pub under_pos: f32,
    /// Underline thickness, font units.
    pub under_thick: f32,
}

impl Metrics {
    /// Cell width in points at `size`.
    pub fn cell_w(&self, size: f32) -> f32 {
        self.advance / self.upem * size
    }
    /// Cell height in points at `size`.
    pub fn cell_h(&self, size: f32) -> f32 {
        (self.ascent - self.descent) / self.upem * size
    }
    /// Baseline drop from the cell top in points at `size`.
    pub fn baseline(&self, size: f32) -> f32 {
        self.ascent / self.upem * size
    }
    /// Scale a font-unit value to thousandths of an em (PDF glyph
    /// space, as `/W`, `/Ascent` etc. want).
    pub fn to_pdf_units(self, v: f32) -> f32 {
        v * 1000.0 / self.upem
    }
}

/// One embedded face: the parsed font plus which glyphs the deck used.
pub struct Face {
    data: &'static [u8],
    face: ttf_parser::Face<'static>,
    /// gid → the character it drew (first use wins; feeds ToUnicode).
    used: BTreeMap<u16, char>,
}

impl Face {
    fn new(data: &'static [u8]) -> Result<Face> {
        let face = ttf_parser::Face::parse(data, 0).context("parsing embedded font")?;
        Ok(Face {
            data,
            face,
            used: BTreeMap::new(),
        })
    }

    /// The glyph for `ch`, recording it as used. `None` when the face
    /// has no coverage (the cell is left blank).
    pub fn glyph(&mut self, ch: char) -> Option<u16> {
        let gid = self.face.glyph_index(ch)?.0;
        self.used.entry(gid).or_insert(ch);
        Some(gid)
    }

    /// Whether `gid` advances exactly one cell — glyphs that don't
    /// (rare in a mono face) break out of text runs and position
    /// themselves.
    pub fn standard_advance(&self, gid: u16, metrics: &Metrics) -> bool {
        self.face
            .glyph_hor_advance(ttf_parser::GlyphId(gid))
            .map(|a| a as f32 == metrics.advance)
            .unwrap_or(false)
    }

    /// Any glyphs recorded?
    pub fn is_used(&self) -> bool {
        !self.used.is_empty()
    }

    /// Subset to the used glyphs. Returns the font program and the
    /// old-gid → new-gid mapping (new gids are the PDF's CIDs).
    pub fn subset(&self) -> Result<(Vec<u8>, BTreeMap<u16, u16>)> {
        let gids: Vec<u16> = self.used.keys().copied().collect();
        let remapper = subsetter::GlyphRemapper::new_from_glyphs(&gids);
        let data = subsetter::subset(self.data, 0, &remapper)
            .map_err(|e| anyhow::anyhow!("subsetting font: {e}"))?;
        let remap = gids
            .iter()
            .filter_map(|&g| remapper.get(g).map(|n| (g, n)))
            .collect();
        Ok((data, remap))
    }

    /// The ToUnicode CMap for the subset — so text copies out of the
    /// PDF as the characters the deck showed.
    pub fn to_unicode(&self, remap: &BTreeMap<u16, u16>) -> String {
        let mut pairs: Vec<(u16, char)> = self
            .used
            .iter()
            .filter_map(|(old, &ch)| remap.get(old).map(|&new| (new, ch)))
            .collect();
        pairs.sort_unstable();

        let mut out = String::from(
            "/CIDInit /ProcSet findresource begin\n\
             12 dict begin\n\
             begincmap\n\
             /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
             /CMapName /Adobe-Identity-UCS def\n\
             /CMapType 2 def\n\
             1 begincodespacerange\n\
             <0000> <FFFF>\n\
             endcodespacerange\n",
        );
        for chunk in pairs.chunks(100) {
            let _ = writeln!(out, "{} beginbfchar", chunk.len());
            for (gid, ch) in chunk {
                let _ = write!(out, "<{gid:04X}> <");
                for unit in ch.encode_utf16(&mut [0u16; 2]) {
                    let _ = write!(out, "{unit:04X}");
                }
                out.push_str(">\n");
            }
            out.push_str("endbfchar\n");
        }
        out.push_str(
            "endcmap\n\
             CMapName currentdict /CMap defineresource pop\n\
             end\nend\n",
        );
        out
    }

    /// Descriptor numbers, in PDF glyph space (thousandths of an em).
    pub fn descriptor(&self, metrics: &Metrics) -> Descriptor {
        let bbox = self.face.global_bounding_box();
        Descriptor {
            ascent: metrics.to_pdf_units(metrics.ascent),
            descent: metrics.to_pdf_units(metrics.descent),
            cap_height: metrics.to_pdf_units(
                self.face
                    .capital_height()
                    .map(|v| v as f32)
                    .unwrap_or(metrics.ascent * 0.7),
            ),
            bbox: [
                metrics.to_pdf_units(bbox.x_min as f32),
                metrics.to_pdf_units(bbox.y_min as f32),
                metrics.to_pdf_units(bbox.x_max as f32),
                metrics.to_pdf_units(bbox.y_max as f32),
            ],
        }
    }
}

/// FontDescriptor numbers for one face.
pub struct Descriptor {
    /// /Ascent.
    pub ascent: f32,
    /// /Descent.
    pub descent: f32,
    /// /CapHeight.
    pub cap_height: f32,
    /// /FontBBox.
    pub bbox: [f32; 4],
}

/// Both embedded faces plus the shared grid metrics.
pub struct Fonts {
    /// Book.
    pub regular: Face,
    /// Bold.
    pub bold: Face,
    /// Shared monospace metrics (from the regular face).
    pub metrics: Metrics,
}

impl Fonts {
    /// Parse the embedded faces.
    pub fn new() -> Result<Fonts> {
        let regular = Face::new(REGULAR)?;
        let bold = Face::new(BOLD)?;
        let advance = regular
            .face
            .glyph_index('M')
            .and_then(|g| regular.face.glyph_hor_advance(g))
            .context("embedded font has no 'M'")?;
        let upem = regular.face.units_per_em() as f32;
        let under = regular.face.underline_metrics();
        let metrics = Metrics {
            upem,
            advance: advance as f32,
            ascent: regular.face.ascender() as f32,
            descent: regular.face.descender() as f32,
            under_pos: under.map(|u| u.position as f32).unwrap_or(-0.085 * upem),
            under_thick: under.map(|u| u.thickness as f32).unwrap_or(0.05 * upem),
        };
        Ok(Fonts {
            regular,
            bold,
            metrics,
        })
    }

    /// The face for `id`.
    pub fn face_mut(&mut self, id: FaceId) -> &mut Face {
        match id {
            FaceId::Regular => &mut self.regular,
            FaceId::Bold => &mut self.bold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_look_like_a_terminal_cell() {
        let fonts = Fonts::new().unwrap();
        let m = fonts.metrics;
        // DejaVu Sans Mono: 2048 upem, advance 1233 — a cell close to
        // the classic 1:2 terminal aspect.
        assert_eq!(m.upem, 2048.0);
        let aspect = m.cell_h(10.0) / m.cell_w(10.0);
        assert!(
            (1.6..=2.2).contains(&aspect),
            "cell aspect {aspect} out of range"
        );
    }

    #[test]
    fn coverage_includes_the_presenters_chrome() {
        let mut fonts = Fonts::new().unwrap();
        // Not vector-drawn, so these must come from the font.
        for ch in ['a', 'Z', '0', '·', '—', '?', 'é', '␣'] {
            assert!(
                fonts.regular.glyph(ch).is_some(),
                "regular face lacks {ch:?}"
            );
        }
        for ch in ['a', '·'] {
            assert!(fonts.bold.glyph(ch).is_some(), "bold face lacks {ch:?}");
        }
    }

    #[test]
    fn subset_keeps_used_glyphs_and_maps_to_unicode() {
        let mut fonts = Fonts::new().unwrap();
        for ch in "hello world".chars() {
            fonts.regular.glyph(ch);
        }
        let (data, remap) = fonts.regular.subset().unwrap();
        assert!(!data.is_empty());
        assert!(data.len() < REGULAR.len() / 4, "subset barely shrank");
        // The subset parses as a font and still has every used glyph.
        let sub = ttf_parser::Face::parse(&data, 0).unwrap();
        assert!(sub.number_of_glyphs() as usize > remap.len());

        let cmap = fonts.regular.to_unicode(&remap);
        assert!(cmap.contains("beginbfchar"));
        // 'h' is in there, as UTF-16BE 0068.
        assert!(cmap.contains("<0068>"), "cmap:\n{cmap}");
    }

    #[test]
    fn standard_advance_holds_for_ascii() {
        let mut fonts = Fonts::new().unwrap();
        let m = fonts.metrics;
        for ch in ('!'..='~').chain(['─', '█', '·']) {
            if let Some(gid) = fonts.regular.glyph(ch) {
                assert!(
                    fonts.regular.standard_advance(gid, &m),
                    "{ch:?} is not one cell wide"
                );
            }
        }
    }
}
