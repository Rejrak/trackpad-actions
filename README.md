# trackpadd

Configurable Linux trackpad edge gestures for desktop actions.

`trackpadd` reads multitouch events directly from Linux `evdev`, recognizes configurable edge swipes, and maps them to actions such as screen brightness and speaker volume.

The project intentionally keeps Linux input handling, gesture recognition, configuration, and desktop actions separate so the core is not tied to a particular Linux distribution or desktop environment.

## Features

* One-finger vertical swipes starting from the **left** or **right** physical trackpad edge.
* One-finger horizontal swipes starting from the **top** physical trackpad edge.
* Configurable activation width and cancellation margin.
* Configurable gesture sensitivity and direction inversion.
* Screen brightness control through `brightnessctl`.
* Default audio output volume control through `wpctl`.
* Continuous media-position scrubbing through `playerctl` / MPRIS.
* Print/debug actions for testing mappings.
* Automatic touchpad selection when exactly one compatible device is available.
* Explicit device selection when multiple compatible touchpads exist.
* XDG-compatible user configuration.
* `udev` + `uaccess` integration for safe touchpad access.
* systemd user service.
* Monitor and dry-run modes for debugging without modifying system state.
* Read-only daemon status over the user D-Bus session bus.
* No root daemon.

## Default example

| Gesture                      | Action            |
| ---------------------------- | ----------------- |
| Vertical swipe on right edge | Screen brightness |
| Vertical swipe on left edge  | Speaker volume    |

Mappings live in the configuration file and can be changed without recompiling.

## Architecture

```text
/dev/input/eventX
       |
       v
 trackpad-linux
 evdev + MT slots
 normalized coordinates
       |
       v
 trackpad-core
 pure gesture recognizers
       |
       v
    trackpadd
 configuration
 bindings
 actions
       |
       +--> brightnessctl
       +--> wpctl
       +--> future D-Bus / desktop adapters
```

Repository layout:

```text
crates/
├── trackpad-core/
│   └── gesture recognition primitives
│
├── trackpad-linux/
│   └── Linux evdev / multitouch reader
│
└── trackpadd/
    └── CLI, configuration, bindings and actions

packaging/
├── systemd/
│   └── trackpadd.service
│
└── udev/
    └── 69-trackpadd.rules

scripts/
├── install-local.sh
└── uninstall-local.sh
```

## Compatibility

### Supported environment

The current source installer targets Linux desktop systems providing:

* Linux `evdev`;
* a compatible multitouch touchpad;
* `udev`;
* `systemd-logind`;
* a systemd user manager.

The `trackpadd` binary itself is intentionally less distribution-specific than its packaging. Non-systemd distributions may still run it manually, but require their own input permission and service-manager integration.

### Touchpad requirements

The device must expose multitouch events including:

```text
ABS_MT_SLOT
ABS_MT_TRACKING_ID
ABS_MT_POSITION_X
ABS_MT_POSITION_Y
```

Direct-input devices such as touchscreens and `SEMI_MT` devices are not currently supported.

## Runtime dependencies

Once built, the daemon does not require Rust at runtime.

Some configured actions require external commands.

| Feature           | Command         |
| ----------------- | --------------- |
| Screen brightness | `brightnessctl` |
| Speaker volume    | `wpctl`         |
| Media scrubbing   | `playerctl`     |

`wpctl` is normally provided by WirePlumber.

The source installer also expects common Linux utilities including:

```text
sudo
udevadm
systemctl
```

## Installation

The currently supported installation path is from a source checkout.

```bash
git clone https://github.com/Rejrak/trackpad-actions.git
cd trackpad-actions
./scripts/install-local.sh
```

The source installer builds the release binary locally, installs it to
`~/.local/bin/trackpadd`, preserves an existing user configuration, installs
the systemd user service and the restricted `udev/uaccess` rule, reloads udev,
and enables the service.

To uninstall the locally installed daemon:

```bash
./scripts/uninstall-local.sh
```

The user configuration is intentionally preserved by the uninstall script.

## Build from source

Building from source requires Rust **1.85 or newer**.

Clone the repository:

```bash
git clone https://github.com/Rejrak/trackpad-actions.git
cd trackpad-actions
```

