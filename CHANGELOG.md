# Changelog

All notable changes to `trackpadd` are documented in this file.

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
