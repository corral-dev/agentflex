#!/bin/sh
# agentflex zero-install runner: fetches the prebuilt binary for this machine
# from the GitHub release and runs it. No Node, no Python, no package manager.
#
#   curl -fsSL https://raw.githubusercontent.com/corral-dev/agentflex/main/scripts/flex.sh | sh
#   curl -fsSL .../flex.sh | sh -s -- --month --png month.png   # args pass through
#
# Everything stays local: the binary only reads coding-agent histories on disk.
set -eu

REPO="corral-dev/agentflex"
VERSION="${AGENTFLEX_VERSION:-v0.3.2}"
DEST="${AGENTFLEX_BIN:-/tmp/agentflex}"

os="$(uname -s)"; arch="$(uname -m)"
case "$os-$arch" in
  Darwin-arm64)  asset="agentflex-aarch64-apple-darwin" ;;
  Darwin-x86_64) asset="agentflex-x86_64-apple-darwin" ;;
  Linux-aarch64|Linux-arm64) asset="agentflex-aarch64-unknown-linux-musl" ;;
  Linux-x86_64)  asset="agentflex-x86_64-unknown-linux-musl" ;;
  *) echo "agentflex: unsupported platform $os/$arch" >&2; exit 1 ;;
esac
# Windows (PowerShell): irm https://github.com/$REPO/releases/download/$VERSION/agentflex-x86_64-pc-windows-msvc.exe -OutFile $env:TEMP/agentflex.exe

url="https://github.com/$REPO/releases/download/$VERSION/$asset"
if command -v curl >/dev/null 2>&1; then
  curl -fsSL "$url" -o "$DEST"
elif command -v wget >/dev/null 2>&1; then
  wget -qO "$DEST" "$url"
else
  echo "agentflex: need curl or wget" >&2; exit 1
fi
chmod +x "$DEST"
exec "$DEST" "$@"
