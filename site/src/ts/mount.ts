// Mounts the wasm presenter into a DOM element via xterm.js.

import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import init, { WebDeck, bundle_to_markdown } from "../../wasm/deckhand.js";

let wasmReady: Promise<unknown> | null = null;

/** Load the wasm module once, shared across mounts. */
export function loadWasm(): Promise<unknown> {
  if (!wasmReady) {
    // Relative to this bundle ({prefix}/js/…), so versioned snapshots
    // served under /vX.Y.Z/ load their own wasm, not the latest one.
    wasmReady = init({
      module_or_path: new URL("../wasm/deckhand_bg.wasm", import.meta.url),
    });
  }
  return wasmReady;
}

export { WebDeck, bundle_to_markdown };

export interface MountOptions {
  /** Called after every state change, e.g. to display presenter notes. */
  onUpdate?: (deck: WebDeck) => void;
}

export interface Mounted {
  term: Terminal;
  getDeck(): WebDeck;
  /** Swap in a rebuilt deck (frees the old one) — used for live editing. */
  setDeck(next: WebDeck): void;
}

/** Resolve a semantic color token from site.css, so the terminals
    follow the page theme instead of hardcoding a palette. */
function cssColor(name: string, fallback: string): string {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

/** Attach a loaded WebDeck to an element and wire keys/resize. */
export function mountDeck(
  el: HTMLElement,
  makeDeck: (cols: number, rows: number) => WebDeck,
  opts: MountOptions = {},
): Mounted {
  const term = new Terminal({
    scrollback: 0,
    cursorBlink: false,
    disableStdin: false,
    fontFamily: "'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace",
    fontSize: 14,
    theme: {
      background: cssColor("--surface-terminal", "#101014"),
      foreground: cssColor("--fg", "#d0d0d0"),
      selectionBackground: cssColor("--accent", "#5fd7d7"),
      selectionForeground: cssColor("--fg-on-fill", "#0c0c0e"),
    },
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(el);
  // The WebGL renderer draws block/box glyphs itself (customGlyphs),
  // edge to edge — the DOM renderer takes them from the font, whose
  // half-blocks leave hairline seams between rows (visible on the QR
  // slide). Fall back to the DOM renderer where WebGL is unavailable.
  try {
    const webgl = new WebglAddon();
    webgl.onContextLoss(() => webgl.dispose());
    term.loadAddon(webgl);
  } catch {
    // DOM renderer it is.
  }
  fit.fit();

  let deck = makeDeck(term.cols, term.rows);
  const paint = () => {
    term.write(deck.render());
    opts.onUpdate?.(deck);
  };
  term.write("\x1b[?25l"); // the presenter has no cursor
  paint();

  // Fullscreen: a corner button (or the f key) fullscreens the frame;
  // the resize observer refits the grid on the way in and out, and Esc
  // exits via the browser. Skipped where the API is missing (iPhones).
  const toggleFullscreen = () => {
    if (document.fullscreenElement === el) {
      void document.exitFullscreen();
    } else {
      void el.requestFullscreen().then(() => {
        // Focus so keys work immediately — except on touch-primary
        // devices, where focusing would summon the platform keyboard.
        if (!window.matchMedia("(pointer: coarse)").matches) term.focus();
      });
    }
  };
  if (document.fullscreenEnabled) {
    const btn = document.createElement("button");
    btn.className = "fullscreen-toggle";
    btn.title = "fullscreen (f)";
    btn.setAttribute("aria-label", "toggle fullscreen");
    btn.textContent = "⛶";
    // Keep the click from bubbling into focus-on-click handlers.
    btn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      toggleFullscreen();
    });
    el.appendChild(btn);
  }

  term.attachCustomKeyEventHandler((ev) => {
    if (ev.type !== "keydown") return false;
    // Leave browser shortcuts (cmd/ctrl combos) alone.
    if (ev.metaKey || ev.ctrlKey) return true;
    if (ev.key === "f" && document.fullscreenEnabled) {
      toggleFullscreen();
      ev.preventDefault();
      return false;
    }
    deck.key(ev.key, ev.ctrlKey, ev.altKey, ev.shiftKey);
    paint();
    ev.preventDefault();
    return false;
  });

  // Wheel: offered to the deck first — over an embedded terminal it
  // scrolls that terminal's history (or feeds a nested TUI, which
  // gets mouse reports or arrow keys); anywhere else the page keeps
  // scrolling. Deltas accumulate so trackpads don't fire a tick per
  // pixel.
  let wheelAcc = 0;
  const cellAt = (ev: WheelEvent): { x: number; y: number } | null => {
    const screen = el.querySelector(".xterm-screen");
    if (!screen) return null;
    const r = screen.getBoundingClientRect();
    const x = Math.floor(((ev.clientX - r.left) / r.width) * term.cols);
    const y = Math.floor(((ev.clientY - r.top) / r.height) * term.rows);
    if (x < 0 || x >= term.cols || y < 0 || y >= term.rows) return null;
    return { x, y };
  };
  el.addEventListener(
    "wheel",
    (ev) => {
      const cell = cellAt(ev);
      if (!cell || !deck.terminal_at(cell.x, cell.y)) {
        wheelAcc = 0;
        return;
      }
      ev.preventDefault();
      // deltaMode: 0 = pixels, 1 = lines, 2 = pages.
      wheelAcc += ev.deltaY * (ev.deltaMode === 1 ? 16 : ev.deltaMode === 2 ? 160 : 1);
      const step = 48;
      let ticked = false;
      while (Math.abs(wheelAcc) >= step) {
        deck.wheel(cell.x, cell.y, wheelAcc < 0);
        wheelAcc += wheelAcc < 0 ? step : -step;
        ticked = true;
      }
      if (ticked) paint();
    },
    { passive: false },
  );

  // Touch: tap or swipe left advances, swipe right goes back — the
  // same linear walk as space/shift-space, so a phone reaches every
  // slide (columns and deeper rows) without a keyboard. Vertical
  // movement stays with the page (touch-action: pan-y in site.css),
  // so scrolling past the terminal still works.
  let touchStart: { x: number; y: number; at: number } | null = null;
  el.addEventListener(
    "touchstart",
    (ev) => {
      touchStart = null;
      if (ev.touches.length !== 1) return;
      const t = ev.touches[0];
      touchStart = { x: t.clientX, y: t.clientY, at: Date.now() };
    },
    { passive: true },
  );
  el.addEventListener("touchend", (ev) => {
    if (!touchStart) return;
    // Taps on the fullscreen button are the button's, not navigation.
    if (ev.target instanceof Element && ev.target.closest(".fullscreen-toggle")) return;
    const t = ev.changedTouches[0];
    const dx = t.clientX - touchStart.x;
    const dy = t.clientY - touchStart.y;
    const elapsed = Date.now() - touchStart.at;
    touchStart = null;
    const isSwipe = Math.abs(dx) >= 48 && Math.abs(dx) > 1.5 * Math.abs(dy);
    const isTap = Math.abs(dx) < 12 && Math.abs(dy) < 12 && elapsed < 350;
    if (!isSwipe && !isTap) return;
    deck.key(" ", false, false, isSwipe && dx > 0); // swipe right = back
    paint();
    // No synthetic click: a tap shouldn't focus the hidden textarea
    // and summon the platform keyboard.
    ev.preventDefault();
  });

  const refit = () => {
    const cols = term.cols;
    const rows = term.rows;
    fit.fit();
    // Only re-render when the grid actually changed — guards against
    // resize-observer feedback loops.
    if (term.cols === cols && term.rows === rows) return;
    deck.resize(term.cols, term.rows);
    term.write("\x1b[2J");
    paint();
  };
  new ResizeObserver(refit).observe(el);

  return {
    term,
    getDeck: () => deck,
    setDeck(next) {
      const old = deck;
      deck = next;
      deck.resize(term.cols, term.rows);
      term.write("\x1b[2J");
      paint();
      old.free();
    },
  };
}

/** Standard "current slide" caption under a mounted deck. */
export function bindCaption(captionEl: HTMLElement | null): (deck: WebDeck) => void {
  return (deck) => {
    if (!captionEl) return;
    const notes = deck.notes();
    const pos = deck.position();
    const title = deck.slide_title();
    captionEl.innerHTML = "";
    const meta = document.createElement("span");
    meta.className = "caption-meta";
    meta.textContent = `${pos} · ${title}`;
    captionEl.appendChild(meta);
    if (notes.trim().length > 0) {
      const n = document.createElement("span");
      n.className = "caption-notes";
      n.textContent = ` — presenter notes: ${notes.split("\n")[0]}`;
      captionEl.appendChild(n);
    }
  };
}
