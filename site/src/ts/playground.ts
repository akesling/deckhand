// Playground: edit deck markdown on the left, present it live on the
// right. Rebuilds are debounced, keep your slide position, and a broken
// mid-edit deck leaves the last good one on screen.

import { resolveDeck } from "./gist.js";
import {
  type Mounted,
  WebDeck,
  bindCaption,
  bundle_to_markdown,
  loadWasm,
  mountDeck,
} from "./mount.js";

const STORAGE_KEY = "deckhand-playground";
const TOKEN_KEY = "deckhand-gh-token";

const DEFAULT_DECK = `---
title: playground
theme:
  border_type: rounded
  accent: magenta
---

# hello, playground ⚓

edit me on the left — the deck re-renders as you type

- \`---\` starts a new column
- \`--\` goes deeper
- click the terminal, then \`space\` / \`o\` to navigate

---

# markdown works

**bold**, *italic*, \`code\`, tables:

| thing  | works |
|--------|:-----:|
| tables |   ✓   |
| themes |   ✓   |

--

## and depth

this slide is *below* the previous one

???

Notes after ??? show up under the terminal.

---

# terminals

\`\`\`terminal rows=8
htop
\`\`\`

(a placeholder here — a real PTY when you present locally)
`;

async function main() {
  const editor = document.getElementById("pg-editor") as HTMLTextAreaElement | null;
  const stage = document.getElementById("pg-terminal");
  const status = document.getElementById("pg-status");
  if (!editor || !stage) return;

  const say = (msg: string, isError = false) => {
    if (!status) return;
    status.textContent = msg;
    status.classList.toggle("error", isError);
  };

  editor.value = localStorage.getItem(STORAGE_KEY) ?? DEFAULT_DECK;

  try {
    await loadWasm();
  } catch (e) {
    say(`wasm failed to load: ${e instanceof Error ? e.message : e}`, true);
    return;
  }

  const onUpdate = bindCaption(document.getElementById("pg-caption"));
  let mounted: Mounted;
  try {
    mounted = mountDeck(
      stage,
      (cols, rows) => new WebDeck(editor.value, "playground", cols, rows),
      { onUpdate },
    );
  } catch (e) {
    // Broken stored source: fall back to the default deck.
    editor.value = DEFAULT_DECK;
    mounted = mountDeck(
      stage,
      (cols, rows) => new WebDeck(editor.value, "playground", cols, rows),
      { onUpdate },
    );
  }
  const { term } = mounted;
  stage.addEventListener("click", () => term.focus());

  const rebuild = () => {
    const source = editor.value;
    try {
      const current = mounted.getDeck();
      const next = new WebDeck(source, "playground", term.cols, term.rows);
      next.goto(current.col(), current.row()); // clamps if the deck shrank
      mounted.setDeck(next);
      localStorage.setItem(STORAGE_KEY, source);
      say("");
    } catch (e) {
      // Keep presenting the last good deck; report why this one failed.
      say(e instanceof Error ? e.message : String(e), true);
    }
  };

  let timer: ReturnType<typeof setTimeout> | undefined;
  editor.addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(rebuild, 200);
  });

  document.getElementById("pg-reset")?.addEventListener("click", () => {
    editor.value = DEFAULT_DECK;
    localStorage.removeItem(STORAGE_KEY);
    rebuild();
  });

  // ------------------------------------------------------- gist tooling

  const sayLink = (prefix: string, url: string) => {
    if (!status) return;
    status.classList.remove("error");
    status.textContent = `${prefix} `;
    const a = document.createElement("a");
    a.href = url;
    a.target = "_blank";
    a.rel = "noopener";
    a.textContent = url;
    status.appendChild(a);
  };

  const gistUrl = document.getElementById("pg-gist-url") as HTMLInputElement | null;
  document.getElementById("pg-gist-load-form")?.addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const url = gistUrl?.value.trim();
    if (!url) return;
    say("loading gist…");
    try {
      const { entry, files } = await resolveDeck(url);
      // Manifest gists flatten to single-file markdown for the editor.
      editor.value = entry.endsWith(".json")
        ? bundle_to_markdown(entry, JSON.stringify(files))
        : files[entry];
      rebuild();
      say(`loaded ${entry}`);
    } catch (e) {
      say(e instanceof Error ? e.message : String(e), true);
    }
  });

  const tokenInput = document.getElementById("pg-gh-token") as HTMLInputElement | null;
  const remember = document.getElementById("pg-token-remember") as HTMLInputElement | null;
  if (tokenInput) {
    const saved = localStorage.getItem(TOKEN_KEY);
    if (saved) {
      tokenInput.value = saved;
      if (remember) remember.checked = true;
    }
  }

  document.getElementById("pg-gist-save-form")?.addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const token = tokenInput?.value.trim();
    if (!token) {
      say("saving needs a GitHub token with the gist scope (or use “copy markdown”)", true);
      return;
    }
    if (remember?.checked) localStorage.setItem(TOKEN_KEY, token);
    else localStorage.removeItem(TOKEN_KEY);
    say("creating gist…");
    try {
      const res = await fetch("https://api.github.com/gists", {
        method: "POST",
        headers: {
          Authorization: `Bearer ${token}`,
          Accept: "application/vnd.github+json",
        },
        body: JSON.stringify({
          description: "deckhand deck (deckhand playground)",
          public: false,
          files: { "deck.md": { content: editor.value } },
        }),
      });
      if (!res.ok) throw new Error(`GitHub API: ${res.status} ${res.statusText}`);
      const gist = (await res.json()) as { html_url: string };
      sayLink("saved —", gist.html_url);
      if (gistUrl) gistUrl.value = gist.html_url;
    } catch (e) {
      say(e instanceof Error ? e.message : String(e), true);
    }
  });

  document.getElementById("pg-copy")?.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(editor.value);
      say("markdown copied — paste it at gist.github.com/new");
      window.open("https://gist.github.com/new", "_blank", "noopener");
    } catch {
      say("clipboard unavailable — select the editor text and copy manually", true);
    }
  });
}

void main();
