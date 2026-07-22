#!/usr/bin/env bash
# Local dev server with auto-reload on every kind of source change:
#   - Rust sources  → wasm rebuild (dev profile, polled watcher)
#   - site/src/ts   → bun build --watch
#   - pages/css     → 11ty --serve's own watcher + live reload
set -euo pipefail
# Tool homes for minimal shells (CI, cron, editors).
export PATH="${HOME}/.cargo/bin:${HOME}/.bun/bin:/opt/homebrew/bin:${PATH}"
_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
_site="${_root}/site"

command -v bun >/dev/null || {
  echo "bun not found — install from https://bun.sh" >&2
  exit 1
}

cd "${_site}" || exit 1
[ -d node_modules ] || bun install
mkdir -p dist/js

echo "[dev] initial wasm build…"
"${_root}/scripts/build-wasm.sh" --dev

echo "[dev] watching site/src/ts (bun build --watch)…"
bun run watch:ts &
_ts_pid=$!

# Poll the Rust sources; rebuild wasm when anything changes. Polling
# keeps this dependency-free (no fswatch/watchexec needed).
_watch_rust() {
  local _marker
  _marker="$(mktemp)"
  touch "${_marker}"
  while sleep 1; do
    if [ -n "$(find "${_root}/src" "${_root}/Cargo.toml" -newer "${_marker}" -print -quit 2>/dev/null)" ]; then
      touch "${_marker}"
      echo "[dev] rust change detected — rebuilding wasm…"
      if "${_root}/scripts/build-wasm.sh" --dev; then
        echo "[dev] wasm rebuilt"
      else
        echo "[dev] wasm build FAILED (fix and save again)" >&2
      fi
    fi
  done
}
echo "[dev] watching src/*.rs for wasm rebuilds…"
_watch_rust &
_rust_pid=$!

_cleanup() { kill "${_ts_pid}" "${_rust_pid}" 2>/dev/null || true; }
trap _cleanup EXIT INT TERM

# 11ty serves and live-reloads; wasm/dist changes are passthrough-copied.
bunx @11ty/eleventy --serve
