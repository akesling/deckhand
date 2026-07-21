# deckhand ⚓

Present slide decks as a TUI. One markdown file in, a full-screen terminal
presentation out — with **live embedded terminals**, a **2-D slide grid**,
a **bird's-eye overview**, and **presenter notes** that follow along in a
separate terminal.

Built in Rust on [ratatui](https://ratatui.rs).

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
- Terminals start when their slide is first shown, keep running while you
  navigate elsewhere, and die when deckhand exits.
- Each terminal keeps 10,000 lines of history: while focused,
  `shift-pgup`/`shift-pgdn` scroll it (half a page at a time), `shift-↑`/
  `shift-↓` go line by line, and typing anything snaps back to live.
- Once a terminal has history, a scrollbar appears on its right border —
  thumb sized to viewport-vs-history, accent-colored while focused.

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

## Keys

| key | action |
|-----|--------|
| `←`/`h`, `→`/`l` | previous / next column |
| `↓`/`j`, `↑`/`k` | deeper / shallower |
| `space`, `n` | next slide (walks every column depth-first) |
| `backspace`, `p` | previous slide |
| `g`, `G` | first / last column |
| `o` | overview grid (`hjkl` + `enter` to jump) |
| `1`-`9` | select a terminal pane (marked with `▸`) |
| `t`, `enter` | focus the selected terminal |
| `Ctrl-q` | release terminal focus |
| `shift-pgup`/`shift-pgdn` | scroll terminal history (also `shift-↑`/`shift-↓`) |
| `R` | restart this slide's terminals |
| `?` | help |
| `q` | quit |

While a terminal is focused, every key except `Ctrl-q` and the
`shift-`scroll keys goes to the PTY.

With several terminals on one slide, each shows its number in the title
and `▸` marks the one `t` will focus; the others' hint shows which number
selects them.

The mouse works too: **click** a terminal to select and focus it, click
anywhere else to release, **scroll-wheel** over a terminal to move through
its history, and in the overview click a slide to jump to it. (Mouse
events aren't forwarded into the PTY yet, so apps like htop stay
keyboard-driven.)

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

`deckhand compile` flattens a JSON deck into a single markdown file that
presents identically — handy for sharing a talk as one artifact:

```sh
deckhand compile deck.json -o talk.md
deckhand talk.md
```

The deck's title and deck-level theme are preserved as frontmatter.
Caveats: per-slide themes and per-slide `title` overrides have no
single-file syntax and are dropped with a warning; bare `---`/`--` lines
inside slide bodies are rewritten to `***` so they don't split slides on
re-parse.

## Theming

Layout, borders, and colors are configurable. Three sources, merged field
by field (each later one wins):

- `~/.config/deckhand/theme.json` (or `$XDG_CONFIG_HOME/deckhand/theme.json`)
  — your personal defaults, applies to every deck including markdown ones
  and the `deckhand notes` view
- a top-level `"theme"` object in a JSON deck manifest, or a `theme` block
  in a single-file deck's frontmatter — per-talk styling
- a `"theme"` object on an individual slide — per-slide overrides, scoped
  to that slide's content (margins, width, alignment, borders, markdown
  colors); the status bar and overview keep the deck theme. e.g. a
  full-bleed terminal slide:

  ```json
  { "terminal": { "command": "htop" },
    "theme": { "margin": 0, "max_width": 500 } }
  ```

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

Every field is optional. Colors take a name (`"cyan"`, `"light-blue"`),
hex (`"#rrggbb"`), or a 0-255 palette index. `border_type` is `plain`,
`rounded`, `double`, or `thick`; `vertical_align` is `center` or `top`;
`max_width`/`margin` are in terminal columns and `max_height` in rows
(default unlimited — it caps the content box, including fill terminals,
and the capped box still follows `vertical_align`).

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

## Current limitations

- No syntax highlighting in code blocks (yet)
- Slides taller than the window are clipped, not scrolled
- No images
- Unix only (PTYs + unix sockets)
