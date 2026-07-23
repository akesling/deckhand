//! Concrete RGB values for every [`ratatui::style::Color`], shared by
//! the PDF and HTML exporters.
//!
//! A terminal defers color choices to the emulator's palette; an
//! exported document has no emulator, so the exporter must pick. These
//! values match what the web presenter shows: xterm.js defaults (the
//! Tango palette for ANSI 0-15, the standard 6×6×6 cube and grayscale
//! ramp for 16-255) over the site's dark surface.
//!
//! [`resolve`] flattens a cell's style the same way for every
//! exporter: concrete colors with `REVERSED` swapped and `DIM`
//! blended, plus the attributes left for the output format to draw.

use ratatui::style::{Color, Modifier};

/// Default foreground — cells whose fg is [`Color::Reset`].
pub const DEFAULT_FG: [u8; 3] = [0xd0, 0xd0, 0xd0];

/// Default background — cells whose bg is [`Color::Reset`], and the
/// page itself.
pub const DEFAULT_BG: [u8; 3] = [0x10, 0x10, 0x14];

/// ANSI 0-15, as xterm.js ships them (Tango).
const ANSI: [[u8; 3]; 16] = [
    [0x2e, 0x34, 0x36], // black
    [0xcc, 0x00, 0x00], // red
    [0x4e, 0x9a, 0x06], // green
    [0xc4, 0xa0, 0x00], // yellow
    [0x34, 0x65, 0xa4], // blue
    [0x75, 0x50, 0x7b], // magenta
    [0x06, 0x98, 0x9a], // cyan
    [0xd3, 0xd7, 0xcf], // white (ratatui's Gray)
    [0x55, 0x57, 0x53], // bright black (DarkGray)
    [0xef, 0x29, 0x29], // bright red
    [0x8a, 0xe2, 0x34], // bright green
    [0xfc, 0xe9, 0x4f], // bright yellow
    [0x72, 0x9f, 0xcf], // bright blue
    [0xad, 0x7f, 0xa8], // bright magenta
    [0x34, 0xe2, 0xe2], // bright cyan
    [0xee, 0xee, 0xec], // bright white
];

/// The color's RGB value, or `None` for [`Color::Reset`] (the caller
/// substitutes [`DEFAULT_FG`]/[`DEFAULT_BG`]).
pub fn rgb(color: Color) -> Option<[u8; 3]> {
    Some(match color {
        Color::Reset => return None,
        Color::Black => ANSI[0],
        Color::Red => ANSI[1],
        Color::Green => ANSI[2],
        Color::Yellow => ANSI[3],
        Color::Blue => ANSI[4],
        Color::Magenta => ANSI[5],
        Color::Cyan => ANSI[6],
        Color::Gray => ANSI[7],
        Color::DarkGray => ANSI[8],
        Color::LightRed => ANSI[9],
        Color::LightGreen => ANSI[10],
        Color::LightYellow => ANSI[11],
        Color::LightBlue => ANSI[12],
        Color::LightMagenta => ANSI[13],
        Color::LightCyan => ANSI[14],
        Color::White => ANSI[15],
        Color::Indexed(i) => indexed(i),
        Color::Rgb(r, g, b) => [r, g, b],
    })
}

/// The xterm 256-color palette: 16 ANSI, a 6×6×6 color cube, then a
/// 24-step grayscale ramp.
fn indexed(i: u8) -> [u8; 3] {
    match i {
        0..=15 => ANSI[i as usize],
        16..=231 => {
            let i = i - 16;
            let level = |n: u8| if n == 0 { 0 } else { 55 + 40 * n };
            [level(i / 36), level(i / 6 % 6), level(i % 6)]
        }
        232..=255 => {
            let v = 8 + 10 * (i - 232);
            [v, v, v]
        }
    }
}

/// `t` of the way from `a` to `b` — dim text, shade blocks.
pub fn blend(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
}

