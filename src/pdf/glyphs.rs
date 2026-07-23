//! Geometric characters drawn as PDF vector paths instead of font
//! glyphs.
//!
//! Box-drawing borders, block elements (banner headings, QR codes,
//! scrollbars), and the presenter's bullets/markers are all geometry by
//! definition. Drawing them as paths — the way terminal emulators like
//! kitty and alacritty synthesize them — makes borders connect without
//! gaps at any font size and keeps QR codes crisp enough to scan. Text
//! stays text; only characters in this set are painted.
//!
//! All coordinates are PDF points; a cell is the rectangle `(x, y)`
//! (bottom-left) to `(x + w, y + h)`. [`draw`] assumes the caller has
//! already set the fill (`rg`) and stroke (`RG`) color.

use std::fmt::Write as _;

/// Coverage of `░ ▒ ▓`: how much foreground to mix into the
/// background. The caller blends and sets the color before [`draw`],
/// which fills the whole cell.
pub fn shade(ch: char) -> Option<f32> {
    match ch {
        '░' => Some(0.25),
        '▒' => Some(0.5),
        '▓' => Some(0.75),
        _ => None,
    }
}

/// Paint `ch` into the cell, or return `false` when it isn't one of
/// the vector-drawn characters (the caller falls back to the font).
pub fn draw(ch: char, x: f32, y: f32, w: f32, h: f32, out: &mut String) -> bool {
    let mut g = Painter { x, y, w, h, out };
    // Stroke weights, relative to the cell like a terminal's renderer.
    let light = w * 0.15;
    let heavy = w * 0.33;
    match ch {
        // Light box drawing.
        '─' => g.arms(L | R, light),
        '│' => g.arms(U | D, light),
        '┌' => g.arms(D | R, light),
        '┐' => g.arms(D | L, light),
        '└' => g.arms(U | R, light),
        '┘' => g.arms(U | L, light),
        '├' => g.arms(U | D | R, light),
        '┤' => g.arms(U | D | L, light),
        '┬' => g.arms(D | L | R, light),
        '┴' => g.arms(U | L | R, light),
        '┼' => g.arms(U | D | L | R, light),
        // Heavy.
        '━' => g.arms(L | R, heavy),
        '┃' => g.arms(U | D, heavy),
        '┏' => g.arms(D | R, heavy),
        '┓' => g.arms(D | L, heavy),
        '┗' => g.arms(U | R, heavy),
        '┛' => g.arms(U | L, heavy),
        '┣' => g.arms(U | D | R, heavy),
        '┫' => g.arms(U | D | L, heavy),
        '┳' => g.arms(D | L | R, heavy),
        '┻' => g.arms(U | L | R, heavy),
        '╋' => g.arms(U | D | L | R, heavy),
        // Double: each arm is two parallel light lines. Junction gaps
        // aren't cut out (deckhand's own chrome never draws them).
        '═' => g.double(L | R),
        '║' => g.double(U | D),
        '╔' => g.double(D | R),
        '╗' => g.double(D | L),
        '╚' => g.double(U | R),
        '╝' => g.double(U | L),
        '╠' => g.double(U | D | R),
        '╣' => g.double(U | D | L),
        '╦' => g.double(D | L | R),
        '╩' => g.double(U | L | R),
        '╬' => g.double(U | D | L | R),
        // Rounded corners (the default border type).
        '╭' => g.rounded(D, R, light),
        '╮' => g.rounded(D, L, light),
        '╰' => g.rounded(U, R, light),
        '╯' => g.rounded(U, L, light),
        // Full and partial blocks.
        '█' | '░' | '▒' | '▓' => g.frect(0.0, 0.0, 1.0, 1.0),
        '▉' => g.frect(0.0, 0.0, 0.875, 1.0),
        '▊' => g.frect(0.0, 0.0, 0.75, 1.0),
        '▋' => g.frect(0.0, 0.0, 0.625, 1.0),
        '▌' => g.frect(0.0, 0.0, 0.5, 1.0),
        '▍' => g.frect(0.0, 0.0, 0.375, 1.0),
        '▎' => g.frect(0.0, 0.0, 0.25, 1.0),
        '▏' => g.frect(0.0, 0.0, 0.125, 1.0),
        '▐' => g.frect(0.5, 0.0, 0.5, 1.0),
        '▕' => g.frect(0.875, 0.0, 0.125, 1.0),
        '▁' => g.frect(0.0, 0.0, 1.0, 0.125),
        '▂' => g.frect(0.0, 0.0, 1.0, 0.25),
        '▃' => g.frect(0.0, 0.0, 1.0, 0.375),
        '▄' => g.frect(0.0, 0.0, 1.0, 0.5),
        '▅' => g.frect(0.0, 0.0, 1.0, 0.625),
        '▆' => g.frect(0.0, 0.0, 1.0, 0.75),
        '▇' => g.frect(0.0, 0.0, 1.0, 0.875),
        '▀' => g.frect(0.0, 0.5, 1.0, 0.5),
        '▔' => g.frect(0.0, 0.875, 1.0, 0.125),
        // Quadrants.
        '▘' => g.quads(&[UL]),
        '▝' => g.quads(&[UR]),
        '▖' => g.quads(&[LL]),
        '▗' => g.quads(&[LR]),
        '▚' => g.quads(&[UL, LR]),
        '▞' => g.quads(&[UR, LL]),
        '▙' => g.quads(&[UL, LL, LR]),
        '▛' => g.quads(&[UL, UR, LL]),
        '▜' => g.quads(&[UL, UR, LR]),
        '▟' => g.quads(&[UR, LL, LR]),
        // Bullets and markers. Sized in `w` units so they stay round
        // in a ~1:2 cell.
        '•' => g.disc(0.32, true),
        '◦' => g.ring(0.32, 0.18),
        '●' => g.disc(0.45, true),
        '○' => g.ring(0.45, 0.33),
        '▪' => {
            let s = 0.55;
            g.frect(0.5 - s / 2.0, 0.5 - s * g.wh() / 2.0, s, s * g.wh())
        }
        '■' => {
            let s = 0.9;
            g.frect(0.5 - s / 2.0, 0.5 - s * g.wh() / 2.0, s, s * g.wh())
        }
        '▸' => g.tri_h(0.30, 0.40, 0.35, true),
        '▹' => g.tri_h(0.30, 0.40, 0.35, false),
        '▶' => g.tri_h(0.45, 0.55, 0.50, true),
        '◂' => g.tri_h(-0.30, -0.40, 0.35, true),
        '◃' => g.tri_h(-0.30, -0.40, 0.35, false),
        '◀' => g.tri_h(-0.45, -0.55, 0.50, true),
        '▴' => g.tri_v(0.30, 0.40, 0.35, true),
        '▵' => g.tri_v(0.30, 0.40, 0.35, false),
        '▲' => g.tri_v(0.45, 0.55, 0.50, true),
        '▾' => g.tri_v(-0.30, -0.40, 0.35, true),
        '▽' => g.tri_v(-0.45, -0.55, 0.50, false),
        '▼' => g.tri_v(-0.45, -0.55, 0.50, true),
        '▦' => g.grid_square(),
        _ => return false,
    }
    true
}

