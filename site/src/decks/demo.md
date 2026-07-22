---
title: deckhand demo
theme:
  border_type: rounded
  pin_title: true
  title_gap: 2
  margin_x: 3
  margin_y: 1
---

```theme
pin_title: false
```

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

every title in this deck sits on the same row — set once, deck-wide,
in the frontmatter:

```yaml
theme:
  pin_title: true    # nail the heading to the top…
  title_gap: 2       # …with this much air under it
  margin_x: 3        # minimum air, left and right
  margin_y: 1        # and above / below
```

the body floats, **vertically centered**, in the space that's left ↓

--

## the title stays put

more content this time — the heading didn't move, the body just
grew around its own center

- layout is one box, two axes, three questions: air (`margin_x` /
  `margin_y`), cap (`max_width` / `max_height`), and where leftover
  space goes (`align_x` / `align_y`)
- headings are always bold; `h1` / `h2` set their colors
- override anything per-slide with a ```` ```theme ```` fence —
  the title slide of this deck opts out with `pin_title: false`

--

```theme
pin_title: false
```

## …and floats when you opt out

this slide's fence sets `pin_title: false`, so title and body
center together as one block — flip `↑` / `↓` to watch the
heading jump

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

```theme
pin_title: false
```

# get it

```
git clone … && cargo install --path .
deckhand your-talk.md
```

plain markdown in · no browser required · presenter notes included

**deckhand** — happy sailing ⚓
