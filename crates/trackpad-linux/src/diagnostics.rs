use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use evdev::{AbsoluteAxisCode, Device, PropType};

#[derive(Debug, Clone)]
pub struct DeviceDiagnostic {
    pub path: PathBuf,
    pub readable: bool,
    pub name: Option<String>,
    pub vendor: Option<u16>,
    pub product: Option<u16>,
    pub pointer: Option<bool>,
    pub direct: Option<bool>,
    pub semi_mt: Option<bool>,
    pub required_mt_axes: usize,
    pub slots: Option<usize>,
    pub x_range: Option<(i32, i32)>,
    pub y_range: Option<(i32, i32)>,
    pub compatible: bool,
    pub issues: Vec<String>,
}

impl DeviceDiagnostic {
    pub fn is_touch_candidate(&self) -> bool {
        self.required_mt_axes > 0 || self.direct == Some(true) || self.semi_mt == Some(true)
    }
}

#[derive(Debug, Clone, Copy)]
struct CapabilitySnapshot {
    has_slot: bool,
    has_tracking_id: bool,
    has_x: bool,
    has_y: bool,
    direct: bool,
    semi_mt: bool,
    slot_range: Option<(i32, i32)>,
    x_range: Option<(i32, i32)>,
    y_range: Option<(i32, i32)>,
}

impl CapabilitySnapshot {
    fn required_axes_count(self) -> usize {
        [self.has_slot, self.has_tracking_id, self.has_x, self.has_y]
            .into_iter()
            .filter(|present| *present)
            .count()
    }

    fn slots(self) -> Option<usize> {
        self.slot_range.map(|(min, max)| {
            if max >= min {
                (max - min + 1) as usize
            } else {
                0
            }
        })
    }
}

pub fn diagnose_devices() -> Result<Vec<DeviceDiagnostic>> {
    let mut paths = fs::read_dir("/dev/input")
        .context("failed to read /dev/input while diagnosing input devices")?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            event_index(name).map(|index| (index, entry.path()))
        })
        .collect::<Vec<_>>();

    paths.sort_by_key(|(index, _)| *index);

    Ok(paths
        .into_iter()
        .map(|(_, path)| diagnose_path(path))
        .collect())
}

fn diagnose_path(path: PathBuf) -> DeviceDiagnostic {
    match Device::open(&path) {
        Ok(device) => diagnose_open_device(path, &device),
        Err(error) => DeviceDiagnostic {
            path,
            readable: false,
            name: None,
            vendor: None,
            product: None,
            pointer: None,
            direct: None,
            semi_mt: None,
            required_mt_axes: 0,
            slots: None,
            x_range: None,
            y_range: None,
            compatible: false,
            issues: vec![format!("cannot open device: {error}")],
        },
    }
}

fn diagnose_open_device(path: PathBuf, device: &Device) -> DeviceDiagnostic {
    let has_slot = has_axis(device, AbsoluteAxisCode::ABS_MT_SLOT);
    let has_tracking_id = has_axis(device, AbsoluteAxisCode::ABS_MT_TRACKING_ID);
    let has_x = has_axis(device, AbsoluteAxisCode::ABS_MT_POSITION_X);
    let has_y = has_axis(device, AbsoluteAxisCode::ABS_MT_POSITION_Y);

    let properties = device.properties();
    let direct = properties.contains(PropType::DIRECT);
    let semi_mt = properties.contains(PropType::SEMI_MT);

    let slot_range = axis_range(device, AbsoluteAxisCode::ABS_MT_SLOT, has_slot);
    let x_range = axis_range(device, AbsoluteAxisCode::ABS_MT_POSITION_X, has_x);
    let y_range = axis_range(device, AbsoluteAxisCode::ABS_MT_POSITION_Y, has_y);

    let snapshot = CapabilitySnapshot {
        has_slot,
        has_tracking_id,
        has_x,
        has_y,
        direct,
        semi_mt,
        slot_range,
        x_range,
        y_range,
    };

    let issues = compatibility_issues(snapshot);
    let input_id = device.input_id();

    DeviceDiagnostic {
        path,
        readable: true,
        name: Some(device.name().unwrap_or("<unnamed>").to_owned()),
        vendor: Some(input_id.vendor()),
        product: Some(input_id.product()),
        pointer: Some(properties.contains(PropType::POINTER)),
        direct: Some(direct),
        semi_mt: Some(semi_mt),
        required_mt_axes: snapshot.required_axes_count(),
        slots: snapshot.slots(),
        x_range,
        y_range,
        compatible: issues.is_empty(),
        issues,
    }
}

