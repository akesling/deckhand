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

a slide deck, in your terminal

*press `space` to begin — `?` for help*

???

Welcome! This first note is only visible in the notes view.
Run `deckhand notes` in another terminal to see it.

---

# navigation

slides live on a **2-D grid**

- `←` / `→` — move between columns (topics)
- `↓` / `↑` — dig deeper / resurface
- `space` — next slide, walking every column top-to-bottom
- `o` — bird's-eye overview of the whole grid

this column has more below it ↓

--

## deeper

each column is a topic; depth is detail

use shallow slides for the main thread of your talk,
and bury the weeds down here for Q&A

--

## deepest

> if nobody asks, nobody sees it

`↑` to resurface, or `space` to keep walking

???

Only dig down here if someone asks about the implementation.

---

# markdown

**bold**, *italic*, ~~strikethrough~~, `inline code`, and
[links](https://ratatui.rs)

> blockquotes for pull-quotes

| feature   | status |
|-----------|:------:|
| tables    |   ✓    |
| lists     |   ✓    |
| terminals |   ✓    |

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
the same statement-slide look as this deck's opening and fin

???

Flip between this slide and the two above to see the heading jump —
that's pin_title.

---

# live terminal

a real PTY, right in the slide — `t` to grab it, `Ctrl-q` to let go

```terminal rows=14
```

- [ ] run something interactive
- [x] look calm while doing it

???

Demo idea: ls, then start htop to show full-screen apps work.

---

# scripted terminal

blocks can start a command for you:

```terminal rows=10
python3 -q
```

`R` restarts the terminals on a slide if a demo goes sideways

???

Press t then y to start the REPL (or present with --eager to skip
the confirmation). Try 2**64 or import this.

---

# presenter notes

in another terminal:

```
deckhand notes
```

- follows the presenter over a unix socket
- shows notes, position, what's next, and a talk timer
- slides marked `???` keep their notes off the screen

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

*where this deck came from*
````

a ```` ```qr ```` fence renders any URL or text, scannable from the
audience — and a ```` ````row ```` fence puts any blocks side-by-side,
cells split by `||`. (`![alt](image.png)` on its own line renders as
ASCII art, too)

---

```theme
pin_title: false
align_y: center
```

# fin

single markdown file · no browser · no cloud

**deckhand** — happy sailing ⚓
