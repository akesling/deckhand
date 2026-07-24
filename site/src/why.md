---
layout: base.njk
title: why
---

# Why deckhand?

## Because your demo *is* the talk

Most technical talks orbit a terminal: a REPL, a build, a deploy, a
`curl`. The usual workflow — slides in one app, terminal in another,
cmd-tab in between, font size wrong, window in the wrong place — makes
the most important part of the talk the most fragile.

deckhand inverts that. Slides render *in* the terminal, and terminals
embed *in* the slides. A fenced block like

````markdown
```terminal rows=10
python3 -q
```
````

is a real PTY: the slide shows the command until you run it with a
keystroke — nothing executes without consent — and once running it
keeps going while you present other slides. Focus it (`t`), type into
it, scroll its history, restart it (`R`) if the demo goes sideways.
Kick off a long build on slide 3, come back to the result on slide 12.

## Because talks aren't linear

Questions don't arrive in slide order. deckhand decks are a **grid**:
columns are your topics (earlier / later), and each column can go
**deeper** (`--`) into detail you only show if someone asks. The main
thread of your talk stays tight; the weeds are one keystroke down.

The overview (`o`) shows the whole grid with short jump codes — type
`fd` and you're on the architecture slide, no arrow-key spelunking.

## Because plain text wins

A deck is a markdown file. It diffs, it reviews, it lives next to the
code it presents, and it renders fine on GitHub. There's no export step,
no browser, no cloud account. For bigger talks, a JSON manifest lays out
one markdown file per slide — and `deckhand compile` flattens it back to
one file you can paste into a gist.

Then anyone can run your deck with one command:

```sh
deckhand https://gist.github.com/you/abc123
```

And when someone insists on "just send the slides", `deckhand compile`
typesets the same deck to a PDF or a single static HTML page — same
layout engine, one page per slide, real selectable text.

## Because presenting well needs a second screen

`deckhand notes` in another terminal (a tmux pane, a second window, a
laptop over ssh) follows the presenter live: current slide, your private
notes, what's next, and a talk timer. Notes are just the part of each
slide after a `???` line — invisible to the audience.

## What deckhand is not

- **Not a pixel-perfect design tool.** It's a terminal; typography is
  monospace and proud of it.
- **Not a web framework.** The [browser presenter](/play/) exists for
  sharing and this site's demos, but the real thing — live PTYs
  included — runs where you already live: the terminal.

[Get started →](/getting-started/)
