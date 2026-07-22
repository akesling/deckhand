//! QR codes in slides.
//!
//! A fenced block whose info string is `qr` renders its body — a URL,
//! some text, wifi credentials — as a scannable QR code:
//!
//! ````text
//! ```qr
//! https://deckhand.sh
//! ```
//! ````
//!
//! Encoding happens at parse time (so a bad payload fails with slide
//! context); drawing happens in the presenter, which paints the rows
//! black-on-white regardless of the terminal palette — scanners want a
//! dark code on a light background, and a themed terminal can't be
//! trusted to provide either.

use anyhow::{Context, Result};
use qrcode::QrCode;
use qrcode::render::unicode::Dense1x2;

/// Encode `data` into rows of unicode half-block characters, quiet zone
/// included. Each row is one terminal line; every row has the same
/// width. Dark modules are the drawn (block) characters, so render with
/// a dark foreground on a light background.
///
/// ```
/// let rows = deckhand::qr::encode("https://deckhand.sh")?;
/// assert!(rows.len() > 10);
/// assert!(rows.iter().all(|r| r.chars().count() == rows[0].chars().count()));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn encode(data: &str) -> Result<Vec<String>> {
    let code = QrCode::new(data.as_bytes())
        .with_context(|| format!("encoding QR code from {} bytes of data", data.len()))?;
    let art = code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Dark)
        .light_color(Dense1x2::Light)
        .build();
    Ok(art.lines().map(str::to_string).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_scannable_grid() {
        let rows = encode("https://deckhand.sh").unwrap();
        assert!(!rows.is_empty());
        // Uniform width, and actual dark modules present.
        let w = rows[0].chars().count();
        assert!(rows.iter().all(|r| r.chars().count() == w));
        assert!(rows.iter().any(|r| r.contains('█') || r.contains('▀')));
        // The quiet zone: first row is all light.
        assert!(rows[0].chars().all(|c| c == ' '));
    }

    #[test]
    fn oversized_payload_errors() {
        // QR tops out just shy of 3 KB of binary data.
        assert!(encode(&"x".repeat(4000)).is_err());
    }
}
