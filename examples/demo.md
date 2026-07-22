---
title: deckhand demo
theme:
  border_type: rounded
---

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

```theme
vertical_align: top
h1: magenta
h2: light-magenta
```

a ```` ```theme ```` fence restyles **just this slide** — this one
pins content to the top and recolors its headings

with `vertical_align: top`, the title starts on the same row on
every slide, no matter how much follows it ↓

--

## the title stays put

```theme
vertical_align: top
h1: magenta
h2: light-magenta
```

same fence, much more content — but the heading didn't move

- headings are always bold; `h1` / `h2` set their colors
- set `vertical_align` deck-wide in frontmatter, per-user in
  `~/.config/deckhand/theme.json`, or per-slide like here
- `max_height` caps how far content grows downward
- also themeable: `accent`, borders, margins, code colors,
  `qr_dark` / `qr_light`

--

## …and floats without it

no fence on this slide, so it's back on the deck default:
content vertically centered

???

Flip between this slide and the two above to see the heading jump —
that's the difference vertical_align makes.

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

# fin

single markdown file · no browser · no cloud

**deckhand** — happy sailing ⚓
