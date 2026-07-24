//! Generative round-trip property test: for any parseable deck,
//! `compile` must emit markdown that re-parses to an equivalent deck.
//!
//! Deck sources are built from a pool of adversarial lines (separator
//! lookalikes, fences, pipes, `???`) by a seeded PRNG, so every run
//! covers the same ~thousands of decks deterministically. This is the
//! executable form of compile's core promise; the hand-picked cases in
//! `src/compile.rs` cover states only manifests can produce (blank
//! slides, baked snapshots, title overrides).

use deckhand::compile::compile;
use deckhand::deck::{self, Deck, Segment, TermSnapshot};

/// Deterministic splitmix64 — no dependency, stable across platforms.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// Lines chosen to stress every splitter: column/depth separators,
/// notes markers, cell separators, special fences, fence-lookalikes
/// inside code, tables, and plain prose.
const LINES: &[&str] = &[
    "# a heading",
    "## smaller heading",
    "plain prose line",
    "**bold** and `code` and *stars*",
    "- a bullet",
    "  - a nested bullet",
    "> a quote",
    "",
    "",
    "---",
    "--",
    "-----",
    "***",
    "???",
    "a note-ish line",
    "| a | b |",
    "|---|---|",
    "| 1 | 2 |",
    "```rust",
    "let x: i32 = 1; // ---",
    "```",
    "```",
    "```terminal rows=5",
    "echo hi",
    "```terminal rows=fill",
    "```qr",
    "https://deckhand.sh",
    "```theme",
    "accent: red",
    "margin: 0",
    "````row",
    "````",
    "||",
    "|||",
    "````markdown",
    "```terminal rows=3",
    "not a real terminal, it's fenced",
    "````",
    "![diagram](arch.png)",
    "![inline](x.png) mid-sentence",
];

fn random_source(rng: &mut Rng) -> String {
    let lines = 1 + rng.below(40);
    let mut src = String::new();
    for _ in 0..lines {
        src.push_str(LINES[rng.below(LINES.len())]);
        src.push('\n');
    }
    src
}

/// A canonical view of a deck for equivalence: everything compile
/// promises to preserve. Markdown is compared whitespace-trimmed
/// (compile normalizes blank-line padding), themes by their YAML.
#[derive(Debug, PartialEq)]
enum Canon {
    Markdown(String),
    Terminal {
        command: Option<String>,
        rows: u16,
        fill: bool,
        snapshot: Option<(u16, u16, Vec<u8>)>,
    },
    Qr(String),
    Image {
        alt: String,
        path: String,
    },
    Row(Vec<Vec<Canon>>),
}

/// True when markdown renders as nothing: only whitespace and HTML
/// comments. Compile represents empty slides as `<!-- blank slide -->`
/// so they survive re-parse; presented content is identical.
fn invisible(src: &str) -> bool {
    let mut rest = src.trim();
    while let Some(open) = rest.find("<!--") {
        let Some(close) = rest[open..].find("-->") else {
            return false;
        };
        if !rest[..open].trim().is_empty() {
            return false;
        }
        rest = rest[open + close + 3..].trim();
    }
    rest.is_empty()
}

fn canon_segments(segments: &[Segment]) -> Vec<Canon> {
    segments
        .iter()
        .filter(|seg| !matches!(seg, Segment::Markdown(src) if invisible(src)))
        .map(|seg| match seg {
            Segment::Markdown(src) => Canon::Markdown(src.trim().to_string()),
            Segment::Terminal(b) => Canon::Terminal {
                command: b.command.clone(),
                rows: b.rows,
                fill: b.fill,
                snapshot: b
                    .snapshot
                    .as_ref()
                    .map(|s| (s.cols, s.rows, s.data.clone())),
            },
            Segment::Qr(q) => Canon::Qr(q.data.trim().to_string()),
            Segment::Image(img) => Canon::Image {
                alt: img.alt.clone(),
                path: img.path.clone(),
            },
            Segment::Row(cells) => Canon::Row(cells.iter().map(|c| canon_segments(c)).collect()),
        })
        .collect()
}

type CanonSlide = (String, Vec<Canon>, String, String);

fn canon_deck(deck: &Deck) -> Vec<Vec<CanonSlide>> {
    deck.columns
        .iter()
        .map(|col| {
            col.slides
                .iter()
                .map(|s| {
                    let theme = s
                        .theme
                        .as_ref()
                        .map(|t| serde_yaml::to_string(t).unwrap())
                        .unwrap_or_default();
                    (
                        s.title.clone(),
                        canon_segments(&s.segments),
                        s.notes.trim().to_string(),
                        theme,
                    )
                })
                .collect()
        })
        .collect()
}

#[test]
fn compiled_decks_reparse_equivalently() {
    let mut rng = Rng(0xdec0de);
    let mut parsed = 0usize;
    for i in 0..4000 {
        let src = random_source(&mut rng);
        // Random sources are often invalid (unclosed fences, empty qr,
        // one-cell rows) — parse errors are fine, silent divergence
        // after a successful parse is not.
        let Ok(mut original) = deck::parse(&src, "prop") else {
            continue;
        };
        parsed += 1;

        // Snapshots come from `--snapshots`, not from parsing; bolt
        // random ones on so their round-trip is exercised too.
        for block in original.terminals_mut() {
            if rng.chance(40) {
                block.snapshot = Some(TermSnapshot {
                    cols: 20 + rng.below(100) as u16,
                    rows: 4 + rng.below(30) as u16,
                    data: (0..rng.below(200)).map(|_| rng.next() as u8).collect(),
                });
            }
        }

        let (md, _rewrites) = compile(&original)
            .unwrap_or_else(|e| panic!("deck #{i} failed to compile: {e:#}\nsource:\n{src}"));
        let reparsed = deck::parse(&md, "prop").unwrap_or_else(|e| {
            panic!(
                "deck #{i} compiled to unparseable markdown: {e:#}\nsource:\n{src}\ncompiled:\n{md}"
            )
        });

        assert_eq!(
            canon_deck(&original),
            canon_deck(&reparsed),
            "deck #{i} diverged after compile\nsource:\n{src}\ncompiled:\n{md}"
        );

        // Compile is idempotent: compiling the reparsed deck emits the
        // same markdown (rewrites already applied on the first pass).
        let (md2, rewrites2) = compile(&reparsed).unwrap();
        assert_eq!(md, md2, "deck #{i} compile not stable\nsource:\n{src}");
        assert_eq!(rewrites2, 0, "deck #{i} still rewriting on 2nd pass");
    }
    // If the pool drifts toward all-invalid sources the test would
    // silently pass on nothing; keep it honest.
    assert!(parsed > 500, "only {parsed} of 4000 sources parsed");
}
