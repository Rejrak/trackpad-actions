#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
USER_UNIT_DIR="${CONFIG_HOME}/systemd/user"
USER_CONFIG="${CONFIG_HOME}/trackpadd/config.toml"
UDEV_RULE_DEST="/etc/udev/rules.d/69-trackpadd.rules"

command -v cargo >/dev/null 2>&1 || {
    echo "error: cargo is not installed" >&2
    exit 1
}
command -v sudo >/dev/null 2>&1 || {
    echo "error: sudo is required to install the udev rule" >&2
    exit 1
}
command -v udevadm >/dev/null 2>&1 || {
    echo "error: udevadm is required by this installer" >&2
    exit 1
}
command -v systemctl >/dev/null 2>&1 || {
    echo "error: systemctl is required by this installer" >&2
    exit 1
}

cd "$ROOT_DIR"

echo "==> Building trackpadd"
cargo build --release -p trackpadd

echo "==> Installing binary to ${BIN_DIR}/trackpadd"
install -Dm755 target/release/trackpadd "${BIN_DIR}/trackpadd"

echo "==> Creating user config if needed"
if [[ ! -e "$USER_CONFIG" ]]; then
    "${BIN_DIR}/trackpadd" init-config
else
    echo "    keeping existing $USER_CONFIG"
fi

echo "==> Installing systemd user service"
install -Dm644 packaging/systemd/trackpadd.service "${USER_UNIT_DIR}/trackpadd.service"

echo "==> Installing udev uaccess rule (sudo required)"
sudo install -Dm644 packaging/udev/69-trackpadd.rules "$UDEV_RULE_DEST"
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=input --action=change

echo "==> Enabling user service"
systemctl --user daemon-reload
systemctl --user enable --now trackpadd.service

echo
echo "Installed. Useful commands:"
echo "  trackpadd devices"
echo "  systemctl --user status trackpadd.service"
echo "  journalctl --user -u trackpadd.service -f"
echo "  $USER_CONFIG"
