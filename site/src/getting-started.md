---
layout: base.njk
title: getting started
---

# Getting started

## Install

deckhand is a single binary. Grab a prebuilt one (macOS arm64/x86_64,
Linux x86_64/arm64):

```sh
curl -fsSL https://deckhand.sh/install.sh | sh
```

…or build from source with a Rust toolchain: `cargo install deckhand`.

Try the demo deck:

```sh
deckhand https://deckhand.sh/decks/demo.md
```

`space` advances, `?` shows every key, `q` quits.

## Your first deck

Create `talk.md`:

```markdown
---
title: my first deck
theme:
  border_type: rounded
---

# hello ⚓

this is a slide

---

# a second slide

- `---` starts a new column
- plain markdown: **bold**, `code`, tables, lists

--

## a deeper slide

`--` goes *deeper* in the same column — detail you only
show if someone asks. `↑` resurfaces.
```

Present it:

```sh
deckhand talk.md
```

Navigate with `space` / `shift-space` (walks everything), or steer
yourself: `←`/`→` between columns, `↓`/`↑` for depth. Press `o` for the
overview — every slide shows a short code; type it to jump there. Or
press `/` and type: it searches every slide's title, body, and notes,
and `enter` jumps to the pick.

## Add a live terminal

Put a real shell in a slide:

````markdown
# demo time

```terminal rows=10
python3 -q
```
````

The slide shows the command; press `t`, then `y` to run it — nothing
executes without your say-so. Once running, `t` focuses it (every key
goes to the terminal), `Ctrl-q` returns to the deck, `s` stops it, and
`R` restarts it if a demo goes sideways. Prefer the old behavior? Pass
`--eager` and terminals start the moment their slide appears. Leave the
body empty for an interactive shell, or use `rows=fill` to fill the
slide.

## Add a QR code — or two, side by side

A `qr` fence renders as a scannable code (black-on-white, whatever
your theme), and a `row` fence puts blocks next to each other, cells
split by `||`:

`````markdown
# find the slides later

````row
```qr
https://gist.github.com/you/abc123
```
||
```qr
https://en.wikipedia.org/wiki/QR_code
```
````
`````

Rows hold any blocks — markdown next to a terminal, code beside a QR.
And a markdown image alone on its own line (`![diagram](arch.png)`)
renders as colored ASCII art, resolved against the deck's directory.

## Add presenter notes

Everything after a bare `???` line is invisible to the audience:

```markdown
# pricing

it's complicated

???

Don't improvise here. Point at the table and move on.
```

In a second terminal (tmux pane, second window, ssh):

```sh
deckhand notes
```

It follows the presentation live: current notes, next slide, talk timer.

## Grow into a manifest

When one file gets unwieldy, lay the talk out in `deck.json` — one
markdown file per slide, terminals as whole slides, stacked panes,
per-slide themes:

```json
{
  "title": "my talk",
  "theme": { "accent": "magenta" },
  "columns": [
    "slides/intro.md",
    ["slides/topic.md", "slides/topic-deep.md"],
    { "terminal": { "command": "htop" }, "title": "live demo" }
  ]
}
```

```sh
deckhand deck.json
```

And flatten it back to one shareable file whenever you like — or
typeset it as a PDF or a self-contained web page, one slide per page,
through the same layout engine that presents it:

```sh
deckhand compile deck.json -o talk.md
deckhand compile deck.json -o talk.pdf
deckhand compile deck.json -o talk.html
```

## Share it as a gist

Put `talk.md` (or a whole manifest + slide files) in a GitHub gist, and
anyone can present it:

```sh
deckhand https://gist.github.com/you/abc123
```

…or load it in the browser on [the play page](/play/).

Next: the [full documentation](/docs/) covers every key, format detail,
and theme knob.
