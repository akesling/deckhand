// Footer version picker. Each deployment is built knowing only its own
// version; the production site's versions.json (CORS-open, so the
// pages.dev branch aliases can read it too) lists every release.
// Picking one jumps to the same path on that release's deployment.

// Local previews (a static server over site/_site) read their own
// freshly generated manifest, so the picker is verifiable without
// deploying; deployed pages — branch aliases included — read
// production's, which lists every release.
const VERSIONS_URL = ["localhost", "127.0.0.1"].includes(location.hostname)
  ? "/versions.json"
  : "https://deckhand.sh/versions.json";

interface VersionEntry {
  version: string;
  url: string;
}

async function main() {
  const picker = document.getElementById("version-picker");
  if (!(picker instanceof HTMLSelectElement)) return;
  const current = picker.dataset.version;

  let versions: VersionEntry[];
  try {
    const res = await fetch(VERSIONS_URL);
    if (!res.ok) return;
    versions = ((await res.json()) as { versions: VersionEntry[] }).versions;
  } catch {
    return; // offline or blocked — the picker keeps its build-time entry
  }
  if (!Array.isArray(versions) || versions.length === 0) return;

  picker.replaceChildren(
    ...versions.map((v) => {
      const option = document.createElement("option");
      // The current version's option is a no-op, not a self-redirect.
      option.value = v.version === current ? "" : v.url;
      option.textContent = `v${v.version}`;
      option.selected = v.version === current;
      return option;
    }),
  );
  // A dev build (or a yanked release) isn't in the manifest — keep its
  // own label on top so the picker never lies about the page it's on.
  if (!versions.some((v) => v.version === current)) {
    const option = document.createElement("option");
    option.value = "";
    option.textContent = current === "dev" ? "dev" : `v${current}`;
    option.selected = true;
    picker.prepend(option);
  }

  picker.addEventListener("change", () => {
    if (!picker.value) return;
    location.href = new URL(location.pathname, picker.value).toString();
  });
}

main();
