## the title stays put

more content this time — the heading didn't move, and the body grew
downward instead of floating away from it

- layout is one box, two axes, three questions: air (`margin_x` /
  `margin_y`), cap (`max_width` / `max_height`), and where leftover
  space goes (`align_x` / `align_y`)
- `align_y: center` floats the body in the leftover space instead —
  better for sparse statement slides than text-heavy ones
- themes layer: user config, then the manifest's deck-wide `theme`,
  then per-slide entries — the intro and fin slides opt out with
  `"theme": { "pin_title": false, "align_y": "center" }`
