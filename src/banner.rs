//! Banner headings: H1s as large block glyphs.
//!
//! `h1_style: banner` in a theme renders level-1 headings through an
//! embedded 3×5 pixel font, drawn with half-block characters so each
//! heading is three terminal rows tall. Coverage is A–Z, 0–9, and
//! light punctuation; a heading using anything else (or too wide for
//! the slide) falls back to a normal H1 — loud typography should
//! never cost you a title.

/// One glyph: five pixel rows, `#` = on. All rows the same width.
type Glyph = [&'static str; 5];

#[rustfmt::skip]
const FONT: &[(char, Glyph)] = &[
    ('A', [".#.", "#.#", "###", "#.#", "#.#"]),
    ('B', ["##.", "#.#", "##.", "#.#", "##."]),
    ('C', [".##", "#..", "#..", "#..", ".##"]),
    ('D', ["##.", "#.#", "#.#", "#.#", "##."]),
    ('E', ["###", "#..", "##.", "#..", "###"]),
    ('F', ["###", "#..", "##.", "#..", "#.."]),
    ('G', [".##", "#..", "#.#", "#.#", ".##"]),
    ('H', ["#.#", "#.#", "###", "#.#", "#.#"]),
    ('I', ["###", ".#.", ".#.", ".#.", "###"]),
    ('J', ["..#", "..#", "..#", "#.#", ".#."]),
    ('K', ["#.#", "#.#", "##.", "#.#", "#.#"]),
    ('L', ["#..", "#..", "#..", "#..", "###"]),
    ('M', ["#.#", "###", "#.#", "#.#", "#.#"]),
    ('N', ["##.", "#.#", "#.#", "#.#", "#.#"]),
    ('O', [".#.", "#.#", "#.#", "#.#", ".#."]),
    ('P', ["##.", "#.#", "##.", "#..", "#.."]),
    ('Q', [".#.", "#.#", "#.#", ".#.", "..#"]),
    ('R', ["##.", "#.#", "##.", "#.#", "#.#"]),
    ('S', [".##", "#..", ".#.", "..#", "##."]),
    ('T', ["###", ".#.", ".#.", ".#.", ".#."]),
    ('U', ["#.#", "#.#", "#.#", "#.#", "###"]),
    ('V', ["#.#", "#.#", "#.#", "#.#", ".#."]),
    ('W', ["#.#", "#.#", "#.#", "###", "#.#"]),
    ('X', ["#.#", "#.#", ".#.", "#.#", "#.#"]),
    ('Y', ["#.#", "#.#", ".#.", ".#.", ".#."]),
    ('Z', ["###", "..#", ".#.", "#..", "###"]),
    ('0', [".#.", "#.#", "#.#", "#.#", ".#."]),
    ('1', [".#.", "##.", ".#.", ".#.", "###"]),
    ('2', ["##.", "..#", ".#.", "#..", "###"]),
    ('3', ["##.", "..#", ".#.", "..#", "##."]),
    ('4', ["#.#", "#.#", "###", "..#", "..#"]),
    ('5', ["###", "#..", "##.", "..#", "##."]),
    ('6', [".##", "#..", "###", "#.#", "###"]),
    ('7', ["###", "..#", ".#.", ".#.", ".#."]),
    ('8', ["###", "#.#", "###", "#.#", "###"]),
    ('9', ["###", "#.#", "###", "..#", "##."]),
    (' ', ["..", "..", "..", "..", ".."]),
    ('-', ["...", "...", "###", "...", "..."]),
    ('.', [".", ".", ".", ".", "#"]),
    ('!', ["#", "#", "#", ".", "#"]),
    ('?', ["###", "..#", ".#.", "...", ".#."]),
    (':', [".", "#", ".", "#", "."]),
    ('\'', ["#", "#", ".", ".", "."]),
    (',', [".", ".", ".", "#", "#"]),
    ('&', [".#.", "#.#", ".#.", "#.#", ".##"]),
    ('/', ["..#", "..#", ".#.", "#..", "#.."]),
];

fn glyph(c: char) -> Option<&'static Glyph> {
    let c = c.to_ascii_uppercase();
    FONT.iter().find(|(f, _)| *f == c).map(|(_, g)| g)
}

/// Render `text` as half-block rows (three of them), or `None` when a
/// character has no glyph or the result wouldn't fit in `max_width` —
/// callers fall back to a normal heading.
pub fn render(text: &str, max_width: usize) -> Option<Vec<String>> {
    let glyphs: Vec<&Glyph> = text.chars().map(glyph).collect::<Option<_>>()?;
    if glyphs.is_empty() {
        return None;
    }
    let width: usize = glyphs.iter().map(|g| g[0].len() + 1).sum::<usize>() - 1;
    if width > max_width {
        return None;
    }
    // Assemble the five pixel rows, one blank column between glyphs.
    let mut px = [const { String::new() }; 5];
    for (i, g) in glyphs.iter().enumerate() {
        for (r, row) in g.iter().enumerate() {
            px[r].push_str(row);
            if i + 1 < glyphs.len() {
                px[r].push('.');
            }
        }
    }
    // Two pixel rows per terminal row via half-blocks (row 5 pairs
    // with an empty row).
    let fold = |top: &str, bottom: &str| -> String {
        top.chars()
            .zip(bottom.chars().chain(std::iter::repeat('.')))
            .map(|(t, b)| match (t == '#', b == '#') {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            })
            .collect()
    };
    Some(vec![
        fold(&px[0], &px[1]),
        fold(&px[2], &px[3]),
        fold(&px[4], ""),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_three_rows_of_half_blocks() {
        let rows = render("deckhand", 96).unwrap();
        assert_eq!(rows.len(), 3);
        let w = rows[0].chars().count();
        assert!(rows.iter().all(|r| r.chars().count() == w));
        // 8 glyphs, 3px wide, 1px apart.
        assert_eq!(w, 8 * 4 - 1);
        assert!(rows[0].contains('█') || rows[0].contains('▀'));
    }

    #[test]
    fn case_insensitive_same_shape() {
        assert_eq!(render("abc", 96), render("ABC", 96));
    }

    #[test]
    fn unsupported_or_oversized_fall_back() {
        assert_eq!(render("hi ⚓", 96), None);
        assert_eq!(render("deckhand", 10), None);
        assert_eq!(render("", 96), None);
    }
}
