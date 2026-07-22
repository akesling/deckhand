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
