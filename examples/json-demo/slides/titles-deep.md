## the title stays put

more content this time — the heading didn't move, the body just grew
around its own center

- layout is one box, two axes, three questions: air (`margin_x` /
  `margin_y`), cap (`max_width` / `max_height`), and where leftover
  space goes (`align_x` / `align_y`)
- themes layer: user config, then the manifest's deck-wide `theme`,
  then per-slide entries — the intro and fin slides opt out with
  `"theme": { "pin_title": false }`
- the same keys work in a ```` ```theme ```` fence inside any slide
  file, if you'd rather keep style next to content
