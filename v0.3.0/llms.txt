# deckhand

> deckhand presents markdown slide decks as a full-screen TUI: live
> embedded terminals inside slides, a 2-D slide grid (columns advance
> the talk, deeper slides hold the weeds), presenter notes in a second
> terminal, and a WebAssembly presenter that runs decks in the browser.

Decks are plain markdown — `---` on its own line starts a new column,
`--` a deeper slide in the same column — or a JSON manifest with one
markdown file per slide. Install: clone
https://github.com/akesling/deckhand and `cargo install --path .`, then
present with `deckhand your-talk.md`.

Every page below is served as raw markdown; the HTML version lives at
the same path without `index.md`.

## Docs

- [Getting started](/getting-started/index.md): install, write and present a first deck, embed a live terminal, use presenter notes
- [Documentation](/docs/index.md): the full reference — deck formats, separators, frontmatter, embedded terminals, images, themes, keybindings, CLI
- [Why deckhand](/why/index.md): the pitch — why the demo and the slides belong in one program
- [Full docs in one file](/llms-full.txt): all of the above, concatenated

## Optional

- [Playground](/playground/): edit and present a deck in the browser (needs JS)
- [Play](/play/): present a deck straight from a URL or GitHub gist (needs JS)
- [GitHub](https://github.com/akesling/deckhand): source, README, issues
- [docs.rs](https://docs.rs/deckhand): Rust API documentation for the deckhand crate
- [versions.json](/versions.json): released versions of this site and their URLs

