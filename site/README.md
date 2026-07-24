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

Versioned docs live under **stable, same-origin paths**: the latest
release at `deckhand.sh/`, every other release at
`deckhand.sh/vX.Y.Z/`. No provider-specific hosts in any user-facing
URL. Snapshots are plain directories on the `site-archive` git branch
— first-class, inspectable state.

Releases (tag pushes, cut by `scripts/release.sh`) trigger the Release
workflow, which runs `scripts/deploy-site.sh` — always a clean cycle,
never a stale artifact:

1. build the release's snapshot with 11ty's `pathPrefix`
   (`HtmlBasePlugin` rewrites URLs so the site is self-consistent
   under `/vX.Y.Z/`) and commit it to `site-archive`;
2. build the latest site for the root;
3. assemble every archived version under its `/vX.Y.Z/` path;
4. verify every version `versions.json` lists is actually present —
   the deploy refuses to publish a picker that lies;
5. publish to production. Only the newest tag may touch production —
   an older tag's deploy refreshes its snapshot and stops.

`scripts/gen-versions.sh` builds and validates `versions.json` (all
release tags + `versions-known.txt` + the version being built, all
mapped to `/` or `/vX.Y.Z/`); `scripts/check.sh` runs that
generation-plus-validation on every check, so the manifest is
verifiable without deploying. The footer picker (`version.ts`)
fetches `/versions.json` — same origin everywhere, localhost previews
of an assembled `_site` included — and switches versions by swapping
the `/vX.Y.Z/` prefix on the current path.

### Backfilling a version that predates tagging

0.1.0 and 0.2.0 were deployed before tagging existed; their content
survives on old `*.pages.dev` branch-alias deployments. To bring one
under `deckhand.sh/vX.Y.Z/`: mirror it, rewrite its root-absolute
asset paths for the prefix, and commit it to `site-archive`:

```sh
git worktree add /tmp/archive site-archive
wget --mirror --no-host-directories \
  https://v0-2-0.deckhand-sh.pages.dev/ -P /tmp/archive/v0.2.0
find /tmp/archive/v0.2.0 \( -name '*.html' -o -name '*.js' \) \
  -exec sed -i '' \
    -e 's|href="/|href="/v0.2.0/|g' \
    -e 's|src="/|src="/v0.2.0/|g' \
    -e 's|"/wasm/|"/v0.2.0/wasm/|g' \
    -e 's|"/decks/|"/v0.2.0/decks/|g' {} +
(cd /tmp/archive && git add v0.2.0 && git commit -m "archive v0.2.0" \
  && git push origin HEAD:site-archive)
git worktree remove /tmp/archive
```

Then add `0.2.0` to `site/versions-known.txt` on main and redeploy.
Caveat: those old snapshots ship their era's picker script, which
lists versions but may not navigate — the content is what's being
preserved.

One-time Cloudflare setup: create a custom API token scoped to
**Account → Cloudflare Pages → Edit** only (don't use `wrangler login`
— its OAuth grant is account-wide and not narrowable), create the
project with
`bunx wrangler pages project create deckhand-sh --production-branch main`,
then give the GitHub repo `CLOUDFLARE_API_TOKEN` and
`CLOUDFLARE_ACCOUNT_ID` secrets. The same two env vars drive local
deploys. `CLOUDFLARE_PAGES_PROJECT` and `DECKHAND_SITE_URL` override
the project name and production URL if they ever change.
