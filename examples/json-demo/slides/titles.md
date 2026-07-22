# themes & layout

this slide is pinned to the top with recolored headings — and in a
manifest deck, the theme lives on the slide's entry in `deck.json`,
no fence needed:

```json
{ "file": "slides/titles.md",
  "theme": { "vertical_align": "top", "h1": "magenta" } }
```

with `vertical_align: top`, the title starts on the same row on
every slide, no matter how much follows it ↓
