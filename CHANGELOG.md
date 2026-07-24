# Changelog

Notable changes to deckhand. Unreleased items land in the next
version; for anything older than 0.2.0, the
[git history](https://github.com/akesling/deckhand/commits/main) is
the record.

## Unreleased

### Added

- **Slides taller than the window scroll.** `pgdn`/`pgup` reveal the
  rest of an overflowing slide before advancing, the mouse wheel
  scrolls off-terminal, and muted `▲`/`▼` corner markers say there's
  more. Previously tall slides were silently clipped.
- **PDF export** — `deckhand compile talk.md -o talk.pdf`: one page
  per slide through the same layout engine that presents, with real
  selectable text in an embedded subsetted DejaVu Sans Mono, vector
  box-drawing/QR paths, and slide-title bookmarks.
- **HTML export** — `deckhand compile talk.md -o talk.html`: a single
  self-contained static page, each slide a `<pre>` of styled text,
  with the same 2-D keyboard navigation, fullscreen, fit-to-window
  scaling, and a print stylesheet.
- **Slide search** — `/` opens an incremental finder over titles,
  body, notes, and terminal commands.
- **Prebuilt binaries** for macOS (arm64/x86_64) and Linux
  (x86_64/arm64) attach to every GitHub release;
  `curl -fsSL https://deckhand.sh/install.sh | sh` installs the
  latest.

### Changed

- `deckhand compile` to markdown now requires the
  `--snapshots`/`--no-snapshots` choice only for terminal blocks
  *without* baked snapshots (matching PDF/HTML) — an
  already-snapshotted deck recompiles without a flag.
- Snapshot capture ends as soon as an exited command's output
  settles; `--snapshot-wait-ms` is the cap, not a fixed sleep.
- A second presenter on an in-use notes socket now errors (pointing
  at `--socket`) instead of silently stealing the first one's socket
  file.

### Fixed

- A hung `deckhand notes` client can no longer freeze the presenter
  mid-slide-change: notes broadcasts moved off the render thread.
- Terminal commands that exit are reaped immediately instead of
  lingering as zombie processes until quit.
- Pasting into a focused terminal no longer redraws one frame per
  character.
- `deckhand <url>` times out against a hung server instead of
  hanging forever.
- Compile round-trip holes: a literal `||` line inside a row cell no
  longer splits the cell on re-parse, blank slides survive instead of
  being dropped, and `---` inside row cells is no longer needlessly
  rewritten. The round trip is now enforced by a generative
  4000-deck property test.
- Wide tables shrink to the slide width with `…`-marked truncation
  instead of walking off-screen.
- `--font-size 0`/negative/NaN no longer produces a broken PDF.
- Static slides no longer redraw at 30 fps; idle CPU is ~zero.

## 0.2.0

Embedded-terminal consent flow (`t` then `y`; `--eager` for the old
behavior), terminal scrollback and mouse passthrough, QR codes,
ASCII-art images, side-by-side `row` layout, syntax highlighting,
banner headings, snapshot baking, the wasm browser presenter, and the
deckhand.sh site.

## 0.1.0

First release.
