#!/usr/bin/env bash
set -euo pipefail

BIN_PATH="${HOME}/.local/bin/trackpadd"
USER_UNIT_DIR="${XDG_CONFIG_HOME:-${HOME}/.config}/systemd/user"
UDEV_RULE_DEST="/etc/udev/rules.d/69-trackpadd.rules"

systemctl --user disable --now trackpadd.service 2>/dev/null || true
rm -f "${USER_UNIT_DIR}/trackpadd.service"
systemctl --user daemon-reload 2>/dev/null || true
rm -f "$BIN_PATH"

if [[ -e "$UDEV_RULE_DEST" ]]; then
    sudo rm -f "$UDEV_RULE_DEST"
    sudo udevadm control --reload-rules
    sudo udevadm trigger --subsystem-match=input --action=change
fi

echo "Removed binary, user service, and udev rule."
echo "User config was kept at ${XDG_CONFIG_HOME:-${HOME}/.config}/trackpadd/config.toml"
