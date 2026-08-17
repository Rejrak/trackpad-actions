# trackpad-actions v0.2

Linux-first configurable trackpad gestures. The project keeps raw Linux input,
gesture recognition, actions, and desktop UI integration separate.

Current example:

- right edge vertical swipe -> screen brightness
- left edge vertical swipe -> output volume

## Architecture

```text
/dev/input/eventX
       |
       v
 trackpad-linux      evdev + MT slots + normalized coordinates
       |
       v
 trackpad-core       pure gesture recognizers
       |
       v
    trackpadd        config + bindings + actions
       |
       +--> brightnessctl
       +--> wpctl
       +--> future D-Bus / GNOME / KDE adapters
```

Neither `trackpad-core` nor `trackpad-linux` depends on GNOME, KDE, PipeWire,
systemd, or a particular distro.

## Fedora prerequisites

```bash
sudo dnf install gcc curl acl brightnessctl wireplumber
```

Install Rust if needed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup component add rustfmt clippy
```

## Development build

```bash
cargo test
cargo build -p trackpadd
```

## v0.2: auto device selection

`--device` is now optional. If exactly one compatible touchpad is readable,
trackpadd selects it automatically:

```bash
./target/debug/trackpadd monitor
```

or:

```bash
./target/debug/trackpadd run --config config.example.toml --dry-run
```

If multiple compatible touchpads exist, trackpadd refuses to guess and asks for
`--device /dev/input/eventX`.

## v0.2: persistent user config

Create the default config:

```bash
./target/debug/trackpadd init-config
```

It writes to:

```text
$XDG_CONFIG_HOME/trackpadd/config.toml
```

or, when `XDG_CONFIG_HOME` is not set:

```text
~/.config/trackpadd/config.toml
```

After that, this is enough:

```bash
./target/debug/trackpadd run
```

## v0.2: persistent touchpad access on systemd/logind distros

For development we previously used a temporary ACL such as:

```bash
sudo setfacl -m "u:$(id -un):r--" /dev/input/event4
```

That is intentionally no longer the installation strategy.

`packaging/udev/69-trackpadd.rules` matches only udev event nodes classified as
`ID_INPUT_TOUCHPAD=1` and adds `TAG+="uaccess"`. On a systemd/logind desktop this
allows the active local user to receive a dynamic ACL for the touchpad without
putting that user in the broad `input` group.

Before installing the rule you can verify your device classification:

```bash
udevadm info --query=property /dev/input/event4 | grep ID_INPUT_TOUCHPAD
```

Expected:

```text
ID_INPUT_TOUCHPAD=1
```

Install/reload manually:

```bash
sudo install -Dm644 \
  packaging/udev/69-trackpadd.rules \
  /etc/udev/rules.d/69-trackpadd.rules

sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=input --action=change
```

Then verify without sudo:

```bash
./target/debug/trackpadd devices
```

### Test that uaccess really replaced your manual ACL

If you previously added a manual ACL to `/dev/input/event4`, you can remove it:

```bash
sudo setfacl -x "u:$(id -un)" /dev/input/event4
```

Then retrigger udev:

```bash
sudo udevadm trigger --subsystem-match=input --action=change
```

Check:

```bash
getfacl /dev/input/event4
```

Your user should regain access through the dynamic `uaccess` policy.

## v0.2: systemd user service

The daemon must run as your desktop user, not root, because actions such as
`wpctl` need the user's PipeWire session.

A user unit is included at:

```text
packaging/systemd/trackpadd.service
```

For a manual source install:

```bash
mkdir -p ~/.local/bin ~/.config/systemd/user
cargo build --release -p trackpadd
install -Dm755 target/release/trackpadd ~/.local/bin/trackpadd
./target/release/trackpadd init-config
install -Dm644 packaging/systemd/trackpadd.service \
  ~/.config/systemd/user/trackpadd.service

systemctl --user daemon-reload
systemctl --user enable --now trackpadd.service
```

Status/logs:

```bash
systemctl --user status trackpadd.service
journalctl --user -u trackpadd.service -f
```

The service runs simply:

```text
trackpadd run
```

so it uses automatic touchpad selection and the XDG user config.

## One-command local install on Fedora/systemd

For this development version:

```bash
./scripts/install-local.sh
```

It:

1. builds the release binary;
2. installs it to `~/.local/bin/trackpadd`;
3. creates the XDG config if missing;
4. installs the systemd user unit;
5. installs the udev `uaccess` rule using sudo;
6. reloads udev;
7. enables and starts the user service.

Uninstall the development install with:

```bash
./scripts/uninstall-local.sh
```

The uninstall script intentionally keeps your user config.

## Configuration

Example:

```toml
[[gestures]]
id = "right-edge"
type = "edge-swipe"
edge = "right"
width = 0.06
cancel_margin = 0.04

[[gestures]]
id = "left-edge"
type = "edge-swipe"
edge = "left"
width = 0.06
cancel_margin = 0.04

[[actions]]
id = "screen-brightness"
type = "brightness"
command = "brightnessctl"
min = 0.05
max = 1.0

[[actions]]
id = "speaker-volume"
type = "volume"
command = "wpctl"
min = 0.0
max = 1.0

[[bindings]]
gesture = "right-edge"
action = "screen-brightness"
sensitivity = 1.20

[[bindings]]
gesture = "left-edge"
action = "speaker-volume"
sensitivity = 1.00
```

Gestures and actions remain independent. Swapping the two bindings does not
require recompilation.

## Action error behaviour

If an action backend fails during `Started` or `Updated`, v0.2 logs the failure
once and suppresses repeated errors for the remainder of that gesture. The next
gesture retries the action normally.

This avoids floods such as dozens of repeated PipeWire connection failures.

## Portability strategy

The **runtime core remains Linux/distro independent** at the evdev boundary.
The files under `packaging/` are adapters:

- `packaging/udev`: device access on udev/logind systems
- `packaging/systemd`: optional systemd user-service integration
- future packages can add OpenRC/runit/autostart adapters without changing
  `trackpad-core` or `trackpad-linux`

The next architectural milestone is an IPC/event layer (most likely D-Bus on
Linux desktops) so GNOME/KDE integrations can display native OSD/progress UI
without owning gesture recognition or system actions.
