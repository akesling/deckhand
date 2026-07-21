# deckhand.dev (the site)

Static site for deckhand: landing page with a live WebAssembly demo,
explainer, tutorial, docs, and an in-browser loader for URL/gist-hosted
decks.

Stack: [11ty](https://www.11ty.dev/) for pages, TypeScript bundled by
`bun build`, [xterm.js](https://xtermjs.org/) for the terminal, and the
deckhand crate itself compiled to wasm via wasm-bindgen (the `web`
module) — no framework components, just vanilla markup.

## Prerequisites

- [bun](https://bun.sh)
- Rust with the `wasm32-unknown-unknown` target
  (`rustup target add wasm32-unknown-unknown`)
- `wasm-bindgen-cli` matching the version in `Cargo.lock`
  (`cargo install wasm-bindgen-cli --version <see Cargo.lock>` — the
  build script checks and tells you the exact command)
- optional: [binaryen](https://github.com/WebAssembly/binaryen) for
  `wasm-opt` shrinking of release builds (`brew install binaryen`)

## Build

```sh
bun install
bun run build        # wasm → ts bundle → 11ty, output in _site/
```

During development:

```sh
bun run build:wasm   # once, and after Rust changes
bun run dev          # bundles ts + 11ty --serve with live reload
```

`bun run check` type-checks the TypeScript.

## Layout

```
src/
  _includes/base.njk   shared layout
  index.njk            landing page + wasm demo
  why.md               explainer
  getting-started.md   tutorial
  docs.md              full usage docs
  play.njk             URL/gist loader
  css/site.css
  decks/demo.md        the landing-page demo deck
  ts/
    mount.ts           xterm.js + wasm glue (shared)
    gist.ts            URL/gist resolution (mirrors src/source.rs)
    demo.ts            landing entry
    play.ts            loader entry
wasm/                  wasm-pack output (generated)
dist/                  bundled js (generated)
_site/                 final site (generated)
```
