---
layout: base.njk
title: docs
---

# Documentation

Everything deckhand does, in detail. For a gentler intro, start with
[getting started](/getting-started/).

## Deck formats

Decks come in two flavors that share all their powers: a **single
markdown file** with separators, or a **JSON manifest** that lays out
one markdown file per slide.

### Single-file decks

Two separators give the deck its two dimensions:

```markdown
# first topic          ← column 1, top

---

# second topic         ← column 2, top

--

## the weeds           ← column 2, one level deeper
```

- `---` on its own line starts a new **column** (earlier / later)
- `--` on its own line starts a **deeper** slide in the same column
- both are ignored inside fenced code blocks

A deck can open with **frontmatter** carrying a title and theme:

```markdown
---
title: my talk
theme:
  border_type: rounded
  accent: magenta
---
```

The leading block is only treated as frontmatter when it parses as a
YAML mapping — a bare `---` at the top still just separates columns.

### Presenter notes

Everything after a bare `???` line is that slide's presenter notes:
invisible when presenting, streamed to `deckhand notes`.

### Embedded terminals

A fenced code block whose info string is `terminal` becomes a live,
interactive PTY on the slide:

````markdown
```terminal rows=10
python3 -q
```
````

- the body runs via `$SHELL -c`; leave it **empty** for an interactive
  shell — the working directory is the deck's directory
- `rows=N` sets the viewport height (default 12, max 40); `rows=fill`
  expands to all remaining slide height
- **nothing executes without consent**: each terminal shows the command
  it *would* run until you approve it in-deck — <kbd>t</kbd> (or click),
  then <kbd>y</kbd>; <kbd>s</kbd> stops it again, <kbd>R</kbd> restarts
  it. `--eager` starts every terminal automatically when its slide first
  appears
- once running, terminals keep running while you navigate elsewhere,
  and die when deckhand exits
- each terminal keeps 10,000 lines of history with a scrollbar on its
  right border once there's something to scroll
- multiple terminal blocks on one slide stack vertically; fill-height
  ones split the leftover space evenly

### Syntax highlighting

Fenced code blocks with a language tag get lightweight highlighting —
keywords, strings, comments, numbers — for rust, python,
javascript/typescript, go, c/c++, sh, json, yaml, toml, and sql.
Unknown languages render plain. Colors come from the theme
(`code_keyword`, `code_string`, `code_comment`, `code_literal`); it's
a small built-in tokenizer, not a grammar engine, so the wasm
presenter stays light and the browser gets the same colors.

### QR codes

A fenced block whose info string is `qr` renders its body — a URL, some
text — as a scannable QR code:

````markdown
```qr
https://deckhand.sh
```
````

It draws black-on-white whatever the terminal palette, so phone cameras
actually read it. Works right here in the browser presenter, too — try
it in the [playground](/playground/).

### Images as ASCII art

A markdown image alone on its own line renders as colored ASCII art,
scaled to the slide and resolved against the deck's directory:

```markdown
![the architecture](diagrams/arch.png)
```

PNG, JPEG, GIF, and WebP. Images referenced mid-sentence stay inline
text, and the browser presenter shows a placeholder — pixels need a
filesystem.

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

### Per-slide themes

A `theme` fence anywhere on a slide overrides the theme for that slide
only (margins, width, borders, colors — the status bar keeps the deck
theme):

````markdown
```theme
margin: 0
accent: red
```
````

## JSON manifests

For bigger talks, point deckhand at a manifest: `deckhand deck.json`.

```json
{
  "title": "my talk",
  "theme": { "accent": "magenta" },
  "columns": [
    "slides/intro.md",
    ["slides/topic.md", "slides/topic-deep.md"],
    { "terminal": { "command": "htop" }, "title": "live demo" },
    { "file": "slides/fin.md", "notes_file": "slides/fin-notes.md" }
  ]
}
```

- `columns` is the presentation left-to-right; each entry is a
  **string** (single-slide column), an **array** (a column with depth,
  top first), or an **object**
