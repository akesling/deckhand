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
    version.ts         footer version picker
  _includes/llms.md    agent-facing site index (body of the two below)
  llms.njk             /llms.txt
  index-md.njk         /index.md — same index; the landing page is njk,
                       so it has no markdown source of its own
  llms-full.njk        /llms-full.txt — all content pages, concatenated
  mirrors.njk          <page>/index.md — raw-markdown mirror of each page
wasm/                  wasm-pack output (generated)
dist/                  bundled js (generated)
_site/                 final site (generated)
```

## Agent-facing pages

Following [llms.txt](https://llmstxt.org/): `/llms.txt` is a markdown
index of the site, every content page is also served raw at
`<page-url>index.md` (e.g. `/docs/index.md`, advertised via
`<link rel="alternate" type="text/markdown">`), and `/llms-full.txt`
concatenates them all for single-fetch consumption. The mirrors ship
each page's raw source (`rawInput`), so content pages must stay plain
markdown — no nunjucks tags (noted where the `pages` collection is
defined in `eleventy.config.js`).

## Deploy & versioning

Releases (tag pushes, cut by `scripts/release.sh`) trigger the Release
workflow, which builds the site and runs `scripts/deploy-site.sh`:
`wrangler pages deploy` to the Cloudflare Pages project — once to
production, and once under the tag's branch alias (`v0.1.0` →
`v0-1-0.deckhand.pages.dev`), which keeps that release's site up
forever.

`scripts/build-site.sh` bakes the crate version into the footer's
version picker and generates `versions.json` (all release tags plus the
version being built, each mapped to its URL). The picker (`version.ts`)
fetches the production copy of that manifest — CORS-opened via
`src/_headers` so old branch-alias deployments can read it too — and
jumps to the same path on whichever release you pick.

One-time Cloudflare setup: create the project with
`bunx wrangler pages project create deckhand --production-branch main`,
then give the GitHub repo `CLOUDFLARE_API_TOKEN` (Pages:Edit) and
`CLOUDFLARE_ACCOUNT_ID` secrets. `CLOUDFLARE_PAGES_PROJECT` and
`DECKHAND_SITE_URL` env vars override the project name and production
URL if they ever change.
