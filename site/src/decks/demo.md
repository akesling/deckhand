---
title: deckhand demo
theme:
  border_type: rounded
---

# deckhand ⚓

a slide deck, in your terminal

*this demo is the real deckhand core, compiled to WebAssembly*

press `space` — or click here first, then press `space`

???

Hello from the presenter notes! In a real presentation these stream to a
second terminal running `deckhand notes`.

---

# navigation

slides live on a **2-D grid**

- `←` / `→` — move between columns (topics)
- `↓` / `↑` — dig deeper / resurface
- `space` — next, walking every column top-to-bottom
- `shift-space` — back
- `o` — bird's-eye overview with quick-jump codes

this column has more below it ↓

--

## deeper

columns are topics; depth is detail

keep the main thread shallow, bury the weeds down here for Q&A

--

## deepest

> if nobody asks, nobody sees it

`↑` to resurface, or `space` to keep walking

???

Only visible if someone digs. Try the overview with `o` from here.

---

# markdown

**bold**, *italic*, ~~strikethrough~~, `inline code`,
[links](https://github.com), and:

| feature   | status |
|-----------|:------:|
| tables    |   ✓    |
| terminals |   ✓    |
| themes    |   ✓    |

```rust
fn main() {
    println!("code blocks too");
}
```

---

# themes & layout

```theme
pin_title: true
h1: magenta
h2: light-magenta
```

a ```` ```theme ```` fence restyles **just this slide** — this one
recolors its headings and sets `pin_title: true`:

the title above is nailed to the top row, while this body floats,
**vertically centered** in the space below it ↓

--

## the title stays put

```theme
pin_title: true
h1: magenta
h2: light-magenta
```

same fence, much more content — the heading didn't move, the body
just grew around its own center

- headings are always bold; `h1` / `h2` set their colors
- `pin_title` anchors the heading only; `align_y: top` pins
  *everything* instead
- also themeable: `accent`, borders, margins, code colors,
  `qr_dark` / `qr_light`

--

## …and floats without it

no fence on this slide, so it's back on the deck default: title and
body centered together — flip `↑` / `↓` to watch the heading jump

---

# live terminals

in a real terminal, this box is a **running PTY** — htop, a REPL,
your build, whatever the talk needs:

```terminal rows=10
python3 -q
```

the browser can't spawn processes, so here it's a placeholder —
install deckhand to get the real thing

---

# try the overview

press `o` — every slide gets a short **jump code**

type a code to teleport; this is how you field
"wait, go back to the architecture slide"

---

# qr codes

````row
```qr
https://en.wikipedia.org/wiki/QR_code
```

*what these are*
||
```qr
https://deckhand.sh
```

*this very site*
````

a ```` ```qr ```` fence renders any URL or text, scannable right off
the screen — and a ```` ````row ```` fence lays any blocks out
side-by-side, cells split by `||`

---

# get it

```
git clone … && cargo install --path .
deckhand your-talk.md
```

plain markdown in · no browser required · presenter notes included

**deckhand** — happy sailing ⚓