fn has_axis(device: &Device, code: AbsoluteAxisCode) -> bool {
    device
        .supported_absolute_axes()
        .is_some_and(|axes| axes.contains(code))
}

fn axis_range(device: &Device, code: AbsoluteAxisCode, supported: bool) -> Option<(i32, i32)> {
    if !supported {
        return None;
    }

    super::axis_info(device, code)
        .ok()
        .map(|info| (info.minimum(), info.maximum()))
}

fn compatibility_issues(snapshot: CapabilitySnapshot) -> Vec<String> {
    let mut issues = Vec::new();

    for (present, axis) in [
        (snapshot.has_slot, "ABS_MT_SLOT"),
        (snapshot.has_tracking_id, "ABS_MT_TRACKING_ID"),
        (snapshot.has_x, "ABS_MT_POSITION_X"),
        (snapshot.has_y, "ABS_MT_POSITION_Y"),
    ] {
        if !present {
            issues.push(format!("missing required axis {axis}"));
        }
    }

    if snapshot.direct {
        issues.push("DIRECT input property is set (likely a touchscreen)".to_string());
    }

    if snapshot.semi_mt {
        issues.push("SEMI_MT input property is set".to_string());
    }

    if snapshot.has_slot {
        match snapshot.slot_range {
            Some((min, max)) if max >= min => {}
            Some((min, max)) => issues.push(format!("invalid ABS_MT_SLOT range {min}..{max}")),
            None => issues.push("could not read ABS_MT_SLOT metadata".to_string()),
        }
    }

    if snapshot.has_x {
        match snapshot.x_range {
            Some((min, max)) if max > min => {}
            Some((min, max)) => {
                issues.push(format!("invalid ABS_MT_POSITION_X range {min}..{max}"))
            }
            None => issues.push("could not read ABS_MT_POSITION_X metadata".to_string()),
        }
    }

    if snapshot.has_y {
        match snapshot.y_range {
            Some((min, max)) if max > min => {}
            Some((min, max)) => {
                issues.push(format!("invalid ABS_MT_POSITION_Y range {min}..{max}"))
            }
            None => issues.push("could not read ABS_MT_POSITION_Y metadata".to_string()),
        }
    }

    issues
}

fn event_index(name: &str) -> Option<u32> {
    name.strip_prefix("event")?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_snapshot() -> CapabilitySnapshot {
        CapabilitySnapshot {
            has_slot: true,
            has_tracking_id: true,
            has_x: true,
            has_y: true,
            direct: false,
            semi_mt: false,
            slot_range: Some((0, 4)),
            x_range: Some((0, 1000)),
            y_range: Some((0, 700)),
        }
    }

    #[test]
    fn event_index_only_accepts_event_nodes() {
        assert_eq!(event_index("event0"), Some(0));
        assert_eq!(event_index("event12"), Some(12));
        assert_eq!(event_index("mouse0"), None);
        assert_eq!(event_index("eventX"), None);
    }

    #[test]
    fn valid_touchpad_capabilities_have_no_issues() {
        let snapshot = valid_snapshot();
        assert!(compatibility_issues(snapshot).is_empty());
        assert_eq!(snapshot.required_axes_count(), 4);
        assert_eq!(snapshot.slots(), Some(5));
    }

    #[test]
    fn diagnostics_explain_incompatible_capabilities() {
        let mut snapshot = valid_snapshot();
        snapshot.has_tracking_id = false;
        snapshot.direct = true;

        let issues = compatibility_issues(snapshot);

        assert!(issues
            .iter()
            .any(|issue| issue.contains("ABS_MT_TRACKING_ID")));
        assert!(issues.iter().any(|issue| issue.contains("DIRECT")));
    }

    #[test]
    fn invalid_coordinate_range_is_rejected() {
        let mut snapshot = valid_snapshot();
        snapshot.x_range = Some((100, 100));

        let issues = compatibility_issues(snapshot);
        assert!(issues
            .iter()
            .any(|issue| issue.contains("ABS_MT_POSITION_X range")));
    }
}
