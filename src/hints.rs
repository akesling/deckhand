//! Quick-jump hint codes for the overview (native TUI and web presenter).

/// Keys usable for overview jump codes, most ergonomic first. Excludes
/// the overview's own bindings (h j k l o q) so a code can never collide
/// with navigation.
pub const JUMP_KEYS: &[char] = &[
    'a', 's', 'd', 'f', 'g', 'e', 'r', 't', 'u', 'i', 'w', 'n', 'm', 'c', 'v', 'b', 'x', 'z', 'y',
    'p',
];

/// Hint codes for `n` slides: single letters while they last, otherwise
/// uniform two-letter codes (no prefix ambiguity either way).
pub fn jump_codes(n: usize) -> Vec<String> {
    if n <= JUMP_KEYS.len() {
        return JUMP_KEYS.iter().take(n).map(char::to_string).collect();
    }
    let mut out = Vec::with_capacity(n);
    'outer: for a in JUMP_KEYS {
        for b in JUMP_KEYS {
            if out.len() >= n {
                break 'outer;
            }
            out.push(format!("{a}{b}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_codes_are_unique_and_avoid_nav_keys() {
        for n in [1, 5, 20, 21, 100, 400] {
            let codes = jump_codes(n);
            assert_eq!(codes.len(), n);
            let unique: std::collections::HashSet<_> = codes.iter().collect();
            assert_eq!(unique.len(), codes.len());
            for code in &codes {
                assert!(
                    !code.chars().any(|c| "hjkloq".contains(c)),
                    "code {code:?} collides with overview navigation"
                );
            }
        }
    }

    #[test]
    fn jump_codes_shape() {
        assert_eq!(jump_codes(3), vec!["a", "s", "d"]);
        // beyond one letter per slide, all codes are uniform two-letter
        let codes = jump_codes(21);
        assert!(codes.iter().all(|c| c.chars().count() == 2));
        assert_eq!(codes[0], "aa");
    }
}
