#!/usr/bin/env bash
#
# mvt installer.
#
# Usage:
#     curl -fsSL https://raw.githubusercontent.com/alfredvc/matvaretabellen-cli/main/install.sh | bash
#
# Environment variables:
#     INSTALL_DIR  Override install directory (default: ~/.local/bin)
#     VERSION      Pin a specific version (default: latest release)
#
set -euo pipefail

REPO="alfredvc/matvaretabellen-cli"
BINARY="mvt"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

log() { printf '%s\n' "$*" >&2; }
die() { log "error: $*"; exit 1; }

# --- Detect platform ---------------------------------------------------

uname_s=$(uname -s | tr '[:upper:]' '[:lower:]')
uname_m=$(uname -m)

case "$uname_s" in
  linux)  os_part="unknown-linux-gnu" ;;
  darwin) os_part="apple-darwin" ;;
  *) die "unsupported OS: $uname_s" ;;
esac

case "$uname_m" in
  x86_64|amd64) arch_part="x86_64" ;;
  arm64|aarch64) arch_part="aarch64" ;;
  *) die "unsupported architecture: $uname_m" ;;
esac

TRIPLE="${arch_part}-${os_part}"
ASSET="${BINARY}-${TRIPLE}.tar.gz"

# --- Resolve version ---------------------------------------------------

if [ -n "${VERSION:-}" ]; then
  TAG="${VERSION#v}"
  TAG="v${TAG}"
else
  # curl -f so that an HTTP error fails instead of piping HTML downstream.
  LATEST_JSON=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest") || \
    die "failed to fetch latest release metadata. Does ${REPO} exist and have releases?"
  TAG=$(printf '%s' "$LATEST_JSON" | grep '"tag_name"' | head -n1 | cut -d'"' -f4)
  [ -n "$TAG" ] || die "could not parse tag_name from GitHub response"
fi

log "Installing ${BINARY} ${TAG} for ${TRIPLE}..."

# --- Download + extract ------------------------------------------------

URL="https://github.com/${REPO}/releases/download/${TAG}/${ASSET}"

TMPDIR=$(mktemp -d)
cleanup() { rm -rf "$TMPDIR"; }
trap cleanup EXIT

# -f is critical: without it a 404 returns HTML that gets piped to tar.
if ! curl -fL "$URL" -o "$TMPDIR/$ASSET"; then
  die "failed to download $URL"
fi

if ! tar xzf "$TMPDIR/$ASSET" -C "$TMPDIR"; then
  die "failed to extract $ASSET"
fi

[ -f "$TMPDIR/$BINARY" ] || die "extracted tarball does not contain '$BINARY'"

# --- Install -----------------------------------------------------------

mkdir -p "$INSTALL_DIR"
mv "$TMPDIR/$BINARY" "$INSTALL_DIR/$BINARY"
chmod +x "$INSTALL_DIR/$BINARY"

log "Installed ${BINARY} to ${INSTALL_DIR}/${BINARY}"

# --- PATH check --------------------------------------------------------

case ":$PATH:" in
  *":${INSTALL_DIR}:"*) : ;;
  *)
    log ""
    log "WARNING: ${INSTALL_DIR} is not in your PATH."
    log "Add this to your shell profile:"
    log "    export PATH=\"${INSTALL_DIR}:\$PATH\""
    ;;
esac
