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

# fin

single markdown file · no browser · no cloud

**deckhand** — happy sailing ⚓
