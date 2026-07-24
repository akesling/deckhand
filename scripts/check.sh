#!/usr/bin/env bash
# Everything CI would want: all lints, then tests and type checks.
set -euo pipefail
# Tool homes for minimal shells (CI, cron, editors).
export PATH="${HOME}/.cargo/bin:${HOME}/.bun/bin:/opt/homebrew/bin:${PATH}"
_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${_root}" || exit 1

"${_root}/scripts/lint.sh"

# Warnings are errors here too: RUSTFLAGS covers unit/integration test
# compilation, RUSTDOCFLAGS covers doctests and the docs build.
echo "== cargo test (unit, integration, doctests)"
RUSTFLAGS="-D warnings" RUSTDOCFLAGS="-D warnings" cargo test --locked

echo "== cargo doc"
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items --quiet

# The site's version manifest, generated and validated without
# deploying anything — a stale or incomplete versions.json fails here.
echo "== versions manifest"
_tmp="$(mktemp -d)"
"${_root}/scripts/gen-versions.sh" "${_tmp}/versions.json"
rm -rf "${_tmp}"

# The TS sources import the generated wasm bindings, so the check only
# means something once those exist. CI runs it in the site job (via
# build-site.sh), which builds the wasm first.
if command -v bun >/dev/null \
  && [ -d "${_root}/site/node_modules" ] \
  && [ -f "${_root}/site/wasm/deckhand.d.ts" ]; then
  echo "== typescript check"
  (cd "${_root}/site" && bunx tsc --noEmit)
else
  echo "== typescript check skipped (needs bun, site/node_modules, and" \
    "site/wasm bindings — build with scripts/build-wasm.sh)"
fi

echo "all checks passed"
