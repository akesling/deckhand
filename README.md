# deckhand ⚓

Present slide decks as a TUI. One markdown file in, a full-screen terminal
presentation out — with **live embedded terminals**, a **2-D slide grid**,
a **bird's-eye overview**, and **presenter notes** that follow along in a
separate terminal.

Built in Rust on [ratatui](https://ratatui.rs). The website (with a
WebAssembly-powered in-browser demo and gist loader) lives in
[`site/`](site/README.md).

## Quick start

```sh
cargo build --release
./target/release/deckhand examples/demo.md

# in a second terminal (optional): live presenter notes
./target/release/deckhand notes
```

## Deck format

Decks come in two flavors: a **single markdown file** with separators, or a
**JSON manifest** that lays out one markdown file per slide (see below).

A single-file deck uses two separators for its two dimensions:

```markdown
# first topic          ← column 1, top

---

# second topic         ← column 2, top

--

## the weeds           ← column 2, one level deeper
```

- `---` on its own line starts a new **column** (earlier / later)
- `--` on its own line starts a **deeper** slide in the same column
  (shallower / deeper — detail you only show if asked)

Both are ignored inside fenced code blocks.

Single-file decks can open with **frontmatter** carrying a title and a
theme (same schema as JSON manifests, see Theming):

```markdown
---
title: my talk
theme:
  border_type: rounded
  accent: magenta
---

# first slide
```

The leading block is only treated as frontmatter when it parses as a
YAML mapping — a bare `---` at the top of a deck still just separates
columns like it always did.

Individual slides take theme overrides via a `theme` fence anywhere on
the slide (works in single-file decks and in per-slide files referenced
from a manifest):

````markdown
# demo time

```theme
margin: 0
accent: red
```

```terminal rows=fill
htop
```
````

### Presenter notes

Everything after a bare `???` line is presenter notes for that slide:
invisible when presenting, streamed to `deckhand notes`.

```markdown
# pricing

it's complicated

???

Don't improvise here. Point at the table and move on.
```

### Embedded terminals

A fenced code block whose info string is `terminal` becomes a live,
interactive PTY on the slide:

````markdown
```terminal rows=10
python3 -q
```
````

- The block body is run via `$SHELL -c`; leave it **empty** for an
  interactive shell.
- `rows=N` sets the viewport height (default 12, max 40); `rows=fill`
  expands it to all remaining slide height.
- **Nothing executes without consent**: by default each terminal shows
  the command it *would* run, and you approve it in-deck — press `t`
  (or click), then `y`. `s` stops a terminal (back to its command
  preview), `R` restarts it. Pass `--eager` to start every terminal
  automatically when its slide first appears.
- Once running, terminals keep running while you navigate elsewhere,
  and die when deckhand exits.
- Each terminal keeps 10,000 lines of history: while focused,
  `shift-pgup`/`shift-pgdn` scroll it (half a page at a time), `shift-↑`/
  `shift-↓` go line by line, and typing anything snaps back to live.
- Once a terminal has history, a scrollbar appears on its right border —
  thumb sized to viewport-vs-history, accent-colored while focused.

### Syntax highlighting

Fenced code blocks with a language tag get lightweight highlighting —
keywords, strings, comments, numbers/constants, called functions, and
Capitalized types — for rust, python, javascript/typescript, go,
c/c++, sh, json, yaml, toml, and sql; unknown languages render plain.
It's a small built-in tokenizer (no grammar engine), so it works
identically in the browser presenter and keeps the wasm bundle small.
Colors are the theme's `code_keyword`, `code_string`, `code_comment`,
`code_literal`, `code_function`, and `code_type`, defaulting to a One
Dark-flavored truecolor palette (RGB, so terminal palettes can't
remap it).

### QR codes

A fenced block whose info string is `qr` renders its body — a URL, some
text — as a scannable QR code, drawn black-on-white regardless of the
terminal palette so phone cameras actually read it:

````markdown
```qr
https://deckhand.sh
```
````

Great for the "slides are at…" closer. Works in the browser presenter
too.

### Images as ASCII art

A markdown image alone on its own line renders as colored ASCII art,
scaled to the slide and resolved against the deck's directory:

```markdown
![the architecture](diagrams/arch.png)
```

PNG, JPEG, GIF, and WebP. Images referenced mid-sentence stay inline
text (`[image: alt]`), and the browser presenter shows a placeholder —
pixels need a filesystem.

### Side-by-side layout

A fenced block whose info string is `row` lays its cells out
side-by-side, splitting the slide width equally; `||` on its own line
separates cells. A cell stacks anything a slide can hold — markdown,
code blocks, terminals, QR codes, images — except another row. Make
the outer fence longer than any fence inside the cells:

`````markdown
````row
```qr
https://deckhand.sh
```
||
```terminal rows=10
python3 -q
```
````
`````

