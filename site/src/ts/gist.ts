// Resolve deck sources in the browser, mirroring the CLI's source.rs:
// plain URLs fetch directly; gist page URLs go through the GitHub API
// and pick the deck file by the same priority.

export interface Resolved {
  /** Entry file name (decides markdown vs manifest handling). */
  entry: string;
  /** File name → content, for manifest path resolution. */
  files: Record<string, string>;
}

const JSON_ENTRY = "deck.json";
const MD_ENTRY = "deck.md";

export function gistId(url: string): string | null {
  const m = url.match(/^https?:\/\/gist\.github\.com\/(?:[^/]+\/)?([0-9a-f]+)(?:\.git)?(?:[#?/].*)?$/i);
  return m ? m[1] : null;
}

/** Same priority as the CLI: deck.json, deck.md, sole file, first .md. */
export function pickEntry(names: string[]): string {
  if (names.includes(JSON_ENTRY)) return JSON_ENTRY;
  if (names.includes(MD_ENTRY)) return MD_ENTRY;
  if (names.length === 1) return names[0];
  const md = [...names].sort().find((n) => n.endsWith(".md"));
  if (md) return md;
  throw new Error(
    `can't tell which file is the deck (looked for deck.json, deck.md, or a .md file) among: ${names.join(", ")}`,
  );
}

async function fetchText(url: string): Promise<string> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`fetching ${url}: ${res.status} ${res.statusText}`);
  return res.text();
}

export async function resolveDeck(input: string): Promise<Resolved> {
  const url = input.trim();
  const id = gistId(url);
  if (id) {
    const res = await fetch(`https://api.github.com/gists/${id}`);
    if (!res.ok) throw new Error(`GitHub API: ${res.status} ${res.statusText}`);
    const gist = (await res.json()) as {
      files: Record<string, { raw_url: string; content?: string; truncated?: boolean }>;
    };
    const files: Record<string, string> = {};
    await Promise.all(
      Object.entries(gist.files).map(async ([name, meta]) => {
        files[name] =
          meta.content !== undefined && !meta.truncated
            ? meta.content
            : await fetchText(meta.raw_url);
      }),
    );
    return { entry: pickEntry(Object.keys(files)), files };
  }

  // Plain URL: fetch the file; if it's a manifest, fetch what it references.
  const clean = url.split(/[#?]/)[0];
  const slash = clean.lastIndexOf("/");
  if (slash < "https://".length) throw new Error(`${url}: not a valid deck URL`);
  const base = clean.slice(0, slash);
  const entry = clean.slice(slash + 1);
  if (entry.length === 0) throw new Error(`${url}: URL must point at a deck file`);

  const files: Record<string, string> = { [entry]: await fetchText(clean) };
  if (entry.endsWith(".json")) {
    for (const path of manifestPaths(files[entry])) {
      if (!(path in files)) {
        files[path] = await fetchText(`${base}/${path}`);
      }
    }
  }
  return { entry, files };
}

/** All file paths a manifest references (file / notes_file / panes). */
export function manifestPaths(manifestJson: string): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  const push = (p: unknown) => {
    if (typeof p === "string" && !seen.has(p)) {
      seen.add(p);
      out.push(p);
    }
  };
  const manifest = JSON.parse(manifestJson) as { columns: unknown[] };
  const slideSpec = (spec: unknown) => {
    if (typeof spec === "string") return push(spec);
    if (typeof spec !== "object" || spec === null) return;
    const s = spec as Record<string, unknown>;
    push(s.file);
    push(s.notes_file);
    if (Array.isArray(s.panes)) {
      for (const pane of s.panes) {
        if (typeof pane === "string") push(pane);
        else if (typeof pane === "object" && pane !== null) {
          push((pane as Record<string, unknown>).file);
        }
      }
    }
  };
  for (const col of manifest.columns ?? []) {
    if (Array.isArray(col)) col.forEach(slideSpec);
    else slideSpec(col);
  }
  return out;
}
