#!/usr/bin/env bash
# All linters, no tests: rust (fmt + clippy for native and wasm), shell
# (shellcheck), and the site's TypeScript/JS (biome).
set -euo pipefail
# Tool homes for minimal shells (CI, cron, editors).
export PATH="${HOME}/.cargo/bin:${HOME}/.bun/bin:/opt/homebrew/bin:${PATH}"
_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${_root}"

echo "== cargo fmt --check"
cargo fmt --check

echo "== cargo clippy (native)"
cargo clippy --all-targets -- -D warnings

echo "== cargo clippy (wasm32)"
cargo clippy --lib --target wasm32-unknown-unknown -- -D warnings

if command -v shellcheck >/dev/null; then
  echo "== shellcheck"
  # --severity=style: every finding, including style nits, is an error.
  shellcheck --severity=style scripts/*.sh
else
  echo "== shellcheck skipped (not installed — brew install shellcheck)"
fi

if command -v bun >/dev/null; then
  echo "== biome (site ts/js)"
  cd "${_root}/site"
  [ -d node_modules ] || bun install
  bun run lint
  cd "${_root}"
else
  echo "== biome skipped (bun not installed)"
fi

echo "all lints passed"
