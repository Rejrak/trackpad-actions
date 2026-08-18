# trackpadd

Configurable Linux trackpad edge gestures for desktop actions.

**Latest stable release:** [`v0.3.0`](https://github.com/Rejrak/trackpad-actions/releases/tag/v0.3.0) · [Changelog](CHANGELOG.md)

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
* Read-only daemon status via `trackpadd status` and action-value streaming via `trackpadd watch`.
* Optional GNOME Shell 45–50 adapter using GNOME's native OSD for brightness, volume, and media feedback.
* Media feedback can include MPRIS duration/player/title/artist context and the current PipeWire output name.
* No root daemon.

## What's new in v0.3.0

`v0.3.0` adds the desktop-integration layer while keeping the Rust daemon
desktop-neutral:

* user-session D-Bus status through `trackpadd status`;
* `ActionValueChanged` events and `trackpadd watch`;
* an optional GNOME Shell 45–50 adapter using the native Shell OSD;
* real media progress and MPRIS player/title/artist context when available;
* current PipeWire output context for volume feedback;
* hardened source installation using the checked-in `Cargo.lock`.

See [CHANGELOG.md](CHANGELOG.md) for the complete release notes.

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
       +--> playerctl
       |
       +-- D-Bus ActionValueChanged
               |
               v
      GNOME Shell extension
               |
               v
       native GNOME OSD
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

desktop/
└── gnome/
    └── trackpadd-osd@rejrak.github.io/
        ├── metadata.json
        └── extension.js

scripts/
├── install-local.sh
├── uninstall-local.sh
├── install-gnome-extension.sh
└── uninstall-gnome-extension.sh
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

### Stable release (recommended)

The latest stable release is `v0.3.0`. To install exactly that version:

```bash
git clone https://github.com/Rejrak/trackpad-actions.git
cd trackpad-actions
git checkout v0.3.0
./scripts/install-local.sh
```

Verify the installed daemon:

```bash
~/.local/bin/trackpadd --version
```

Expected output:

```text
trackpadd 0.3.0
```

The installer builds against the checked-in `Cargo.lock`, installs the binary
to `~/.local/bin/trackpadd`, preserves an existing user configuration, installs
the systemd user service and restricted `udev/uaccess` rule, reloads udev, and
restarts the service so the installed binary is active immediately.

On GNOME Shell 45–50, the native OSD adapter is optional and installed
separately:

```bash
./scripts/install-gnome-extension.sh
```

The GNOME-specific adapter remains separate from the desktop-neutral daemon.

To uninstall the locally installed daemon:

```bash
./scripts/uninstall-local.sh
```

To remove only the GNOME integration:

```bash
./scripts/uninstall-gnome-extension.sh
```

The user configuration is intentionally preserved by the daemon uninstall
script.

### Development branch

The `main` branch tracks ongoing development and may move ahead of the latest
stable release. Contributors who want the current development state can use
`main`; users who want reproducible installation should use the release tag
shown above.

## Build from source

These commands are for building the current checkout. For a reproducible stable install, use the tagged release instructions in [Installation](#installation).

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
cargo test --workspace --locked
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
  --locked \
  -- \
  -D warnings
```

Build the optimized daemon:

```bash
cargo build --release --locked -p trackpadd
```

The executable will be:

```text
target/release/trackpadd
```

For installation, upgrade, and uninstall commands, see [Installation](#installation).

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

At gesture start it also performs a best-effort `wpctl inspect` of that sink.
For desktop feedback it prefers PipeWire's short `node.nick`, then the
human-readable `node.description`, and finally `node.name`. Failure to inspect
the sink never blocks volume changes.

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
relative to that starting position. It also reads optional MPRIS/playerctl
metadata once at gesture start: total duration (`mpris:length`), player name,
title, and artist. Duration is used to clamp seeking to the end of the media,
while the text metadata is exposed to desktop adapters. Players or streams that
do not support MPRIS seeking may reject the operation; missing metadata is
non-fatal and simply produces a less detailed UI.

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

The v0.3 D-Bus contract uses:

```text
service:   io.github.Rejrak.Trackpadd
path:      /io/github/Rejrak/Trackpadd
interface: io.github.Rejrak.Trackpadd1
```

The read-only status methods are:

```text
Ping()       -> string
Version()    -> string
DevicePath() -> string
ConfigPath() -> string
DryRun()     -> boolean
```

For the 0.3.x release line, `io.github.Rejrak.Trackpadd1` and the signal body
below are treated as stable. A future incompatible protocol change should use
a new interface generation instead of silently changing `Trackpadd1`.

D-Bus setup is best-effort: failure to connect to the session bus does not stop
gesture processing. In that case `trackpadd status` is unavailable.

Action backends that expose continuous values also emit:

```text
ActionValueChanged(action_id, kind, value, max_value, unit, (source, title, artist))
D-Bus body signature: ssdds(sss)
```

Current value kinds are:

```text
brightness      percent
volume          percent
media-position  seconds
```

`max_value` is the configured percentage ceiling for brightness/volume. For
`media-position`, it is the total media duration in seconds when the active
player exposes `mpris:length`; `0` means that no maximum is currently known.

`source`, `title`, and `artist` are optional textual context fields. The
`media-seek` backend fills them from playerctl's `playerName`, `xesam:title`,
and `xesam:artist` metadata when available. The volume backend uses `source`
for the current PipeWire output name when WirePlumber exposes one; `title` and
`artist` remain empty. Desktop adapters can therefore stay generic without
inventing metadata.

Watch these events from another terminal with:

```bash
trackpadd watch
```

This event stream is intended as the integration point for desktop OSDs and
other lightweight consumers. IPC failures remain best-effort and never turn a
successful gesture action into an action failure.

### Native GNOME Shell OSD

For GNOME Shell 45–50, trackpadd includes an optional Shell extension that
subscribes directly to `ActionValueChanged` and renders feedback through GNOME
Shell's own OSD window manager. This is the same Shell UI used for native
brightness and volume feedback; it is not a desktop notification and it does
not create a separate overlay window.

Install the extension from the source checkout:

```bash
./scripts/install-gnome-extension.sh
```

On Wayland, a newly installed extension may require logging out and back in
before GNOME Shell discovers it. Then enable it with:

```bash
gnome-extensions enable trackpadd-osd@rejrak.github.io
```

Inspect its state:

```bash
gnome-extensions info trackpadd-osd@rejrak.github.io
```

Brightness uses the native icon, label and level bar. Volume additionally
uses the current PipeWire output name when available, so the GNOME OSD can show
labels such as `Speakers` or a headset/device name instead of the generic
`Volume`. Media seeking shows available player/title/artist context together
with the current and total clock position, plus a native progress bar when the
player exposes `mpris:length`. Missing metadata degrades independently.

The adapter is intentionally GNOME-specific while the daemon's D-Bus event
contract remains desktop-neutral. Other desktop environments can implement
their own adapters without changing gesture recognition or action backends.

Uninstall only the GNOME integration with:

```bash
./scripts/uninstall-gnome-extension.sh
```

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

Dry-run recognizes gestures and calculates action deltas without invoking
configured action backends. Because action backends are skipped,
`ActionValueChanged` events are not emitted in dry-run mode.

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
