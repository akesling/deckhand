#!/usr/bin/env bash
# Deploy the site to Cloudflare Pages with versioned docs on the
# production domain: the latest site at /, every released version
# under /vX.Y.Z/ — stable, first-class URLs, no provider-specific
# hosts. Snapshots are plain directories on the `site-archive` git
# branch; each deploy is a clean cycle:
#
#   1. build the release's snapshot (path-prefixed, self-consistent
#      under /<tag>/) and commit it to site-archive
#   2. build the latest site for the root
#   3. assemble every archived version under its /vX.Y.Z/ path
#   4. verify every version versions.json lists is actually present
#   5. publish to production (newest tag only — an older tag may only
#      refresh its snapshot, never replace the live site)
#
# Auth: CLOUDFLARE_API_TOKEN + CLOUDFLARE_ACCOUNT_ID, in CI (repo
# secrets) and locally alike. Use a custom API token scoped to
# Account → Cloudflare Pages → Edit — avoid `wrangler login`, whose
# OAuth grant is account-wide and not narrowable. One-time setup:
#   bunx wrangler pages project create <project> --production-branch main
set -euo pipefail
# Tool homes for minimal shells (CI, cron, editors).
export PATH="${HOME}/.cargo/bin:${HOME}/.bun/bin:/opt/homebrew/bin:${PATH}"
_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
_site="${_root}/site"

_tag="${1:-}"
if [ -z "${_tag}" ]; then
  echo "usage: scripts/deploy-site.sh <tag>   (e.g. scripts/deploy-site.sh v0.3.0)" >&2
  exit 2
fi

_project="${CLOUDFLARE_PAGES_PROJECT:-deckhand-sh}"
_archive="site-archive"
cd "${_root}"

# ------------------------------------------------ 1. snapshot → archive
echo "== snapshot build: self-consistent under /${_tag}/"
DECKHAND_PATH_PREFIX="/${_tag}/" "${_root}/scripts/build-site.sh"

echo "== archiving ${_tag} on the ${_archive} branch"
git fetch origin "${_archive}" 2>/dev/null || true
_wt="$(mktemp -d)/archive"
trap 'git worktree remove --force "${_wt}" >/dev/null 2>&1 || true' EXIT
if git show-ref --verify --quiet "refs/remotes/origin/${_archive}"; then
  git worktree add --detach "${_wt}" "origin/${_archive}"
else
  # First archive ever: an orphan history holding only snapshots.
  git worktree add --detach "${_wt}"
  git -C "${_wt}" checkout --orphan "${_archive}-init"
  git -C "${_wt}" rm -r -f -q . 2>/dev/null || true
fi
rm -rf "${_wt:?}/${_tag}"
mkdir -p "${_wt}/${_tag}"
cp -R "${_site}/_site/." "${_wt}/${_tag}/"
if [ -n "$(git -C "${_wt}" status --porcelain)" ]; then
  git -C "${_wt}" add -A
  git -C "${_wt}" -c user.name="deckhand release" \
    -c user.email="release@deckhand.sh" commit -q -m "archive ${_tag}"
  git -C "${_wt}" push origin "HEAD:refs/heads/${_archive}"
else
  echo "   snapshot unchanged — nothing to archive"
fi

# Older tags stop here: their snapshot is preserved, production is not
# theirs to replace.
_newest="$({
  git tag -l 'v*'
  echo "${_tag}"
} | sort -Vu | tail -n1)"
if [ "${_newest}" != "${_tag}" ]; then
  echo "== ${_tag} is older than ${_newest} — archived, skipping production"
  exit 0
fi

# ------------------------------------------- 2 + 3. root build, assemble
echo "== root build"
"${_root}/scripts/build-site.sh"

echo "== assembling versioned docs"
for _dir in "${_wt}"/v*/; do
  [ -d "${_dir}" ] || continue
  _v="$(basename "${_dir}")"
  rm -rf "${_site:?}/_site/${_v}"
  cp -R "${_dir%/}" "${_site}/_site/${_v}"
  echo "   /${_v}/"
done

# ------------------------------------------------- 4. consistency check
# Every version the manifest offers must actually be served; a listing
# without a site fails here, locally, before anything is published.
# shellcheck disable=SC2016 # the single-quoted script is JS, not shell
MANIFEST="${_site}/_site/versions.json" SITE="${_site}/_site" bun -e '
  const fs = require("fs");
  const m = JSON.parse(fs.readFileSync(process.env.MANIFEST, "utf8"));
  for (const v of m.versions) {
    const dir = v.url === "/" ? "" : v.url;
    if (!fs.existsSync(`${process.env.SITE}${dir}/index.html`)) {
      console.error(`versions.json lists ${v.version} but ${v.url} has no site`);
      process.exit(1);
    }
  }
  console.log(`   all ${m.versions.length} listed version(s) present`);
'

# ------------------------------------------------- 5. deploy production
echo "== deploying ${_tag} → production"
cd "${_site}"
bunx wrangler pages deploy _site --project-name "${_project}" \
  --branch main --commit-dirty=true

echo "site deployed ⚓"
