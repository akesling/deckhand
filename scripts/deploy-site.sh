#!/usr/bin/env bash
# Publish site/_site to Cloudflare Pages, twice: once under the
# release's branch alias (v0.1.0 → v0-1-0.<project>.pages.dev — the
# URLs versions.json points old releases at) and then to production.
# Build first with scripts/build-site.sh; this script only deploys.
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
  echo "usage: scripts/deploy-site.sh <tag>   (e.g. scripts/deploy-site.sh v0.1.0)" >&2
  exit 2
fi
if [ ! -d "${_site}/_site" ]; then
  echo "site/_site not found — run scripts/build-site.sh first" >&2
  exit 1
fi

_project="${CLOUDFLARE_PAGES_PROJECT:-deckhand-sh}"
cd "${_site}" || exit 1

# Cloudflare derives the alias subdomain from the branch name:
# lowercased, non-alphanumerics collapsed to dashes, 28 chars max.
_alias="$(printf '%s' "${_tag}" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9]/-/g' | cut -c1-28)"

echo "== deploying ${_tag} → https://${_alias}.${_project}.pages.dev"
bunx wrangler pages deploy _site --project-name "${_project}" \
  --branch "${_tag}" --commit-dirty=true

echo "== deploying ${_tag} → production"
bunx wrangler pages deploy _site --project-name "${_project}" \
  --branch main --commit-dirty=true

echo "site deployed ⚓"
