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
center together as one block

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
```

# fin

single markdown file · no browser · no cloud

**deckhand** — happy sailing ⚓
