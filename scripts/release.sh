#!/usr/bin/env bash
# Cut a release of the version already in Cargo.toml:
#   1. refuse unless on main, clean, and in sync with origin/main,
#      and the tag v<version> doesn't exist yet
#   2. build the site and run the full publish gate (check suite,
#      package verification, crates.io dry-run)
#   3. tag v<version> and push the tag — GitHub Actions (release.yml)
#      then builds the site and deploys it to Cloudflare Pages
#   4. publish to crates.io
#
# Steps 1–2 only by default. To actually release (3–4), pass
# --i-know-what-i-am-doing.
set -euo pipefail
# Tool homes for minimal shells (CI, cron, editors).
export PATH="${HOME}/.cargo/bin:${HOME}/.bun/bin:/opt/homebrew/bin:${PATH}"
_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${_root}" || exit 1

_for_real=false
case "${1:-}" in
  "") ;;
  --i-know-what-i-am-doing) _for_real=true ;;
  *)
    echo "usage: scripts/release.sh [--i-know-what-i-am-doing]" >&2
    echo "dry-runs by default; the flag tags, deploys, and publishes" >&2
    exit 2
    ;;
esac

_version="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
_tag="v${_version}"

if [ -n "$(git status --porcelain)" ]; then
  echo "working tree is dirty — commit or stash before releasing" >&2
  exit 1
fi
_branch="$(git rev-parse --abbrev-ref HEAD)"
if [ "${_branch}" != "main" ]; then
  echo "releases are cut from main (currently on '${_branch}')" >&2
  exit 1
fi

echo "== fetching origin"
git fetch origin main --tags
if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then
  echo "HEAD is not origin/main — push (or pull) first" >&2
  exit 1
fi
if git rev-parse --quiet --verify "refs/tags/${_tag}" >/dev/null; then
  echo "tag ${_tag} already exists — bump version in Cargo.toml first" >&2
  exit 1
fi

# The site build is a release gate too: a broken wasm/ts/11ty build
# should stop the release before anything is tagged or published.
echo "== building the site (release gate)"
"${_root}/scripts/build-site.sh"

# Full check suite, package verification, and crates.io dry-run.
"${_root}/scripts/publish.sh"

if [ "${_for_real}" = false ]; then
  echo "dry run for ${_tag} complete — nothing tagged or published"
  echo "to release for real: scripts/release.sh --i-know-what-i-am-doing"
  exit 0
fi

echo "== tagging ${_tag}"
git tag -a "${_tag}" -m "deckhand ${_tag}"
git push origin "${_tag}"
echo "   pushed — the Release workflow now deploys the site to" \
  "Cloudflare Pages (production + ${_tag} alias)"

echo "== publishing to crates.io"
"${_root}/scripts/publish.sh" --i-know-what-i-am-doing

echo "released deckhand ${_tag} ⚓ — site deploy is running in GitHub Actions"
