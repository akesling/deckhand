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

if command -v bun >/dev/null && [ -d "${_root}/site/node_modules" ]; then
  echo "== typescript check"
  (cd "${_root}/site" && bunx tsc --noEmit)
else
  echo "== typescript check skipped (bun or site/node_modules missing)"
fi

echo "all checks passed"
