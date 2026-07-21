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
- terminals spawn when their slide is first shown, keep running while
  you navigate elsewhere, and die when deckhand exits
- each terminal keeps 10,000 lines of history with a scrollbar on its
  right border once there's something to scroll
- multiple terminal blocks on one slide stack vertically; fill-height
  ones split the leftover space evenly

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
  "margin": 4,
  "vertical_align": "top",
  "border_type": "rounded",
  "accent": "magenta",
  "muted": 244,
  "term_border": "gray",
  "status_bg": "#3a3a3a",
  "status_fg": 250,
  "h1": "yellow",
  "h2": "light-yellow",
  "bullet": "magenta",
  "quote": "green",
  "link": "light-blue",
  "inline_code": "yellow",
  "code_bg": 235,
  "code_fg": 252
}
```

Colors take a name (`"cyan"`, `"light-blue"`), hex (`"#rrggbb"`), or a
0–255 palette index. `border_type` is `plain`, `rounded`, `double`, or
`thick`; `vertical_align` is `center` or `top`. `max_width` / `margin`
are terminal columns; `max_height` is rows and caps the content box —
including fill terminals.

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
| <kbd>t</kbd>, <kbd>enter</kbd> | focus the selected terminal |
| <kbd>Ctrl-q</kbd> | release terminal focus |
| <kbd>shift-pgup</kbd>/<kbd>pgdn</kbd> | scroll terminal history (also <kbd>shift-↑</kbd>/<kbd>↓</kbd>) |
| <kbd>R</kbd> | restart this slide's terminals |
| <kbd>?</kbd> | help |
| <kbd>q</kbd> | quit |

While a terminal is focused, every key except <kbd>Ctrl-q</kbd> and the
shift-scroll keys goes to the PTY. The mouse works too: click a
terminal to focus it, click outside to release, scroll-wheel through
history, click a slide in the overview to jump.

On slides with several terminals, each pane shows its number and ▸
marks the one <kbd>t</kbd> will focus.

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

## Architecture: presenters and providers

The presentation core (navigation, layout, theming, drawing) is
platform-independent; each context plugs in a **terminal provider**
that supplies the live innards of terminal blocks. The native TUI's
provider spawns real PTYs; the browser build on this site provides
placeholders. The `TerminalProvider` trait is public — a provider
could just as well run commands over ssh, in a container, or replay a
recording.

## Limitations

- no syntax highlighting in code blocks (yet)
- slides taller than the window are clipped, not scrolled
- no images
- native presenting is unix-only (PTYs + unix sockets)
- the browser presenter can't run terminals — that's what the real one
  is for
