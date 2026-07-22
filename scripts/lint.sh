#!/usr/bin/env bash
# All linters, no tests: rust (fmt + clippy for native and wasm), shell
# (shellcheck), and the site's TypeScript/JS (biome).
set -euo pipefail
# Tool homes for minimal shells (CI, cron, editors).
export PATH="${HOME}/.cargo/bin:${HOME}/.bun/bin:/opt/homebrew/bin:${PATH}"
_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${_root}" || exit 1

echo "== cargo fmt --check"
cargo fmt --check

echo "== cargo clippy (native)"
cargo clippy --locked --all-targets -- -D warnings

echo "== cargo clippy (wasm32)"
cargo clippy --locked --lib --target wasm32-unknown-unknown -- -D warnings

# --severity=style: every finding, including style nits, is an error.
# The site-vendored copy is preferred: it's version-pinned by bun.lock,
# so every environment lints with the same shellcheck. (The real binary,
# not the .bin shim — the shim needs node, which bun-only machines lack.)
_shellcheck="${_root}/site/node_modules/shellcheck/bin/shellcheck"
if [ -x "${_shellcheck}" ]; then
  echo "== shellcheck (vendored)"
  "${_shellcheck}" --severity=style scripts/*.sh
elif command -v shellcheck >/dev/null; then
  echo "== shellcheck (system)"
  shellcheck --severity=style scripts/*.sh
else
  echo "== shellcheck skipped (run: bun install --cwd site)"
fi

if command -v bun >/dev/null; then
  echo "== biome (site ts/js)"
  (
    cd "${_root}/site" || exit 1
    [ -d node_modules ] || bun install
    bun run lint
  )
else
  echo "== biome skipped (bun not installed)"
fi

echo "all lints passed"
