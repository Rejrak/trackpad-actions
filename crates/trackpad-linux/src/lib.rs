use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use anyhow::{anyhow, bail, Context, Result};
use evdev::{AbsInfo, AbsoluteAxisCode, Device, EventType, InputEvent, PropType};
use trackpad_core::{Contact, TouchFrame};

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub path: PathBuf,
    pub name: String,
    pub vendor: u16,
    pub product: u16,
    pub pointer: bool,
    pub direct: bool,
    pub semi_mt: bool,
    pub slots: usize,
    pub x_min: i32,
    pub x_max: i32,
    pub y_min: i32,
    pub y_max: i32,
    pub compatible: bool,
}

/// Enumerates input devices that the current process can open and describe.
pub fn list_devices() -> Vec<DeviceInfo> {
    evdev::enumerate()
        .filter_map(|(path, device)| describe_device(path, &device).ok())
        .collect()
}

/// Returns compatible touchpads visible to the current user.
pub fn compatible_devices() -> Vec<DeviceInfo> {
    list_devices()
        .into_iter()
        .filter(|device| device.compatible)
        .collect()
}

/// Auto-select a touchpad when exactly one compatible device is visible.
///
/// We deliberately refuse to guess when multiple candidates are present. A later
/// configuration layer can persist vendor/product/name matching if needed.
pub fn auto_select_touchpad() -> Result<DeviceInfo> {
    let devices = compatible_devices();

    match devices.as_slice() {
        [] => bail!(
            "no compatible touchpad is readable by the current user; run `trackpadd devices` \
             and verify the udev/uaccess installation"
        ),
        [device] => Ok(device.clone()),
        many => {
            let candidates = many
                .iter()
                .map(|device| format!("{} ({})", device.path.display(), device.name))
                .collect::<Vec<_>>()
                .join(", ");
            bail!(
                "multiple compatible touchpads are visible ({candidates}); pass --device explicitly"
            )
        }
    }
}

fn describe_device(path: PathBuf, device: &Device) -> Result<DeviceInfo> {
    let axes = device.supported_absolute_axes();

    let has_mt_axes = axes.is_some_and(|axes| {
        axes.contains(AbsoluteAxisCode::ABS_MT_SLOT)
            && axes.contains(AbsoluteAxisCode::ABS_MT_TRACKING_ID)
            && axes.contains(AbsoluteAxisCode::ABS_MT_POSITION_X)
            && axes.contains(AbsoluteAxisCode::ABS_MT_POSITION_Y)
    });

    let properties = device.properties();
    let pointer = properties.contains(PropType::POINTER);
    let direct = properties.contains(PropType::DIRECT);
    let semi_mt = properties.contains(PropType::SEMI_MT);

    let x = axis_info(device, AbsoluteAxisCode::ABS_MT_POSITION_X)?;
    let y = axis_info(device, AbsoluteAxisCode::ABS_MT_POSITION_Y)?;
    let slot = axis_info(device, AbsoluteAxisCode::ABS_MT_SLOT)?;

    let slots = if slot.maximum() >= slot.minimum() {
        (slot.maximum() - slot.minimum() + 1) as usize
    } else {
        0
    };

    let input_id = device.input_id();

    Ok(DeviceInfo {
        path,
        name: device.name().unwrap_or("<unnamed>").to_owned(),
        vendor: input_id.vendor(),
        product: input_id.product(),
        pointer,
        direct,
        semi_mt,
        slots,
        x_min: x.minimum(),
        x_max: x.maximum(),
        y_min: y.minimum(),
        y_max: y.maximum(),
        compatible: has_mt_axes && !direct && !semi_mt && slots > 0,
    })
}

fn axis_info(device: &Device, code: AbsoluteAxisCode) -> Result<AbsInfo> {
    device
        .get_absinfo()
        .context("failed to read absolute axis metadata")?
        .find_map(|(axis, info)| (axis == code).then_some(info))
        .ok_or_else(|| anyhow!("device does not expose {code:?}"))
}

#[derive(Debug, Clone, Default)]
struct SlotState {
    tracking_id: Option<i32>,
    x: Option<i32>,
    y: Option<i32>,
}

pub struct TouchpadReader {
    device: Device,
    x_min: i32,
    x_max: i32,
    y_min: i32,
    y_max: i32,
    slot_min: i32,
    current_slot: usize,
    slots: Vec<SlotState>,
    pending_frames: VecDeque<TouchFrame>,
}

