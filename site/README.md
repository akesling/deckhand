# deckhand.sh (the site)

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
`v0-1-0.deckhand-sh.pages.dev`), which keeps that release's site up
forever.

`scripts/build-site.sh` bakes the crate version into the footer's
version picker and generates `versions.json` (all release tags, plus
any backfilled versions listed in `versions-known.txt`, plus the
version being built, each mapped to its URL). The picker
(`version.ts`) fetches the production copy of that manifest —
CORS-opened via `src/_headers` so old branch-alias deployments can
read it too — and jumps to the same path on whichever release you
pick.

Versioned docs only accumulate through that flow: the picker lists
**git tags**, and each tag's docs exist only because the Release
workflow deployed its branch alias. Site versions deployed without a
tag (manual `wrangler` runs) are invisible to the picker.

### Backfilling a version that predates tagging

0.1.0 and 0.2.0 were deployed manually (their `v0-1-0`/`v0-2-0`
aliases exist), so they're listed via `versions-known.txt` directly.
If an old version's alias is ever *missing*, rebuild and deploy it
after the fact — without pushing an old tag, which would run that
tag's own Release workflow:

```sh
git worktree add /tmp/deckhand-0.2.0 <the release's commit>
(cd /tmp/deckhand-0.2.0 && ./scripts/build-site.sh)
# deploy only the permanent alias; production stays untouched
(cd /tmp/deckhand-0.2.0/site && bunx wrangler pages deploy _site \
  --project-name deckhand-sh --branch v0.2.0 --commit-dirty=true)
git worktree remove /tmp/deckhand-0.2.0
```

Then add `0.2.0` to `site/versions-known.txt` on main — the next
production deploy's `versions.json` will list it. (Safety nets:
`deploy-site.sh --alias-only` does an alias-only deploy of the
current build, and its production step refuses tags older than the
newest known `v*` tag, so a stray old-tag run can't clobber the live
site.)

One-time Cloudflare setup: create a custom API token scoped to
**Account → Cloudflare Pages → Edit** only (don't use `wrangler login`
— its OAuth grant is account-wide and not narrowable), create the
project with
`bunx wrangler pages project create deckhand-sh --production-branch main`,
then give the GitHub repo `CLOUDFLARE_API_TOKEN` and
`CLOUDFLARE_ACCOUNT_ID` secrets. The same two env vars drive local
deploys. `CLOUDFLARE_PAGES_PROJECT` and `DECKHAND_SITE_URL` override
the project name and production URL if they ever change.
