//! Quick-jump hint codes for the overview (native TUI and web presenter).

/// Keys usable for overview jump codes, most ergonomic first. Excludes
/// the overview's own bindings (h j k l o q) so a code can never collide
/// with navigation.
pub const JUMP_KEYS: &[char] = &[
    'a', 's', 'd', 'f', 'g', 'e', 'r', 't', 'u', 'i', 'w', 'n', 'm', 'c', 'v', 'b', 'x', 'z', 'y',
    'p',
];

/// Hint codes for `n` slides: single letters while they last, then
/// uniform two-letter codes, three past 400, and so on — always one
/// code per slide, always uniform length (so no prefix ambiguity).
///
/// ```
/// use deckhand::hints::jump_codes;
///
/// assert_eq!(jump_codes(3), ["a", "s", "d"]);
/// assert_eq!(jump_codes(25)[0], "aa"); // >20 slides: two-letter codes
/// assert_eq!(jump_codes(401).len(), 401); // >400: three letters
/// ```
pub fn jump_codes(n: usize) -> Vec<String> {
    let k = JUMP_KEYS.len();
    let mut len = 1usize;
    let mut cap = k;
    while cap < n {
        len += 1;
        cap = cap.saturating_mul(k);
    }
    (0..n)
        .map(|mut i| {
            let mut code = vec![' '; len];
            for slot in code.iter_mut().rev() {
                *slot = JUMP_KEYS[i % k];
                i /= k;
            }
            code.into_iter().collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_codes_are_unique_and_avoid_nav_keys() {
        for n in [1, 5, 20, 21, 100, 400, 401, 1000] {
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
        // the two-letter ordering matches the original nested-loop
        // enumeration: all of a?, then all of s?, …
        assert_eq!(jump_codes(25)[1], "as");
        assert_eq!(jump_codes(25)[20], "sa");
        // absurd decks get three letters instead of silently missing codes
        let codes = jump_codes(500);
        assert!(codes.iter().all(|c| c.chars().count() == 3));
        assert_eq!(codes[0], "aaa");
    }
}