impl TouchpadReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let device =
            Device::open(path).with_context(|| format!("failed to open {}", path.display()))?;

        let axes = device
            .supported_absolute_axes()
            .ok_or_else(|| anyhow!("device does not expose absolute axes"))?;

        for axis in [
            AbsoluteAxisCode::ABS_MT_SLOT,
            AbsoluteAxisCode::ABS_MT_TRACKING_ID,
            AbsoluteAxisCode::ABS_MT_POSITION_X,
            AbsoluteAxisCode::ABS_MT_POSITION_Y,
        ] {
            if !axes.contains(axis) {
                bail!("{} is missing required axis {axis:?}", path.display());
            }
        }

        if device.properties().contains(PropType::DIRECT) {
            bail!(
                "{} looks like a direct device (for example a touchscreen)",
                path.display()
            );
        }

        if device.properties().contains(PropType::SEMI_MT) {
            bail!(
                "{} is a semi-multitouch device and is not supported by this MVP",
                path.display()
            );
        }

        let x = axis_info(&device, AbsoluteAxisCode::ABS_MT_POSITION_X)?;
        let y = axis_info(&device, AbsoluteAxisCode::ABS_MT_POSITION_Y)?;
        let slot = axis_info(&device, AbsoluteAxisCode::ABS_MT_SLOT)?;

        if x.maximum() <= x.minimum() || y.maximum() <= y.minimum() {
            bail!("invalid trackpad coordinate ranges");
        }

        let slot_count = (slot.maximum() - slot.minimum() + 1).max(0) as usize;
        if slot_count == 0 {
            bail!("device reports zero multitouch slots");
        }

        Ok(Self {
            device,
            x_min: x.minimum(),
            x_max: x.maximum(),
            y_min: y.minimum(),
            y_max: y.maximum(),
            slot_min: slot.minimum(),
            current_slot: 0,
            slots: vec![SlotState::default(); slot_count],
            pending_frames: VecDeque::new(),
        })
    }

    pub fn name(&self) -> &str {
        self.device.name().unwrap_or("<unnamed>")
    }

    /// Blocks until the next complete SYN_REPORT frame is available.
    pub fn next_frame(&mut self) -> Result<TouchFrame> {
        loop {
            if let Some(frame) = self.pending_frames.pop_front() {
                return Ok(frame);
            }

            // Collect first so the iterator's mutable borrow of `device` ends before
            // we mutate the rest of `self`.
            let events: Vec<InputEvent> = self
                .device
                .fetch_events()
                .context("failed while reading evdev events")?
                .collect();

            for event in events {
                self.process_event(event);
            }
        }
    }

    fn process_event(&mut self, event: InputEvent) {
        if event.event_type() == EventType::ABSOLUTE {
            let code = event.code();
            let value = event.value();

            if code == AbsoluteAxisCode::ABS_MT_SLOT.0 {
                let raw_index = value - self.slot_min;
                if raw_index >= 0 && (raw_index as usize) < self.slots.len() {
                    self.current_slot = raw_index as usize;
                }
            } else if code == AbsoluteAxisCode::ABS_MT_TRACKING_ID.0 {
                let slot = &mut self.slots[self.current_slot];
                if value < 0 {
                    slot.tracking_id = None;
                    slot.x = None;
                    slot.y = None;
                } else if slot.tracking_id != Some(value) {
                    slot.tracking_id = Some(value);
                    // Prevent stale coordinates from a previous contact that reused this slot.
                    slot.x = None;
                    slot.y = None;
                }
            } else if code == AbsoluteAxisCode::ABS_MT_POSITION_X.0 {
                self.slots[self.current_slot].x = Some(value);
            } else if code == AbsoluteAxisCode::ABS_MT_POSITION_Y.0 {
                self.slots[self.current_slot].y = Some(value);
            }
        } else if event.event_type() == EventType::SYNCHRONIZATION && event.code() == 0 {
            self.pending_frames.push_back(self.build_frame(&event));
        }
    }

    fn build_frame(&self, event: &InputEvent) -> TouchFrame {
        let mut contacts: Vec<Contact> = self
            .slots
            .iter()
            .filter_map(|slot| {
                Some(Contact {
                    id: slot.tracking_id?,
                    x: normalize(slot.x?, self.x_min, self.x_max),
                    y: normalize(slot.y?, self.y_min, self.y_max),
                })
            })
            .collect();

        contacts.sort_by_key(|contact| contact.id);

        let timestamp_us = event
            .timestamp()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros()
            .min(u64::MAX as u128) as u64;

        TouchFrame {
            timestamp_us,
            contacts,
        }
    }
}

fn normalize(value: i32, min: i32, max: i32) -> f64 {
    if max <= min {
        return 0.0;
    }

    ((value - min) as f64 / (max - min) as f64).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_is_clamped() {
        assert_eq!(normalize(-10, 0, 100), 0.0);
        assert_eq!(normalize(50, 0, 100), 0.5);
        assert_eq!(normalize(110, 0, 100), 1.0);
    }
}
