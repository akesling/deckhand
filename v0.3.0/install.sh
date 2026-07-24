#!/bin/sh
# deckhand installer — fetches the latest release binary for this
# platform from GitHub releases and installs it:
#
#   curl -fsSL https://deckhand.sh/install.sh | sh
#
# Installs to ~/.local/bin by default; override with
# DECKHAND_INSTALL_DIR. Prefer building from source? cargo install
# deckhand.
set -eu

_repo="akesling/deckhand"
_os="$(uname -s)"
_arch="$(uname -m)"

case "${_os}-${_arch}" in
Darwin-arm64) _target="aarch64-apple-darwin" ;;
Darwin-x86_64) _target="x86_64-apple-darwin" ;;
Linux-x86_64) _target="x86_64-unknown-linux-gnu" ;;
Linux-aarch64 | Linux-arm64) _target="aarch64-unknown-linux-gnu" ;;
*)
    echo "no prebuilt binary for ${_os}/${_arch} — try: cargo install deckhand" >&2
    exit 1
    ;;
esac

_url="https://github.com/${_repo}/releases/latest/download/deckhand-${_target}.tar.gz"
_bin_dir="${DECKHAND_INSTALL_DIR:-${HOME}/.local/bin}"
_tmp="$(mktemp -d)"
trap 'rm -rf "${_tmp}"' EXIT

echo "downloading ${_url}"
curl -fsSL "${_url}" | tar -xz -C "${_tmp}"
mkdir -p "${_bin_dir}"
install -m 755 "${_tmp}/deckhand" "${_bin_dir}/deckhand"

echo "installed $("${_bin_dir}/deckhand" --version) to ${_bin_dir}/deckhand"
case ":${PATH}:" in
*":${_bin_dir}:"*) ;;
*) echo "note: ${_bin_dir} is not on your PATH" ;;
esac
