#!/usr/bin/env bash
# Install HwpCraft from this release folder for the current user (no root needed):
# binaries to ~/.local/bin, the desktop entry and icon to ~/.local/share.
# Usage: ./install.sh            (run from the unpacked HwpCraft-<version>-linux-x64 folder)
#        ./install.sh --uninstall
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"

if [[ "${1:-}" == "--uninstall" ]]; then
  rm -f "$BIN/hwpcraft" "$BIN/hwpcraft-cli" \
        "$DATA/applications/hwpcraft.desktop" \
        "$DATA/icons/hicolor/256x256/apps/hwpcraft.png"
  echo "HwpCraft removed."
  exit 0
fi

install -Dm755 "$HERE/hwpcraft" "$BIN/hwpcraft"
install -Dm755 "$HERE/hwpcraft-cli" "$BIN/hwpcraft-cli"
install -Dm644 "$HERE/hwpcraft.desktop" "$DATA/applications/hwpcraft.desktop"
if [[ -f "$HERE/hwpcraft.png" ]]; then
  install -Dm644 "$HERE/hwpcraft.png" "$DATA/icons/hicolor/256x256/apps/hwpcraft.png"
fi
command -v update-desktop-database >/dev/null && update-desktop-database "$DATA/applications" || true
echo "Installed HwpCraft to $BIN (make sure it is on PATH)."
echo "Korean text needs a Korean font, e.g. Noto Sans CJK KR (fonts-noto-cjk) or NanumGothic (fonts-nanum)."
