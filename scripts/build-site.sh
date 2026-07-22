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

_version="$(grep -m1 '^version' "${_root}/Cargo.toml" | cut -d'"' -f2)"

cd "${_site}" || exit 1
[ -d node_modules ] || bun install
# Type-check against the freshly generated wasm bindings — this is the
# one place the real deckhand.d.ts is guaranteed to exist (check.sh
# skips the TS check when it doesn't).
bun run check
mkdir -p dist/js
bun run build:ts
# The version labels the footer picker (eleventy.config.js reads it).
DECKHAND_VERSION="${_version}" bun run build:site

# versions.json: the manifest the footer picker fetches — every release
# tag plus the version being built, newest first. The newest entry
# points at production; older ones at the Cloudflare Pages branch
# aliases that deploy-site.sh publishes each release under.
echo "== versions.json"
_site_url="${DECKHAND_SITE_URL:-https://deckhand.sh}"
_project="${CLOUDFLARE_PAGES_PROJECT:-deckhand}"
_versions="$(
  {
    git -C "${_root}" tag -l 'v*' | sed 's/^v//'
    echo "${_version}"
  } | grep -v '^$' | sort -Vru
)"
_latest="$(echo "${_versions}" | head -n1)"
{
  printf '{\n  "latest": "%s",\n  "versions": [\n' "${_latest}"
  _first=true
  while IFS= read -r _v; do
    if [ "${_first}" = true ]; then
      _first=false
    else
      printf ',\n'
    fi
    if [ "${_v}" = "${_latest}" ]; then
      _url="${_site_url}"
    else
      # Cloudflare's branch-alias rule: lowercase, non-alphanumerics
      # collapsed to dashes, 28 chars max (mirrored in deploy-site.sh).
      _alias="$(printf 'v%s' "${_v}" | tr '[:upper:]' '[:lower:]' \
        | sed 's/[^a-z0-9]/-/g' | cut -c1-28)"
      _url="https://${_alias}.${_project}.pages.dev"
    fi
    printf '    { "version": "%s", "url": "%s" }' "${_v}" "${_url}"
  done <<<"${_versions}"
  printf '\n  ]\n}\n'
} >"${_site}/_site/versions.json"

echo "site built → site/_site/"
