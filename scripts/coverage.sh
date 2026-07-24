#!/bin/sh
# Line-coverage report via cargo-llvm-cov: an HTML report plus a
# terminal summary. One-time setup:
#
#   rustup component add llvm-tools-preview
#   cargo install cargo-llvm-cov --locked
#
# Arguments pass through to the test harness — CI runs with
# `--include-ignored` to fold in the PTY and unix-socket tests, which
# sandboxed local environments may not allow.
set -eu

_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "${_root}"

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  echo "cargo-llvm-cov is not installed; get it with:" >&2
  echo "  rustup component add llvm-tools-preview" >&2
  echo "  cargo install cargo-llvm-cov --locked" >&2
  exit 1
fi

cargo llvm-cov --locked --html -- "$@"
echo "HTML report: ${_root}/target/llvm-cov/html/index.html"
