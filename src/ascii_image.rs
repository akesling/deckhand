//! Images in slides, rendered as colored ASCII art.
//!
//! A markdown image on its own line (`![alt](diagram.png)`) becomes a
//! [`crate::deck::Segment::Image`]. When presenting natively, the file
//! is decoded and downscaled once at load time ([`load`], resolved
//! against the deck's directory); every frame, [`render`] rasterizes
//! the pixels into a luminance ramp of ASCII characters with per-cell
//! RGB foreground colors. Contexts without a filesystem (the browser
//! presenter) leave the pixels unloaded and show a placeholder instead.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};

/// Decoded, pre-downscaled pixels: row-major RGBA8. Small enough to
/// keep in the deck (bounded by [`load`]), big enough to sample a
/// full-width slide from.
#[derive(Clone, PartialEq, Eq)]
pub struct ImageData {
    /// Bitmap width in pixels.
    pub width: u32,
    /// Bitmap height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes, RGBA, row-major.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The derived impl would dump every pixel.
        write!(f, "ImageData({}x{})", self.width, self.height)
    }
}

/// Read and decode an image file, downscaling so the bitmap stays a
/// sampling source rather than a memory hog. PNG, JPEG, GIF, and WebP.
#[cfg(not(target_arch = "wasm32"))]
pub fn load(path: &std::path::Path) -> anyhow::Result<ImageData> {
    use anyhow::Context as _;
    let bytes = std::fs::read(path).with_context(|| format!("reading image {}", path.display()))?;
    let img = image::load_from_memory(&bytes)
        .with_context(|| format!("decoding image {}", path.display()))?;
    // Cells sample this bitmap; 512px across covers a 200-column slide
    // with pixels to spare.
    let img = if img.width() > 512 || img.height() > 512 {
        img.thumbnail(512, 512)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    Ok(ImageData {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    })
}

/// Dark → bright. Tuned for the common case of a dark terminal
/// background: dark pixels disappear into it, bright pixels get dense
/// glyphs.
const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

/// Rasterize into at most `max_cols` × `max_rows` cells, preserving
/// aspect ratio (a terminal cell is about twice as tall as it is wide)
/// and centering horizontally in `max_cols`.
pub fn render(img: &ImageData, max_cols: u16, max_rows: u16) -> Text<'static> {
    let iw = img.width.max(1) as f64;
    let ih = img.height.max(1) as f64;
    let max_cols = max_cols.max(1) as usize;
    let max_rows = max_rows.max(1) as usize;

    let mut cols = max_cols as f64;
    let mut rows = (cols * ih / iw / 2.0).max(1.0);
    if rows > max_rows as f64 {
        rows = max_rows as f64;
        cols = (rows * 2.0 * iw / ih).clamp(1.0, max_cols as f64);
    }
    let (cols, rows) = (cols.round().max(1.0) as usize, rows.round() as usize);
    let pad = (max_cols - cols.min(max_cols)) / 2;

    let mut lines = Vec::with_capacity(rows);
    for y in 0..rows {
        let mut spans: Vec<Span<'static>> = Vec::new();
        if pad > 0 {
            spans.push(Span::raw(" ".repeat(pad)));
        }
        // Coalesce same-colored runs so a line is a few spans, not `cols`.
        let mut run = String::new();
        let mut run_color: Option<Color> = None;
        for x in 0..cols {
            let (r, g, b, lum) = sample(img, x, y, cols, rows);
            let idx = (lum * (RAMP.len() - 1) as f64).round() as usize;
            let ch = RAMP[idx.min(RAMP.len() - 1)];
            // Spaces draw nothing; keep the current run's color alive.
            let color = if ch == ' ' {
                run_color.unwrap_or(Color::Reset)
            } else {
                Color::Rgb(r, g, b)
            };
            match run_color {
                Some(c) if c == color => run.push(ch),
                Some(c) => {
                    spans.push(Span::styled(
                        std::mem::take(&mut run),
                        Style::default().fg(c),
                    ));
                    run.push(ch);
                    run_color = Some(color);
                }
                None => {
                    run.push(ch);
                    run_color = Some(color);
                }
            }
        }
        if let Some(c) = run_color {
            spans.push(Span::styled(run, Style::default().fg(c)));
        }
        lines.push(Line::from(spans));
    }
    Text::from(lines)
}

/// Box-average the source region behind cell (x, y) of a `cols` ×
/// `rows` grid. Returns (r, g, b, luminance 0..=1); transparency fades
/// toward black — i.e. toward the ramp's empty end.
fn sample(img: &ImageData, x: usize, y: usize, cols: usize, rows: usize) -> (u8, u8, u8, f64) {
    let x0 = x * img.width as usize / cols;
    let x1 = ((x + 1) * img.width as usize / cols).max(x0 + 1);
    let y0 = y * img.height as usize / rows;
    let y1 = ((y + 1) * img.height as usize / rows).max(y0 + 1);
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    for py in y0..y1.min(img.height as usize) {
        for px in x0..x1.min(img.width as usize) {
            let i = (py * img.width as usize + px) * 4;
            let a = img.rgba[i + 3] as u64;
            r += img.rgba[i] as u64 * a / 255;
            g += img.rgba[i + 1] as u64 * a / 255;
            b += img.rgba[i + 2] as u64 * a / 255;
            n += 1;
        }
    }
    if n == 0 {
        return (0, 0, 0, 0.0);
    }
    let (r, g, b) = ((r / n) as u8, (g / n) as u8, (b / n) as u8);
    let lum = (0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64) / 255.0;
    (r, g, b, lum)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w`×`h` bitmap filled with one RGBA color.
    fn solid(w: u32, h: u32, px: [u8; 4]) -> ImageData {
        ImageData {
            width: w,
            height: h,
            rgba: px.repeat((w * h) as usize),
        }
    }

    #[test]
    fn fits_width_and_aspect() {
        // A square image at 40 cols should be ~20 rows (cells are 1:2).
        let text = render(&solid(100, 100, [255, 255, 255, 255]), 40, 100);
        assert_eq!(text.height(), 20);
        assert!(text.lines.iter().all(|l| l.width() <= 40));
    }

    #[test]
    fn caps_height_and_recenters() {
        let text = render(&solid(100, 100, [255, 0, 0, 255]), 80, 10);
        assert_eq!(text.height(), 10);
        // 10 rows → 20 cols wide, centered in 80: 30 cells of pad.
        assert!(text.lines[0].width() > 20);
        assert!(format!("{:?}", text.lines[0]).contains("Rgb(255, 0, 0)"));
    }

    #[test]
    fn bright_pixels_get_dense_glyphs() {
        let text = render(&solid(10, 10, [255, 255, 255, 255]), 10, 10);
        assert!(format!("{:?}", text.lines[0]).contains('@'));
    }

    #[test]
    fn transparent_renders_blank() {
        let text = render(&solid(10, 10, [255, 255, 255, 0]), 10, 10);
        for line in &text.lines {
            for span in &line.spans {
                assert!(span.content.chars().all(|c| c == ' '));
            }
        }
    }
}