Verify Rust:

```bash
rustc --version
cargo --version
```

Run tests:

```bash
cargo test --workspace
```

Run formatting checks:

```bash
cargo fmt --all -- --check
```

Run Clippy:

```bash
cargo clippy \
  --workspace \
  --all-targets \
  --all-features \
  -- \
  -D warnings
```

Build the optimized daemon:

```bash
cargo build --release -p trackpadd
```

The executable will be:

```text
target/release/trackpadd
```

### Install from a source checkout

The repository includes a development/source installer:

```bash
./scripts/install-local.sh
```

It builds the release binary locally and installs the binary, user configuration, systemd unit, and udev rule.

To uninstall:

```bash
./scripts/uninstall-local.sh
```

## Configuration

Configuration follows the XDG Base Directory convention.

When `$XDG_CONFIG_HOME` is set:

```text
$XDG_CONFIG_HOME/trackpadd/config.toml
```

Otherwise:

```text
~/.config/trackpadd/config.toml
```

Create the default configuration with:

```bash
trackpadd init-config
```

Overwrite an existing configuration intentionally with:

```bash
trackpadd init-config --force
```

### Example configuration

```toml
# A gesture starts only when a NEW finger contact appears
# inside the configured physical edge zone.

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


# Screen brightness

[[actions]]
id = "screen-brightness"
type = "brightness"
command = "brightnessctl"
min = 0.05
max = 1.0


# Default output volume

[[actions]]
id = "speaker-volume"
type = "volume"
command = "wpctl"
min = 0.0
max = 1.0


# Bind gestures to actions

[[bindings]]
gesture = "right-edge"
action = "screen-brightness"
sensitivity = 1.20
invert = false

[[bindings]]
gesture = "left-edge"
action = "speaker-volume"
sensitivity = 1.00
invert = false
```

## Gesture configuration

An edge swipe supports:

```toml
[[gestures]]
id = "right-edge"
type = "edge-swipe"
edge = "right"
width = 0.06
cancel_margin = 0.04
```

`id` is the unique gesture identifier.

`edge` can currently be:

```text
left
right
top
```

Left and right edges track vertical movement: positive delta means moving up.

The top edge tracks horizontal movement: positive delta means moving right and negative delta means moving left.

`width` controls the activation zone. For example:

```toml
width = 0.06
```

means that the outermost 6% of the selected physical edge is used as the gesture activation zone.

For `edge = "top"`, the same value describes the top 6% of the trackpad height, while the gesture displacement itself is measured horizontally.

`cancel_margin` adds extra inward tolerance once the gesture has started.

A gesture begins only when a **new contact starts inside the edge zone**. Sliding an already active finger into the edge does not activate a gesture.

A horizontal top-edge swipe is configured with:

```toml
[[gestures]]
id = "top-edge"
type = "edge-swipe"
edge = "top"
width = 0.06
cancel_margin = 0.10
```

It must start in the top activation band. Moving right produces a positive delta; moving left produces a negative delta. Moving too far downward after activation cancels the gesture.

Command actions can distinguish the two directions with `direction = "right"` and `direction = "left"`.

## Actions

### Brightness

```toml
[[actions]]
id = "screen-brightness"
type = "brightness"
command = "brightnessctl"
min = 0.05
max = 1.0
```

Keeping a non-zero minimum can prevent accidentally reducing the screen to an unusably low brightness.

### Volume

```toml
[[actions]]
id = "speaker-volume"
type = "volume"
command = "wpctl"
min = 0.0
max = 1.0
```

The current backend controls:

```text
@DEFAULT_AUDIO_SINK@
```

### Media seek

Continuous media scrubbing uses `playerctl`, which controls MPRIS-compatible players:

```toml
[[actions]]
id = "media-position"
type = "media-seek"
command = "playerctl"
seconds_per_full_swipe = 60
update_interval_ms = 50
deadzone = 0.025
curve = 1.4
```

`seconds_per_full_swipe` is the maximum signed offset represented by a normalized
full-width gesture. `deadzone` filters small initial movements. `curve > 1`
provides more precision near the starting point and progressively accelerates
larger movements. Updates are rate-limited by `update_interval_ms`.

