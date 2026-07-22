---
title: deckhand demo
theme:
  border_type: rounded
  pin_title: true
  title_gap: 2
  align_y: top
  margin_x: 3
  margin_y: 2
---

```theme
pin_title: false
align_y: center
h1_style: banner
```

# deckhand

⚓ a slide deck, in your terminal

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

every slide in this deck shares one shape — title on a fixed row,
body hanging just below it — set once, deck-wide, in the frontmatter:

```yaml
theme:
  pin_title: true   # carve the heading out…
  title_gap: 2      # …with this much air under it
  align_y: top      # body hangs from the title, no dead space
  margin_x: 3       # minimum air, left and right
  margin_y: 2       # and above / below
```

there's a longer slide below — watch what *doesn't* move ↓

--

## the title stays put

more content this time — the heading didn't move, and the body
grew downward instead of floating away from it

- layout is one box, two axes, three questions: air (`margin_x` /
  `margin_y`), cap (`max_width` / `max_height`), and where leftover
  space goes (`align_x` / `align_y`)
- `align_y: center` floats the body in the leftover space instead —
  better for sparse statement slides than text-heavy ones
- headings are always bold; `h1` / `h2` set their colors
- override anything per-slide with a ```` ```theme ```` fence, like
  the next slide down

--

```theme
pin_title: false
align_y: center
```

## …and floats when you opt out

this slide's fence sets `pin_title: false` and
`align_y: center` — title and body center together as one block,
the same statement-slide look as this deck's opening and fin — flip `↑` / `↓` to watch the
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
align_y: center
```

# get it

```
git clone … && cargo install --path .
deckhand your-talk.md
```

plain markdown in · no browser required · presenter notes included

**deckhand** — happy sailing ⚓