/// A cell's style with every terminal indirection resolved: concrete
/// colors and the attributes an exporter still has to draw itself.
pub struct Resolved {
    /// Foreground, `REVERSED` and `DIM` already applied.
    pub fg: [u8; 3],
    /// Background; `None` when the document background already covers
    /// it.
    pub bg: Option<[u8; 3]>,
    /// `BOLD`.
    pub bold: bool,
    /// `ITALIC`.
    pub italic: bool,
    /// `UNDERLINED`.
    pub underline: bool,
    /// `CROSSED_OUT`.
    pub strike: bool,
    /// `HIDDEN`: draw the background, not the glyph.
    pub hidden: bool,
}

/// Flatten a cell's `(fg, bg, modifiers)` into concrete colors:
/// defaults substituted, `REVERSED` swapped, `DIM` blended toward the
/// background.
pub fn resolve(fg: Color, bg: Color, m: Modifier) -> Resolved {
    let mut fg = rgb(fg);
    let mut bg = rgb(bg);
    if m.contains(Modifier::REVERSED) {
        let f = fg.unwrap_or(DEFAULT_FG);
        let b = bg.unwrap_or(DEFAULT_BG);
        (fg, bg) = (Some(b), Some(f));
    }
    let bg = bg.filter(|&b| b != DEFAULT_BG);
    let effective_bg = bg.unwrap_or(DEFAULT_BG);
    let mut fg = fg.unwrap_or(DEFAULT_FG);
    if m.contains(Modifier::DIM) {
        fg = blend(fg, effective_bg, 0.5);
    }
    Resolved {
        fg,
        bg,
        bold: m.contains(Modifier::BOLD),
        italic: m.contains(Modifier::ITALIC),
        underline: m.contains(Modifier::UNDERLINED),
        strike: m.contains(Modifier::CROSSED_OUT),
        hidden: m.contains(Modifier::HIDDEN),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_indexed_and_rgb_resolve() {
        assert_eq!(rgb(Color::Reset), None);
        assert_eq!(rgb(Color::Red), Some([0xcc, 0x00, 0x00]));
        // Named ANSI and their indexed twins agree.
        assert_eq!(rgb(Color::White), rgb(Color::Indexed(15)));
        // Cube corners: 16 is black, 231 is white, 196 is pure red.
        assert_eq!(rgb(Color::Indexed(16)), Some([0, 0, 0]));
        assert_eq!(rgb(Color::Indexed(231)), Some([255, 255, 255]));
        assert_eq!(rgb(Color::Indexed(196)), Some([255, 0, 0]));
        // Grayscale ramp ends.
        assert_eq!(rgb(Color::Indexed(232)), Some([8, 8, 8]));
        assert_eq!(rgb(Color::Indexed(255)), Some([238, 238, 238]));
        assert_eq!(rgb(Color::Rgb(1, 2, 3)), Some([1, 2, 3]));
    }

    #[test]
    fn resolve_applies_reverse_and_dim() {
        let r = resolve(Color::Reset, Color::Reset, Modifier::REVERSED);
        // Reversed default cell: fg becomes the page background, and
        // the old default fg becomes a real background to draw.
        assert_eq!(r.fg, DEFAULT_BG);
        assert_eq!(r.bg, Some(DEFAULT_FG));

        let plain = resolve(Color::White, Color::Reset, Modifier::empty());
        let dim = resolve(Color::White, Color::Reset, Modifier::DIM);
        assert_ne!(plain.fg, dim.fg);
        assert!(!plain.bold && !plain.hidden);

        let attrs = resolve(
            Color::Reset,
            Color::Reset,
            Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED,
        );
        assert!(attrs.bold && attrs.italic && attrs.underline && !attrs.strike);
    }

    #[test]
    fn blend_interpolates() {
        assert_eq!(blend([0, 0, 0], [255, 255, 255], 0.0), [0, 0, 0]);
        assert_eq!(blend([0, 0, 0], [255, 255, 255], 1.0), [255, 255, 255]);
        assert_eq!(blend([0, 0, 0], [100, 200, 50], 0.5), [50, 100, 25]);
    }
}
