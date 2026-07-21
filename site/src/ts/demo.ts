// Landing-page demo: load the bundled demo deck into the wasm presenter.

import { WebDeck, bindCaption, loadWasm, mountDeck } from "./mount.js";

async function main() {
  const el = document.getElementById("demo-terminal");
  if (!el) return;
  try {
    const [source] = await Promise.all([
      fetch("/decks/demo.md").then((r) => r.text()),
      loadWasm(),
    ]);
    const onUpdate = bindCaption(document.getElementById("demo-caption"));
    const { term } = mountDeck(el, (cols, rows) => new WebDeck(source, "demo", cols, rows), {
      onUpdate,
    });
    // Focus on click so keys go to the deck only when the user opts in.
    el.addEventListener("click", () => term.focus());
  } catch (e) {
    el.textContent = `demo failed to load: ${e instanceof Error ? e.message : e}`;
  }
}

main();
