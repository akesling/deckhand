---
layout: base.njk
title: getting started
---

# Getting started

## Install

deckhand is a single Rust binary. With a Rust toolchain installed:

```sh
git clone https://github.com/akesling/deckhand
cd deckhand
cargo install --path .
```

Try the bundled demo deck:

```sh
deckhand examples/demo.md
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
overview — every slide shows a short code; type it to jump there.

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

And flatten it back to one shareable file whenever you like:

```sh
deckhand compile deck.json -o talk.md
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
