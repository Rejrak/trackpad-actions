#!/usr/bin/env bash
set -euo pipefail

UUID="trackpadd-osd@rejrak.github.io"
DATA_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}"
DEST_DIR="${DATA_HOME}/gnome-shell/extensions/${UUID}"

if command -v gnome-extensions >/dev/null 2>&1; then
    gnome-extensions disable "${UUID}" >/dev/null 2>&1 || true
    gnome-extensions uninstall "${UUID}" >/dev/null 2>&1 || true
fi

rm -rf "${DEST_DIR}"

echo "Removed GNOME Shell extension: ${UUID}"
echo "If GNOME Shell had already loaded it, log out and back in to fully refresh the session."
