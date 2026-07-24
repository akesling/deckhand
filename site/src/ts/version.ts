// Footer version picker. Versioned snapshots are served under
// /vX.Y.Z/ on the same origin as the latest site (/), so every page —
// any version, localhost previews of an assembled _site included —
// reads the one manifest at the site root and jumps by swapping the
// version prefix on the current path. No cross-origin, no
// implementation-detail hosts.
const VERSIONS_URL = "/versions.json";
// The prefix a versioned snapshot is served under; absent on latest.
const VERSION_PREFIX = /^\/v\d[^/]*(?=\/)/;

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
    // Same page, other version: swap the version prefix on the path.
    const page = location.pathname.replace(VERSION_PREFIX, "").replace(/^\//, "");
    location.href = new URL(page, new URL(picker.value, location.origin)).toString();
  });
}

main();
