# Changelog

All notable changes to `trackpadd` are documented in this file.

## [Unreleased]

## [0.3.0] - 2026-08-18

### Added

- Read-only daemon status over the user D-Bus session bus.
- `trackpadd status` for querying the running daemon without opening evdev.
- D-Bus `ActionValueChanged` events for brightness, volume, and media position.
- `trackpadd watch` for observing daemon action-value events.
- Optional GNOME Shell 45–50 adapter using GNOME's native OSD window manager.
- Helper scripts for installing and removing the GNOME Shell integration.
- Media duration propagation from MPRIS `mpris:length` through D-Bus to the
  native GNOME OSD, including a real seek progress bar when duration is known.
- Media player, title, and artist context from playerctl/MPRIS in
  `ActionValueChanged`, with native GNOME OSD labels and graceful fallback when
  metadata fields are absent.
- Current PipeWire output context for volume events, allowing the native GNOME
  OSD to display the active sink name while preserving a generic fallback.

### Changed

- The source installer now builds against the checked-in `Cargo.lock` and
  restarts an already-enabled user service after installing a new daemon binary.
- The v0.3 user-session D-Bus interface and `ActionValueChanged` payload are
  documented as the stable `io.github.Rejrak.Trackpadd1` protocol generation.

## [0.2.0] - 2026-08-18

### Added

- Persistent touchpad selection by name, vendor, and product.
- `trackpadd check-config` for hardware-independent configuration validation.
- Generic one-shot command actions with trigger, direction, and threshold controls.
- Detailed Linux input-device diagnostics through `trackpadd devices` and `--all`.
- Horizontal one-finger swipes starting from the physical top edge.
- Continuous media-position scrubbing through `playerctl` / MPRIS with configurable
  seek range, update interval, deadzone, and response curve.
- Left/right direction handling for command actions.
- `trackpadd --version`.

### Changed

- Touchpad compatibility diagnostics now explain rejected and unreadable devices.
- Runtime device selection prefers a persistent configured selector when present.
- Top-edge gestures track horizontal displacement while left/right edges retain
  vertical displacement semantics.
- Documentation and example configuration now cover the v0.2 features and the
  source-based installation path actually present in the repository.

## [0.1.0]

- Initial public release with direct Linux evdev multitouch input.
- Left/right one-finger edge swipes.
- Brightness control through `brightnessctl`.
- Volume control through `wpctl`.
- Debug/print actions, systemd user service, and udev/uaccess integration.