// Arm bitflags: which cell edges a box-drawing stroke reaches.
const L: u8 = 1;
const R: u8 = 2;
const U: u8 = 4;
const D: u8 = 8;

// Quadrants as (left, bottom) in cell fractions.
const UL: (f32, f32) = (0.0, 0.5);
const UR: (f32, f32) = (0.5, 0.5);
const LL: (f32, f32) = (0.0, 0.0);
const LR: (f32, f32) = (0.5, 0.0);

/// Kappa: cubic bezier approximation of a quarter circle.
const K: f32 = 0.5523;

struct Painter<'a> {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    out: &'a mut String,
}

impl Painter<'_> {
    /// Height of one `w` unit as a fraction of cell height — for
    /// squares/circles specified in cell widths.
    fn wh(&self) -> f32 {
        self.w / self.h
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let _ = write!(self.out, "{x:.2} {y:.2} {w:.2} {h:.2} re ");
    }

    /// Fill a rectangle given in cell fractions.
    fn frect(&mut self, fx: f32, fy: f32, fw: f32, fh: f32) {
        self.rect(
            self.x + fx * self.w,
            self.y + fy * self.h,
            fw * self.w,
            fh * self.h,
        );
        self.out.push_str("f\n");
    }

    /// Box-drawing arms: a stroke of thickness `t` from the cell center
    /// to each flagged edge, drawn as overlapping fills so junctions
    /// are seamless.
    fn arms(&mut self, dirs: u8, t: f32) {
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        let ht = t / 2.0;
        if dirs & L != 0 {
            self.rect(self.x, cy - ht, self.w / 2.0 + ht, t);
        }
        if dirs & R != 0 {
            self.rect(cx - ht, cy - ht, self.w / 2.0 + ht, t);
        }
        if dirs & U != 0 {
            self.rect(cx - ht, cy - ht, t, self.h / 2.0 + ht);
        }
        if dirs & D != 0 {
            self.rect(cx - ht, self.y, t, self.h / 2.0 + ht);
        }
        self.out.push_str("f\n");
    }

    /// Double lines: two parallel light strokes per arm.
    fn double(&mut self, dirs: u8) {
        let t = self.w * 0.12;
        let off = self.w * 0.19;
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        let ht = t / 2.0;
        // Reach: arms extend to the far rail so corners close.
        let reach = off + ht;
        if dirs & L != 0 {
            for s in [-1.0f32, 1.0] {
                self.rect(self.x, cy + s * off - ht, self.w / 2.0 + reach, t);
            }
        }
        if dirs & R != 0 {
            for s in [-1.0f32, 1.0] {
                self.rect(cx - reach, cy + s * off - ht, self.w / 2.0 + reach, t);
            }
        }
        if dirs & U != 0 {
            for s in [-1.0f32, 1.0] {
                self.rect(cx + s * off - ht, cy - reach, t, self.h / 2.0 + reach);
            }
        }
        if dirs & D != 0 {
            for s in [-1.0f32, 1.0] {
                self.rect(cx + s * off - ht, self.y, t, self.h / 2.0 + reach);
            }
        }
        self.out.push_str("f\n");
    }

    /// A rounded corner: straight runs to the two flagged edges joined
    /// by a quarter-circle arc, stroked at width `t`. `v` is U or D,
    /// `hz` is L or R.
    fn rounded(&mut self, v: u8, hz: u8, t: f32) {
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        let r = self.w / 2.0;
        // Signs: which way the vertical run and horizontal run leave
        // the center.
        let sy = if v == U { 1.0f32 } else { -1.0 };
        let sx = if hz == R { 1.0f32 } else { -1.0 };
        let vy = if v == U { self.y + self.h } else { self.y };
        let hx = if hz == R { self.x + self.w } else { self.x };
        // Vertical edge → arc start → arc end → horizontal edge.
        let (ax, ay) = (cx, cy + sy * r);
        let (bx, by) = (cx + sx * r, cy);
        let _ = writeln!(
            self.out,
            "{t:.2} w 1 j {cx:.2} {vy:.2} m {ax:.2} {ay:.2} l \
             {c1x:.2} {c1y:.2} {c2x:.2} {c2y:.2} {bx:.2} {by:.2} c \
             {hx:.2} {by:.2} l S",
            c1x = ax,
            c1y = ay - sy * K * r,
            c2x = bx - sx * K * r,
            c2y = by,
        );
    }

    fn quads(&mut self, quads: &[(f32, f32)]) {
        for &(fx, fy) in quads {
            self.rect(
                self.x + fx * self.w,
                self.y + fy * self.h,
                self.w / 2.0,
                self.h / 2.0,
            );
        }
        self.out.push_str("f\n");
    }

    /// A circle of radius `r` (in `w` units) at the cell center.
    fn circle_path(&mut self, r: f32) {
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        let r = r * self.w;
        let k = K * r;
        let _ = write!(
            self.out,
            "{x0:.2} {cy:.2} m \
             {x0:.2} {y1:.2} {x1:.2} {y2:.2} {cx:.2} {y2:.2} c \
             {x2:.2} {y2:.2} {x3:.2} {y1:.2} {x3:.2} {cy:.2} c \
             {x3:.2} {y3:.2} {x2:.2} {y4:.2} {cx:.2} {y4:.2} c \
             {x1:.2} {y4:.2} {x0:.2} {y3:.2} {x0:.2} {cy:.2} c ",
            x0 = cx - r,
            x1 = cx - k,
            x2 = cx + k,
            x3 = cx + r,
            y1 = cy + k,
            y2 = cy + r,
            y3 = cy - k,
            y4 = cy - r,
        );
    }

    fn disc(&mut self, r: f32, fill: bool) {
        self.circle_path(r);
        self.out.push_str(if fill { "f\n" } else { "S\n" });
    }

    /// An annulus via even-odd fill of two concentric circles.
    fn ring(&mut self, outer: f32, inner: f32) {
        self.circle_path(outer);
        self.circle_path(inner);
        self.out.push_str("f*\n");
    }

    /// Horizontal-pointing triangle: base `base` behind the center,
    /// apex `apex` past it (both in `w` units, negative = pointing
    /// left), half-height `half` in `w` units.
    fn tri_h(&mut self, base: f32, apex: f32, half: f32, fill: bool) {
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        let (x0, x1) = (cx - base * self.w, cx + apex * self.w);
        let dy = half * self.w;
        let _ = writeln!(
            self.out,
            "{x0:.2} {y0:.2} m {x0:.2} {y1:.2} l {x1:.2} {cy:.2} l h {op}",
            y0 = cy - dy,
            y1 = cy + dy,
            op = if fill { "f" } else { "S" },
        );
    }

    /// Vertical-pointing triangle (positive = up), same conventions.
    fn tri_v(&mut self, base: f32, apex: f32, half: f32, fill: bool) {
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        let (y0, y1) = (cy - base * self.w, cy + apex * self.w);
        let dx = half * self.w;
        let _ = writeln!(
            self.out,
            "{x0:.2} {y0:.2} m {x1:.2} {y0:.2} l {cx:.2} {y1:.2} l h {op}",
            x0 = cx - dx,
            x1 = cx + dx,
            op = if fill { "f" } else { "S" },
        );
    }

    /// `▦`: a stroked square with center cross-hairs.
    fn grid_square(&mut self) {
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        let s = 0.8 * self.w;
        let t = 0.09 * self.w;
        let (x0, y0) = (cx - s / 2.0, cy - s / 2.0);
        let _ = writeln!(
            self.out,
            "{t:.2} w {x0:.2} {y0:.2} {s:.2} {s:.2} re \
             {x0:.2} {cy:.2} m {x1:.2} {cy:.2} l \
             {cx:.2} {y0:.2} m {cx:.2} {y1:.2} l S",
            x1 = x0 + s,
            y1 = y0 + s,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(ch: char) -> Option<String> {
        let mut out = String::new();
        draw(ch, 10.0, 20.0, 6.0, 12.0, &mut out).then_some(out)
    }

    #[test]
    fn text_stays_text() {
        for ch in ['a', 'Z', '0', ' ', '·', '—', '?', 'é'] {
            assert!(ops(ch).is_none(), "{ch:?} should render as a font glyph");
        }
    }

    #[test]
    fn box_drawing_is_painted() {
        for ch in "─│┌┐└┘├┤┬┴┼━┃┏┓┗┛┣┫┳┻╋═║╔╗╚╝╠╣╦╩╬╭╮╰╯".chars()
        {
            let ops = ops(ch).unwrap_or_else(|| panic!("{ch:?} not painted"));
            assert!(
                ops.contains("f\n") || ops.contains("S\n"),
                "{ch:?} paints nothing: {ops}"
            );
        }
    }

    #[test]
    fn full_block_covers_the_cell() {
        let ops = ops('█').unwrap();
        assert_eq!(ops, "10.00 20.00 6.00 12.00 re f\n");
    }

    #[test]
    fn half_blocks_cover_half() {
        assert_eq!(ops('▀').unwrap(), "10.00 26.00 6.00 6.00 re f\n");
        assert_eq!(ops('▄').unwrap(), "10.00 20.00 6.00 6.00 re f\n");
        assert_eq!(ops('▌').unwrap(), "10.00 20.00 3.00 12.00 re f\n");
    }

    #[test]
    fn shades_fill_the_cell_with_blend_levels() {
        assert_eq!(shade('░'), Some(0.25));
        assert_eq!(shade('▒'), Some(0.5));
        assert_eq!(shade('▓'), Some(0.75));
        assert_eq!(shade('█'), None);
        assert_eq!(ops('▒').unwrap(), "10.00 20.00 6.00 12.00 re f\n");
    }

    #[test]
    fn bullets_and_markers_are_painted() {
        for ch in ['•', '◦', '▪', '▸', '▶', '◀', '▲', '▼', '▦'] {
            assert!(ops(ch).is_some(), "{ch:?} not painted");
        }
    }

    #[test]
    fn quadrants_compose() {
        let ops = ops('▚').unwrap();
        // Upper-left + lower-right.
        assert!(ops.contains("10.00 26.00 3.00 6.00 re"));
        assert!(ops.contains("13.00 20.00 3.00 6.00 re"));
    }
}
