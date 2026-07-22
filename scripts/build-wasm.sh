#!/usr/bin/env bash
# Build the deckhand wasm module into site/wasm/ using cargo +
# wasm-bindgen directly (no wasm-pack — its self-managed cache breaks
# under sandboxes). Pass --dev for a faster unoptimized build.
set -euo pipefail
# Tool homes for minimal shells (CI, cron, editors).
export PATH="${HOME}/.cargo/bin:${HOME}/.bun/bin:/opt/homebrew/bin:${PATH}"
_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${_root}" || exit 1

_profile=release
if [ "${1:-}" = "--dev" ]; then
  _profile=debug
fi

# The CLI version must match the wasm-bindgen crate in Cargo.lock, or the
# generated glue won't line up with the compiled module.
_required="$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | grep version | cut -d'"' -f2)"
command -v wasm-bindgen >/dev/null || {
  echo "wasm-bindgen not found — install with: cargo install wasm-bindgen-cli --version ${_required}" >&2
  exit 1
}
_have="$(wasm-bindgen --version | awk '{print $2}')"
if [ "${_have}" != "${_required}" ]; then
  echo "wasm-bindgen ${_have} doesn't match Cargo.lock (${_required})" >&2
  echo "fix with: cargo install wasm-bindgen-cli --version ${_required} --force" >&2
  exit 1
fi

if [ "${_profile}" = "release" ]; then
  cargo build --locked --lib --target wasm32-unknown-unknown --release
else
  cargo build --locked --lib --target wasm32-unknown-unknown
fi
wasm-bindgen "target/wasm32-unknown-unknown/${_profile}/deckhand.wasm" \
  --target web --out-dir site/wasm

# Optional shrink/speed pass when binaryen is installed (brew install binaryen).
if [ "${_profile}" = "release" ] && command -v wasm-opt >/dev/null; then
  wasm-opt -O2 site/wasm/deckhand_bg.wasm -o site/wasm/deckhand_bg.wasm
fi

echo "wasm built (${_profile}) → site/wasm/"