- object slides take exactly one of `file`, `terminal`, or `panes`,
  plus optional `title`, `notes` (inline markdown), `notes_file`, and
  `theme`
- a `terminal` slide is one big PTY, filling the slide unless you give
  it `rows`
- `panes` stacks several panes vertically — any mix of markdown files
  and terminals:

```json
{ "panes": [
    "slides/context.md",
    { "terminal": { "command": "python3 -q", "rows": 8 } },
    { "terminal": {} }
] }
```

- paths are relative to the manifest; referenced markdown files are one
  slide each (no `---` splitting) but keep `???` notes and
  `terminal`/`theme` fences
- a slide file's own `theme` fence merges *under* the manifest's
  per-slide `theme` (manifest wins field by field)

## Theming

Layout, borders, and colors merge field by field from four sources,
each later one winning:

1. built-in defaults (solarized-safe indexed grays)
2. `~/.config/deckhand/theme.json` (or under `$XDG_CONFIG_HOME`) — your
   personal defaults, applies to every deck and the notes view
3. the deck's theme — manifest `"theme"` or markdown frontmatter
4. the slide's theme — manifest slide `"theme"` or a `theme` fence

All fields, all optional:

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
  "code_keyword": "magenta",
  "code_string": "green",
  "code_comment": 244,
  "code_literal": "yellow",
  "qr_dark": "#102040",
  "qr_light": "#fdf6e3"
}
```

Colors take a name (`"cyan"`, `"light-blue"`), hex (`"#rrggbb"`), or a
0–255 palette index. `qr_dark`/`qr_light` color QR blocks (modules /
background, quiet zone included), defaulting to true black on true
white — restyle at your own risk: scanners need a dark code on a light
background with strong contrast, so dark navy on cream scans fine but
an inverted or low-contrast pairing often won't. `snapshot_border` styles terminals showing a
baked-in snapshot, superseding `term_border` for those — set it in a
slide's `theme` fence to mark that slide's captures distinctly; unset,
snapshots use `term_border` like any terminal. `border_type` is
`plain`, `rounded`, `double`, or `thick`.

### Layout

One content box, two axes, the same three questions per axis — and
three rules: margins always win, caps bound everything drawn,
alignment only places what's left.

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
back to a normal H1 — try it on the title slide in the
[playground](/playground/).

> Tip: prefer palette indexes (16–255) or hex over ANSI names for
> grays. Schemes like solarized remap the 16 ANSI colors, turning ANSI
> "dark gray" nearly invisible — deckhand's own defaults use indexed
> grays for exactly this reason.

## Keys

| key | action |
|-----|--------|
| <kbd>←</kbd>/<kbd>h</kbd> <kbd>→</kbd>/<kbd>l</kbd> | previous / next column |
| <kbd>↓</kbd>/<kbd>j</kbd> <kbd>↑</kbd>/<kbd>k</kbd> | deeper / shallower |
| <kbd>space</kbd>, <kbd>n</kbd> | next slide (walks every column depth-first) |
| <kbd>shift-space</kbd>, <kbd>backspace</kbd>, <kbd>p</kbd> | previous slide |
| <kbd>g</kbd>, <kbd>G</kbd> | first / last column |
| <kbd>o</kbd> | overview — type a slide's jump code, or <kbd>hjkl</kbd> + <kbd>enter</kbd>, or click |
| <kbd>1</kbd>–<kbd>9</kbd> | select a terminal pane (marked with ▸) |
| <kbd>t</kbd>, <kbd>enter</kbd> | run (after <kbd>y</kbd>) / focus the selected terminal |
| <kbd>Ctrl-q</kbd> | release terminal focus |
| <kbd>shift-pgup</kbd>/<kbd>pgdn</kbd> | scroll terminal history (also <kbd>shift-↑</kbd>/<kbd>↓</kbd>) |
| <kbd>s</kbd> / <kbd>R</kbd> | stop / restart the selected terminal |
| <kbd>?</kbd> | help |
| <kbd>q</kbd> | quit |

While a terminal is focused, every key except <kbd>Ctrl-q</kbd> and the
shift-scroll keys goes to the PTY. The mouse works too: click a
terminal to focus it, click outside to release, scroll-wheel through
history, click a slide in the overview to jump.

On slides with several terminals, each pane shows its number and ▸
marks the one <kbd>t</kbd> will focus.

## Live reload

Local decks live-reload while presenting: deckhand watches the deck
file — and for manifests, every referenced slide and notes file, plus
your user theme — and hot-swaps the deck when anything changes. Your
position is kept (clamped if the deck shrank), live terminals survive
when the deck's terminal blocks are unchanged, and a mid-edit parse
error keeps the current deck with the error shown on the status bar.
Disable with `--no-watch`; remote decks aren't watched.

## Presenter notes over a socket

`deckhand present` listens on a unix socket (default
`$TMPDIR/deckhand.sock`) and broadcasts the current slide, its notes,
the next slide's title, and a talk timer. `deckhand notes` renders that
from any other terminal and reconnects automatically.

Running two decks at once? Give each a socket:

```sh
deckhand present talk.md --socket /tmp/talk.sock
deckhand notes --socket /tmp/talk.sock
```

## Presenting from URLs and gists

Any deck argument can be a URL:

```sh
deckhand https://example.com/talks/deck.md
deckhand https://gist.github.com/you/abc123
```

- plain URLs fetch that file; remote manifests fetch their referenced
  paths relative to the manifest's URL
- gist page URLs resolve through the GitHub API; the deck file is
  picked by priority: `deck.json`, `deck.md`, the only file, first
  `.md` — so a gist can hold a whole multi-file deck
- embedded terminals in remote decks run in your current directory

The [play page](/play/) does the same resolution in your browser.

## Compiling to one file

`deckhand compile` flattens any deck — including manifests and remote
decks — into a single markdown file that presents identically:

```sh
deckhand compile deck.json -o talk.md
deckhand compile https://gist.github.com/you/abc123 -o talk.md
```

The deck title and deck theme are preserved as frontmatter, per-slide
themes as `theme` fences. Only per-slide `title` overrides have no
single-file syntax; bare `---`/`--` lines inside slide bodies are
rewritten to `***` so they can't split slides on re-parse.

### Snapshots: real terminal output on the web

If a deck has terminal blocks, compiling requires choosing
`--snapshots` or `--no-snapshots` — capturing *executes the deck's
commands* on your machine, so deckhand won't guess. With `--snapshots`,
each block runs in a real PTY, output settles
(`--snapshot-wait-ms`, default 1500), and the screen is captured into
the compiled file at `--snapshot-cols` width (default 80).
`--snapshot-root` picks the working directory captures run in — it
controls what shell prompts display — defaulting to the deck's
directory. The capture lives inside the terminal fence as a
`%%snapshot COLSxROWS` marker plus base64-encoded ANSI.

Contexts that can't spawn PTYs — this site's presenter, the
[playground](/playground/), [play](/play/) — replay the capture through
the same terminal emulator the native TUI uses, colors and all, marked
with a `snapshot` hint on the border. Presenting natively always runs
the command live and ignores the snapshot.

## Architecture: presenters and providers

The presentation core (navigation, layout, theming, drawing) is
platform-independent; each context plugs in a **terminal provider**
that supplies the live innards of terminal blocks. The native TUI's
provider spawns real PTYs; the browser build on this site provides
placeholders. The `TerminalProvider` trait is public — a provider
could just as well run commands over ssh, in a container, or replay a
recording.

## Limitations

- slides taller than the window are clipped, not scrolled
- images render as ASCII art, and only when presenting natively — the
  browser presenter shows a placeholder
- native presenting is unix-only (PTYs + unix sockets)
- the browser presenter can't run terminals — that's what the real one
  is for