The action reads the current media position when the gesture starts, then seeks
relative to that starting position. Players or streams that do not support MPRIS
seeking may reject the operation.

### Debug action

```toml
[[actions]]
id = "debug"
type = "print"
label = "edge debug"
```

Useful while creating new mappings because it does not modify system state.

## Bindings

Gestures and actions are intentionally independent.

```toml
[[bindings]]
gesture = "right-edge"
action = "screen-brightness"
sensitivity = 1.20
invert = false
```

`sensitivity` multiplies the gesture displacement before sending it to the action.

Set:

```toml
invert = true
```

to reverse the direction.

For example, swapping brightness and volume only requires changing the bindings. No recompilation is required.

## CLI usage

Show help:

```bash
trackpadd --help
```

### Diagnose devices

```bash
trackpadd devices
```

The default view focuses on touchpad-like candidates and explains why a device is accepted or rejected.

Inspect every Linux input event node, including devices the current user cannot open:

```bash
trackpadd devices --all
```

Compare compatible devices with a persistent `[device]` selector:

```bash
trackpadd devices --config ~/.config/trackpadd/config.toml
```

The output includes compatibility reasons, multitouch axis coverage, coordinate ranges, slot count, suggested stable selectors, and an automatic/configured selection summary.

### Validate configuration

```bash
trackpadd check-config
```

Validate another file without opening the touchpad or executing actions:

```bash
trackpadd check-config --config /path/to/config.toml
```

### Query daemon status over D-Bus

When `trackpadd run` is active and a user session bus is available:

```bash
trackpadd status
```

The command reports the running daemon version, selected input device, active
configuration path, and whether the daemon is running in dry-run mode.

The initial v0.3 D-Bus API uses:

```text
service:   io.github.Rejrak.Trackpadd
path:      /io/github/Rejrak/Trackpadd
interface: io.github.Rejrak.Trackpadd1
```

D-Bus setup is best-effort: failure to connect to the session bus does not stop
gesture processing. In that case `trackpadd status` is unavailable.

Action backends that expose continuous values also emit:

```text
ActionValueChanged(action_id, kind, value, unit)
```

Current value kinds are:

```text
brightness      percent
volume          percent
media-position  seconds
```

Watch these events from another terminal with:

```bash
trackpadd watch
```

This event stream is intended as the integration point for desktop OSDs and
other lightweight consumers. IPC failures remain best-effort and never turn a
successful gesture action into an action failure.

### Monitor touch and gesture events

```bash
trackpadd monitor
```

With an explicit device:

```bash
trackpadd monitor --device /dev/input/eventX
```

### Dry-run

```bash
trackpadd run --dry-run
```

Dry-run recognizes gestures and calculates action deltas without invoking external brightness or volume commands.

### Run

```bash
trackpadd run
```

Use a different configuration:

```bash
trackpadd run --config /path/to/config.toml
```

Use a specific touchpad:

```bash
trackpadd run --device /dev/input/eventX
```

If exactly one compatible touchpad exists, `trackpadd` selects it automatically.

If multiple compatible devices exist, it intentionally refuses to guess and asks for an explicit `--device`.

## systemd user service

The service runs as the current desktop user.

Manage it with:

```bash
systemctl --user status trackpadd.service

systemctl --user restart trackpadd.service

systemctl --user stop trackpadd.service

systemctl --user start trackpadd.service
```

Follow logs:

```bash
journalctl --user -u trackpadd.service -f
```

The daemon should not normally run as root because actions such as `wpctl` need access to services belonging to the current desktop session.

## Device permissions

The packaged udev rule is:

```udev
ACTION!="remove", SUBSYSTEM=="input", KERNEL=="event*", ENV{ID_INPUT_TOUCHPAD}=="1", TAG+="uaccess"
```

It only matches input event nodes already classified as touchpads.

On systemd/logind desktops, the `uaccess` tag allows the active local user to receive a dynamic ACL for the device.

This is preferable to permanently adding a desktop user to the broad Linux `input` group.

Check device classification:

```bash
udevadm info \
  --query=property \
  /dev/input/eventX \
  | grep ID_INPUT_TOUCHPAD
```

