// The /play/ loader: fetch a URL- or gist-hosted deck and present it.

import { resolveDeck } from "./gist.js";
import { WebDeck, bindCaption, loadWasm, mountDeck } from "./mount.js";

const form = document.getElementById("play-form") as HTMLFormElement | null;
const input = document.getElementById("play-url") as HTMLInputElement | null;
const status = document.getElementById("play-status");
const stage = document.getElementById("play-terminal");

function say(msg: string, isError = false) {
  if (!status) return;
  status.textContent = msg;
  status.classList.toggle("error", isError);
}

async function load(url: string) {
  if (!stage) return;
  say("fetching deck…");
  try {
    await loadWasm();
    const { entry, files } = await resolveDeck(url);
    stage.innerHTML = "";
    const onUpdate = bindCaption(document.getElementById("play-caption"));
    const { term } = mountDeck(
      stage,
      (cols, rows) => WebDeck.from_bundle(entry, JSON.stringify(files), cols, rows),
      { onUpdate },
    );
    term.focus();
    stage.addEventListener("click", () => term.focus());
    say(`presenting ${entry} — keys go to the deck; space advances`);
    const params = new URLSearchParams(location.search);
    if (params.get("deck") !== url) {
      params.set("deck", url);
      history.replaceState(null, "", `?${params}`);
    }
  } catch (e) {
    say(e instanceof Error ? e.message : String(e), true);
  }
}

form?.addEventListener("submit", (ev) => {
  ev.preventDefault();
  const url = input?.value.trim();
  if (url) void load(url);
});

// Deep link: /play/?deck=<url>
const preset = new URLSearchParams(location.search).get("deck");
if (preset && input) {
  input.value = preset;
  void load(preset);
}
