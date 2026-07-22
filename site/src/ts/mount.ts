// Mounts the wasm presenter into a DOM element via xterm.js.

import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import init, { WebDeck, bundle_to_markdown } from "../../wasm/deckhand.js";

let wasmReady: Promise<unknown> | null = null;

/** Load the wasm module once, shared across mounts. */
export function loadWasm(): Promise<unknown> {
  if (!wasmReady) {
    wasmReady = init({ module_or_path: "/wasm/deckhand_bg.wasm" });
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

  term.attachCustomKeyEventHandler((ev) => {
    if (ev.type !== "keydown") return false;
    // Leave browser shortcuts (cmd/ctrl combos) alone.
    if (ev.metaKey || ev.ctrlKey) return true;
    deck.key(ev.key, ev.ctrlKey, ev.shiftKey);
    paint();
    ev.preventDefault();
    return false;
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