Expected result:

```text
ID_INPUT_TOUCHPAD=1
```

Inspect permissions:

```bash
getfacl /dev/input/eventX
```

## Troubleshooting

### `trackpadd: command not found`

The default binary location is:

```text
~/.local/bin/trackpadd
```

For the current shell:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Add the same configuration to your shell profile if necessary.

### No compatible touchpad is readable

Start with:

```bash
trackpadd devices
```

Then inspect the event device:

```bash
udevadm info \
  --query=property \
  /dev/input/eventX \
  | grep ID_INPUT_TOUCHPAD

getfacl /dev/input/eventX
```

Reload the udev rule if necessary:

```bash
sudo udevadm control --reload-rules

sudo udevadm trigger \
  --subsystem-match=input \
  --action=change
```

If the desktop session still has stale ACL state, log out and log back in.

### Multiple touchpads detected

List them:

```bash
trackpadd devices
```

Then select one explicitly:

```bash
trackpadd run --device /dev/input/eventX
```

### Gesture detected but brightness does not change

Test the backend independently:

```bash
brightnessctl -m

brightnessctl set 50%
```

If these fail outside `trackpadd`, fix the brightness backend first.

### Gesture detected but volume does not change

Test:

```bash
wpctl get-volume @DEFAULT_AUDIO_SINK@

wpctl set-volume @DEFAULT_AUDIO_SINK@ 50%
```

Also verify that PipeWire/WirePlumber is running in the same user session.

### Service repeatedly fails

Inspect:

```bash
systemctl --user status trackpadd.service

journalctl \
  --user \
  -u trackpadd.service \
  -b \
  --no-pager
```

For interactive debugging:

```bash
systemctl --user stop trackpadd.service

trackpadd run --dry-run
```

## Security model

`trackpadd` intentionally avoids running the daemon as root.

Administrative privileges are only needed during installation to place the udev rule under:

```text
/etc/udev/rules.d/
```

and reload udev.

Runtime device access is delegated to the active desktop user through `uaccess`.

Because Linux input devices contain sensitive interaction data, broad permanent access to the entire `input` group should be avoided when possible.

Release archives should include a `SHA256SUMS` file. The provided release installer verifies the selected archive before installing it.

## Development

Run the complete development checks with:

```bash
cargo fmt --all -- --check \
  && cargo clippy \
       --workspace \
       --all-targets \
       --all-features \
       -- \
       -D warnings \
  && cargo test --workspace
```

Build:

```bash
cargo build --release -p trackpadd
```

## Releases

The project uses semantic version tags:

```text
v0.2.0
v0.2.1
v0.3.0
```

Before creating a release:

```bash
cargo fmt --all -- --check

cargo clippy \
  --workspace \
  --all-targets \
  --all-features \
  -- \
  -D warnings

cargo test --workspace --locked
```

Commit the release:

```bash
git add .

git commit -m "chore: prepare v0.2.0"

git push origin main
```

Create an annotated tag:

```bash
git tag -a v0.2.0 -m "trackpadd v0.2.0"

git push origin v0.2.0
```

Pushing a `v*.*.*` tag triggers the release workflow under:

```text
.github/workflows/release.yml
```

The workflow builds and publishes:

```text
trackpadd-x86_64-unknown-linux-musl.tar.gz
trackpadd-aarch64-unknown-linux-musl.tar.gz
SHA256SUMS
```

The installer then downloads the appropriate asset from the latest GitHub Release.

## Roadmap

Possible future work:

* additional gesture recognizers;
* persistent per-device selection;
* GNOME/KDE OSD integration;
* D-Bus integration;
* more configurable action backends;
* OpenRC/runit service adapters;
* native RPM/DEB/Arch packages;
* GUI configuration;
* additional architectures.

## Contributing

Issues and pull requests are welcome.

Before opening a pull request, please run:

```bash
cargo fmt --all -- --check

cargo clippy \
  --workspace \
  --all-targets \
  --all-features \
  -- \
  -D warnings

cargo test --workspace
```

Whenever possible, keep gesture recognition independent from desktop-specific integrations.

## License

Licensed under the MIT License.

See [`LICENSE`](LICENSE).
