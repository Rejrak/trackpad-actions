#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
UUID="trackpadd-osd@rejrak.github.io"
SOURCE_DIR="${ROOT_DIR}/desktop/gnome/${UUID}"

command -v gnome-shell >/dev/null 2>&1 || {
    echo "error: GNOME Shell is not installed or not in PATH" >&2
    exit 1
}

command -v gnome-extensions >/dev/null 2>&1 || {
    echo "error: gnome-extensions is required" >&2
    exit 1
}

[[ -f "${SOURCE_DIR}/metadata.json" && -f "${SOURCE_DIR}/extension.js" ]] || {
    echo "error: GNOME extension source is incomplete: ${SOURCE_DIR}" >&2
    exit 1
}

GNOME_VERSION="$(gnome-shell --version)"
GNOME_MAJOR="$(printf '%s\n' "${GNOME_VERSION}" | grep -oE '[0-9]+' | head -n 1)"

if [[ -z "${GNOME_MAJOR}" ]]; then
    echo "error: could not determine GNOME Shell version from: ${GNOME_VERSION}" >&2
    exit 1
fi

if (( GNOME_MAJOR < 45 || GNOME_MAJOR > 50 )); then
    echo "error: this extension currently declares support for GNOME Shell 45–50; found ${GNOME_VERSION}" >&2
    exit 1
fi

# Use the data directory from the running GNOME Shell process when possible.
# This avoids installing into the terminal's XDG_DATA_HOME when it differs from
# the graphical session's environment.
SHELL_DATA_HOME=""
SHELL_PID="$(pgrep -n gnome-shell 2>/dev/null || true)"

if [[ -n "${SHELL_PID}" && -r "/proc/${SHELL_PID}/environ" ]]; then
    SHELL_DATA_HOME="$(
        tr '\0' '\n' < "/proc/${SHELL_PID}/environ" \
            | sed -n 's/^XDG_DATA_HOME=//p' \
            | head -n 1
    )"
fi

if [[ -z "${SHELL_DATA_HOME}" ]]; then
    SHELL_DATA_HOME="${HOME}/.local/share"
fi

export XDG_DATA_HOME="${SHELL_DATA_HOME}"

DEST_DIR="${XDG_DATA_HOME}/gnome-shell/extensions/${UUID}"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "==> GNOME Shell: ${GNOME_VERSION}"
echo "==> GNOME session data dir: ${XDG_DATA_HOME}"
echo "==> Packing extension with gnome-extensions"

gnome-extensions pack \
    --force \
    --out-dir "${TMP_DIR}" \
    "${SOURCE_DIR}"

PACK="${TMP_DIR}/${UUID}.shell-extension.zip"

[[ -f "${PACK}" ]] || {
    echo "error: expected extension bundle was not created: ${PACK}" >&2
    exit 1
}

echo "==> Installing bundle with gnome-extensions"
INSTALLED_UUID="$(gnome-extensions install --force --print-uuid "${PACK}")"

if [[ "${INSTALLED_UUID}" != "${UUID}" ]]; then
    echo "error: installer returned unexpected UUID: ${INSTALLED_UUID}" >&2
    exit 1
fi

[[ -f "${DEST_DIR}/metadata.json" && -f "${DEST_DIR}/extension.js" ]] || {
    echo "error: extension bundle was not installed at the expected location: ${DEST_DIR}" >&2
    exit 1
}

echo "==> Installed: ${DEST_DIR}"
echo

if gnome-extensions list --user | grep -Fxq "${UUID}"; then
    echo "GNOME Shell already knows this extension."
    gnome-extensions disable "${UUID}" >/dev/null 2>&1 || true

    if gnome-extensions enable "${UUID}"; then
        echo
        echo "Enabled ${UUID}."
        echo "Verify:"
        echo "  gnome-extensions info ${UUID}"
        exit 0
    fi

    echo "warning: GNOME Shell knows the extension, but enabling it failed." >&2
    echo "Run:"
    echo "  gnome-extensions info ${UUID}"
    exit 1
fi

cat <<EOF
The bundle was installed successfully, but the currently running GNOME Shell
has not loaded this newly installed extension yet.

The official gnome-extensions installer loads newly installed bundles in the
next GNOME Shell session.

On Wayland:
  1. log out completely
  2. log back in
  3. verify:
       gnome-extensions list --user | grep -F ${UUID}
  4. enable:
       gnome-extensions enable ${UUID}
  5. inspect:
       gnome-extensions info ${UUID}

Installed files:
  ${DEST_DIR}

If the extension is still absent after a real logout/login, run:
  PID="\$(pgrep -n gnome-shell)"
  tr '\0' '\n' < "/proc/\$PID/environ" | grep '^XDG_DATA_HOME=' || true
  journalctl -b /usr/bin/gnome-shell --no-pager | grep -Ei 'trackpadd|extension' | tail -n 100
EOF
