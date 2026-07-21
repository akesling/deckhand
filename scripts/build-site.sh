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

cd "${_site}"
[ -d node_modules ] || bun install
mkdir -p dist/js
bun run build:ts
bun run build:site
echo "site built → site/_site/"
