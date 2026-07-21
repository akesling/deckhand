#!/usr/bin/env bash
# Publish deckhand to crates.io without thinking about it:
#   1. refuse on a dirty tree, run the full check suite
#   2. stage a pristine clone of HEAD (cargo force-includes any
#      LICENSE/README-named file it can see — even gitignored ones like
#      site/node_modules — so packaging must happen where they don't exist)
#   3. verify the package contents and run cargo publish --dry-run
#
# That's all this does by default. To actually publish, pass
# --i-know-what-i-am-doing.
set -euo pipefail
# Tool homes for minimal shells (CI, cron, editors).
export PATH="${HOME}/.cargo/bin:${HOME}/.bun/bin:/opt/homebrew/bin:${PATH}"
_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${_root}"

_for_real=false
case "${1:-}" in
  "") ;;
  --i-know-what-i-am-doing) _for_real=true ;;
  *)
    echo "usage: scripts/publish.sh [--i-know-what-i-am-doing]" >&2
    echo "dry-runs by default; the flag performs the actual publish" >&2
    exit 2
    ;;
esac

if [ -n "$(git status --porcelain)" ]; then
  echo "working tree is dirty — commit or stash before publishing" >&2
  exit 1
fi
_branch="$(git rev-parse --abbrev-ref HEAD)"
if [ "${_branch}" != "main" ]; then
  echo "warning: publishing from branch '${_branch}', not main" >&2
fi

echo "== running the full check suite"
"${_root}/scripts/check.sh"

_stage="$(mktemp -d)"
_cleanup() { rm -rf "${_stage}"; }
trap _cleanup EXIT INT TERM

echo "== staging a pristine clone of HEAD"
git clone --quiet --no-hardlinks "${_root}" "${_stage}/deckhand"
cd "${_stage}/deckhand"

echo "== verifying package contents"
_list="$(cargo package --list --locked)"
if echo "${_list}" | grep -q node_modules; then
  echo "package would include node_modules files — aborting" >&2
  exit 1
fi
echo "${_list}" | sed 's/^/   /'
echo "   ($(echo "${_list}" | wc -l | tr -d ' ') files)"

echo "== cargo publish --dry-run"
cargo publish --dry-run --locked

_version="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
if [ "${_for_real}" = false ]; then
  echo "dry run of deckhand v${_version} complete — nothing published"
  echo "to publish for real: scripts/publish.sh --i-know-what-i-am-doing"
  exit 0
fi

cargo publish --locked
echo "published deckhand v${_version} ⚓"
