#!/usr/bin/env bash
# Full production build of the site: wasm → ts bundle → 11ty, into site/_site/.
set -euo pipefail
# Tool homes for minimal shells (CI, cron, editors).
export PATH="${HOME}/.cargo/bin:${HOME}/.bun/bin:/opt/homebrew/bin:${PATH}"
_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
_site="${_root}/site"

command -v bun >/dev/null || {
  echo "bun not found — install from https://bun.sh" >&2
  exit 1
}

"${_root}/scripts/build-wasm.sh"

_version="$(grep -m1 '^version' "${_root}/Cargo.toml" | cut -d'"' -f2)"

cd "${_site}" || exit 1
[ -d node_modules ] || bun install
# Type-check against the freshly generated wasm bindings — this is the
# one place the real deckhand.d.ts is guaranteed to exist (check.sh
# skips the TS check when it doesn't).
bun run check
mkdir -p dist/js
bun run build:ts
# 11ty doesn't delete removed outputs — always build into a clean dir.
rm -rf _site
# The version labels the footer picker (eleventy.config.js reads it).
DECKHAND_VERSION="${_version}" bun run build:site

echo "== versions.json"
"${_root}/scripts/gen-versions.sh" "${_site}/_site/versions.json"

echo "site built → site/_site/"