Cells are top-aligned and the row is as tall as its tallest cell.
`rows=fill` terminals inside a row fall back to their fixed height
(default 12) — "the rest of the slide" isn't a height a side-by-side
cell can claim.

## JSON decks

For bigger talks, point deckhand at a manifest instead:
`deckhand deck.json` (see `examples/json-demo/`).

```json
{
  "title": "my talk",
  "columns": [
    "slides/intro.md",
    ["slides/topic.md", "slides/topic-deep.md"],
    { "terminal": { "command": "htop" }, "title": "live demo" },
    { "file": "slides/fin.md", "notes_file": "slides/fin-notes.md" }
  ]
}
```

- `columns` is the presentation left-to-right; each entry is a **string**
  (single-slide column), an **array** (a column with depth, top first), or
  an **object**.
- Object slides take exactly one of `file` (markdown), `terminal`, or
  `panes` — plus optional `title`, `notes` (inline markdown),
  `notes_file`, and `theme` (per-slide overrides, see Theming).
- A `terminal` slide is one big PTY, filling the slide unless you give it
  `rows`.
- `panes` stacks several panes vertically on one slide — any mix of
  markdown files and terminals, so two or three stacked terminals is just:

  ```json
  { "panes": [
      "slides/context.md",
      { "terminal": { "command": "python3 -q", "rows": 8 } },
      { "terminal": {} }
  ] }
  ```

  Markdown panes contribute their `???` notes (concatenated) and the first
  one names the slide; fill-height terminals split the leftover space
  evenly. (In single-file markdown decks the same thing works by putting
  multiple ` ```terminal ` blocks on one slide.)
- Paths are relative to the manifest. Referenced markdown files are one
  slide each — no `---`/`--` splitting — but `???` notes and
  ` ```terminal ` blocks inside them still work.

## Live reload

Local decks **live-reload while presenting**: deckhand watches the deck
file — and for manifests, every referenced slide and notes file, plus
your user theme — and hot-swaps the deck when anything changes. Your
position is kept (clamped if the deck shrank), live terminals survive
when the deck's terminal blocks are unchanged, and a mid-edit parse
error keeps the current deck with the error on the status bar. Disable
with `--no-watch`. Remote decks aren't watched.

## Keys

| key | action |
|-----|--------|
| `←`/`h`, `→`/`l` | previous / next column |
| `↓`/`j`, `↑`/`k` | deeper / shallower |
| `space`, `n` | next slide (walks every column depth-first) |
| `shift-space`, `backspace`, `p` | previous slide |
| `g`, `G` | first / last column |
| `pgdn`, `pgup` | scroll a slide taller than the window, then advance |
| `o` | overview grid — each slide shows a short code; type it to jump (or `hjkl` + `enter`, or click) |
| `/` | search slides — type to filter (titles, body, notes, commands), `↓`/`↑` select, `enter` jumps, `esc` closes |
| `1`-`9` | select a terminal pane (marked with `▸`) |
| `t`, `enter` | run (after a `y` confirmation) / focus the selected terminal |
| `Ctrl-q` | release terminal focus |
| `shift-pgup`/`shift-pgdn` | scroll terminal history (also `shift-↑`/`shift-↓`) |
| `s` / `R` | stop / restart the selected terminal |
| `?` | help |
| `q` | quit |

While a terminal is focused, every key except `Ctrl-q` and the
`shift-`scroll keys goes to the PTY.

With several terminals on one slide, each shows its number in the title
and `▸` marks the one `t` will focus; the others' hint shows which number
selects them.

The mouse works too: **click** a terminal to select and focus it (apps
that asked for the mouse get the click), click anywhere else to
release, and in the overview click a slide to jump to it. The
**scroll-wheel** over a terminal reaches whatever wants it: mouse-aware
apps (htop) get real wheel events, full-screen apps without mouse
reporting (less, vim) get arrow keys, and plain shells scroll
deckhand's own history view. Off-terminal, the wheel scrolls a slide
taller than the window (`▲`/`▼` in the corners mark clipped content).

## Presenting from a URL or gist

Any deck argument can be a URL:

```sh
deckhand https://example.com/talks/deck.md
deckhand https://gist.github.com/alex/abc123
deckhand compile https://gist.github.com/alex/abc123 -o talk.md
```

- A plain URL fetches that file. JSON manifests work remotely too —
  their relative paths (slides, `notes_file`) fetch against the
  manifest's URL.
- A **gist page URL** is resolved through the GitHub API: deckhand looks
  at the gist's files and picks the deck by priority — `deck.json`, then
  `deck.md`, then the only file, then the first `.md`. A gist containing
  `deck.json` plus slide files works as a full multi-file deck (manifest
  paths resolve against the gist's filenames).
- Embedded terminals in remote decks run in your current directory.

### Compiling a manifest to one file

`deckhand compile` turns a deck into a single artifact. The default
format flattens a JSON deck into one markdown file that presents
identically — handy for sharing a talk (the output format follows
`-o`'s extension; see [Exporting](#exporting-pdf-and-html) for
`.pdf`/`.html`):

```sh
deckhand compile deck.json -o talk.md
deckhand talk.md
```

The deck's title and deck-level theme are preserved as frontmatter, and
per-slide themes as ` ```theme ` blocks. Caveats: per-slide `title`
overrides have no single-file syntax and fall back to what the content
implies; bare `---`/`--` lines inside slide bodies are rewritten to `***`
(and bare `||` lines inside row cells escaped) so they don't split
slides on re-parse.

### Baking in terminal snapshots

If the deck has terminal blocks without baked snapshots, compiling
requires an explicit choice — because capturing *executes the deck's
commands* on your machine, and skipping silently would lose their
output (a deck whose terminals are all already snapshotted needs no
flag):

```sh
deckhand compile deck.json -o talk.md --snapshots      # run + capture
deckhand compile deck.json -o talk.md --no-snapshots   # placeholders
# capture knobs: --snapshot-wait-ms 3000 --snapshot-cols 100
#                --snapshot-root ~/demo
```

With `--snapshots`, each block runs in a real PTY, output settles, and
the screen is captured into the compiled file. `--snapshot-root` sets
the working directory captures run in — handy for controlling what your
shell prompt shows — and defaults to the deck's directory, matching
live presenting. Decks without terminal blocks need no flag.

The capture is stored inside the terminal fence as a `%%snapshot
COLSxROWS` marker plus base64-encoded ANSI — so contexts that can't
spawn PTYs (the web presenter) replay the real, colored output instead
of showing a placeholder. Native presenting always runs the command
live and ignores snapshots.

## Exporting: PDF and HTML

`deckhand compile` also renders a deck through the same layout engine
that presents it — theming, banner headings, QR codes, and ASCII-art
images included — to a **PDF** (one page per slide) or a **single
self-contained static HTML page**. The `-o` extension picks the
format (or force one with `--to md|pdf|html`):

```sh
deckhand compile talk.md -o talk.pdf
deckhand compile talk.md -o talk.html
deckhand compile deck.json -o slides.pdf --cols 100 --rows 30 --font-size 12
```

Both formats:

- Slides are a terminal-cell grid (`--cols` × `--rows`, default
  100 × 30).
- Colors resolve like the web presenter: xterm defaults on a dark page.
- Terminal blocks render their baked snapshots (see
  [snapshots](#baking-in-terminal-snapshots)). A deck with
  unsnapshotted terminals requires `--snapshots` (runs the commands
  and captures their output) or `--no-snapshots` (placeholders).
- Each is a cargo feature (`pdf`, `html`), on by default; build with
  `--no-default-features` for a deckhand without them.

**PDF**: real, selectable text in an embedded, subsetted DejaVu Sans
Mono — box-drawing borders, block glyphs, and QR modules are drawn as
vector paths, so borders connect seamlessly and QR codes stay
scannable at any zoom. `--font-size` (points) scales the page around
the grid. Slide titles become PDF bookmarks: one entry per column,
deeper slides nested beneath it.

**HTML**: one file, no external assets, no wasm — each slide is a
`<pre>` of styled text. Arrow keys / hjkl / space walk the same 2-D
grid as presenting (the URL hash tracks `#col.row`), `f` goes
fullscreen, text scales to fit the window, and printing lays slides
out one per page.

## Theming

Layout, borders, and colors are configurable. Three sources, merged field
by field (each later one wins):

- `~/.config/deckhand/theme.json` (or `$XDG_CONFIG_HOME/deckhand/theme.json`)
  — your personal defaults, applies to every deck including markdown ones
  and the `deckhand notes` view
- a top-level `"theme"` object in a JSON deck manifest, or a `theme` block
  in a single-file deck's frontmatter — per-talk styling
- a `"theme"` object on an individual manifest slide, or a ` ```theme `
  block in a slide's markdown — per-slide overrides, scoped to that
  slide's content (margins, width, alignment, borders, markdown colors);
  the status bar and overview keep the deck theme. When both exist for
  one slide, the manifest's wins field by field. e.g. a full-bleed
  terminal slide:

  ```json
  { "terminal": { "command": "htop" },
    "theme": { "margin": 0, "max_width": 500 } }
  ```

```json
{
  "max_width": 84,
  "max_height": 30,
  "margin": 2,
  "margin_y": 1,
  "align_x": "center",
  "align_y": "top",
  "pin_title": true,
  "title_gap": 1,
  "border_type": "rounded",
  "accent": "magenta",
  "muted": 244,
  "term_border": "gray",
  "snapshot_border": "yellow",
  "status_bg": "#3a3a3a",
  "status_fg": 250,
  "h1_style": "banner",
  "h1": "yellow",
  "h2": "light-yellow",
  "bullet": "magenta",
  "quote": "green",
  "link": "light-blue",
  "inline_code": "yellow",
  "code_bg": 235,
  "code_fg": 252,
  "code_keyword": "#c678dd",
  "code_string": "#98c379",
  "code_comment": "#7f8490",
  "code_literal": "#d19a66",
  "code_function": "#61afef",
  "code_type": "#e5c07b",
  "qr_dark": "#102040",
  "qr_light": "#fdf6e3"
}
```

Every field is optional. Colors take a name (`"cyan"`, `"light-blue"`),
hex (`"#rrggbb"`), or a 0-255 palette index. `snapshot_border` styles
terminals that are showing a baked-in snapshot, superseding
`term_border` for those (unset = same as `term_border`). `border_type`
is `plain`, `rounded`, `double`, or `thick`.

**Layout** is one content box, two axes, the same three questions per
axis — and three rules: margins always win, caps bound everything
drawn, alignment only places what's left.

| axis | air (min) | cap (max) | leftover space |
|------|-----------|-----------|----------------|
| x | `margin_x` (default 2) | `max_width` (default 96) | `align_x`: left / **center** / right |
| y | `margin_y` (default 0) | `max_height` (default unlimited) | `align_y`: top / **center** / bottom |

`margin` sets both axes at once; the per-axis keys win over it. Units
are terminal cells (columns / rows). `max_height` caps everything the
slide draws — fill terminals and a pinned title included.

`pin_title` anchors a slide-leading heading to the top of the content
area (i.e. at row `margin_y`), with `title_gap` blank rows (default 1)
below it; the body lays out in the remaining space, still following
`align_y`. That's how a title sits on the same row on every slide
while the body stays centered.

`h1_style: "banner"` draws level-1 headings as large block glyphs,
three rows tall (A–Z, 0–9, light punctuation — case-insensitive). A
heading the font can't render, or that won't fit the width, falls
back to a normal H1. The demo decks use it on their title slides.

`qr_dark`/`qr_light` color QR blocks (modules / background, quiet zone
included) and default to true black on true white. Restyle at your own
risk: scanners need a dark code on a light background with strong
contrast — dark navy on cream scans fine, an inverted or low-contrast
pairing often won't.

Tip: prefer palette indexes (16-255) or hex over ANSI names for grays.
Schemes like solarized remap the 16 ANSI colors — ANSI "dark gray" becomes
nearly invisible against a solarized background, which is why deckhand's
own defaults use indexed grays.

## Presenter notes over a socket

`deckhand present` listens on a unix socket (default
`$TMPDIR/deckhand.sock`) and broadcasts the current slide, its notes, the
next slide's title, and a talk timer. `deckhand notes` renders that from
any other terminal — a second window, a tmux pane, or over ssh to the same
machine. Reconnects automatically.

Running two decks at once? Give each a socket:

```sh
deckhand present talk.md --socket /tmp/talk.sock
deckhand notes --socket /tmp/talk.sock
```

## Development

Repo tooling lives in `scripts/` (each self-contained and executable):

| script | does |
|--------|------|
| `scripts/lint.sh` | all linters: `cargo fmt --check`, clippy (native + wasm32), shellcheck, biome |
| `scripts/check.sh` | lint + `cargo test` + TypeScript type-check |
| `scripts/build-wasm.sh` | wasm module → `site/wasm/` (`--dev` for fast builds) |
| `scripts/build-site.sh` | full site build → `site/_site/` |
| `scripts/dev.sh` | dev server: 11ty live reload + `bun --watch` + wasm rebuild on Rust changes |
| `scripts/publish.sh` | crates.io publish from a pristine clone; dry-run by default (`--i-know-what-i-am-doing` to publish) |
| `scripts/deploy-site.sh` | deploy `site/_site/` to Cloudflare Pages (production + a permanent per-release alias) |
| `scripts/release.sh` | cut a release: checks + site build + dry-run, then tag `v<version>`, push (CI deploys the site), and publish to crates.io; dry-run by default |

CI (`.github/workflows/ci.yml`) runs the same gates on PRs and merges to
main, plus the PTY tests and an MSRV check; everything is pinned —
actions by commit SHA, the toolchain by `rust-toolchain.toml`, deps by
`Cargo.lock`/`bun.lock` (cargo runs `--locked`, bun `--frozen-lockfile`).
Dependabot PRs don't run CI automatically; trigger the workflow manually
after review.

## Current limitations

- Images render as ASCII art, and only when presenting natively — the
  browser presenter shows a placeholder
- Unix only (PTYs + unix sockets)
