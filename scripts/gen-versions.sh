#!/usr/bin/env bash
# Generate versions.json — the manifest the site footer's version
# picker fetches — and validate it before it can reach a deploy.
# Entries: every v* release tag, plus backfilled pre-tag versions from
# site/versions-known.txt, plus the crate version being built, newest
# first. The newest entry points at production; older ones at the
# permanent Cloudflare Pages branch alias each release deploys under.
#
#   scripts/gen-versions.sh <output-file>
set -euo pipefail
# Tool homes for minimal shells (CI, cron, editors).
export PATH="${HOME}/.cargo/bin:${HOME}/.bun/bin:/opt/homebrew/bin:${PATH}"
_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

_out="${1:-}"
if [ -z "${_out}" ]; then
  echo "usage: scripts/gen-versions.sh <output-file>" >&2
  exit 2
fi

_version="$(grep -m1 '^version' "${_root}/Cargo.toml" | cut -d'"' -f2)"
_site_url="${DECKHAND_SITE_URL:-https://deckhand.sh}"
_project="${CLOUDFLARE_PAGES_PROJECT:-deckhand-sh}"
_known="${_root}/site/versions-known.txt"

_versions="$(
  {
    git -C "${_root}" tag -l 'v*' | sed 's/^v//'
    if [ -f "${_known}" ]; then
      grep -v '^[[:space:]]*#' "${_known}" || true
    fi
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
} >"${_out}"

# Validate what was just written: parseable, newest-first, and
# complete — the current version and every versions-known.txt entry
# must be present. A stale or broken manifest fails here, locally,
# instead of surfacing as a wrong picker after a deploy.
if command -v bun >/dev/null; then
  # shellcheck disable=SC2016 # the single-quoted script is JS, not shell
  OUT="${_out}" VERSION="${_version}" KNOWN="${_known}" bun -e '
    const fs = require("fs");
    const fail = (msg) => {
      console.error(`versions.json invalid: ${msg}`);
      process.exit(1);
    };
    const m = JSON.parse(fs.readFileSync(process.env.OUT, "utf8"));
    const versions = m.versions.map((v) => v.version);
    if (!versions.includes(process.env.VERSION))
      fail(`missing the version being built (${process.env.VERSION})`);
    if (m.latest !== versions[0]) fail("latest is not the first entry");
    const known = fs.existsSync(process.env.KNOWN)
      ? fs.readFileSync(process.env.KNOWN, "utf8").split("\n")
          .map((l) => l.trim()).filter((l) => l && !l.startsWith("#"))
      : [];
    for (const k of known)
      if (!versions.includes(k)) fail(`missing known version ${k}`);
    for (const v of m.versions)
      if (!/^https:\/\/[a-z0-9.-]+$/.test(v.url)) fail(`bad url ${v.url}`);
    console.log(`   versions.json ok: ${versions.join(", ")}`);
  '
else
  echo "   bun not found — versions.json written but not validated" >&2
fi
