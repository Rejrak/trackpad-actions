use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

const EXAMPLE_CONFIG: &str = include_str!("../../../config.example.toml");

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub device: Option<DeviceConfig>,
    #[serde(default)]
    pub gestures: Vec<GestureConfig>,
    #[serde(default)]
    pub actions: Vec<ActionConfig>,
    #[serde(default)]
    pub bindings: Vec<BindingConfig>,
}

impl AppConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config: Self = toml::from_str(&source)
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        config
            .validate()
            .with_context(|| format!("invalid config {}", path.display()))?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(device) = &self.device {
            device.validate()?;
        }

        let mut gesture_ids = HashSet::new();
        for gesture in &self.gestures {
            let id = gesture.id();
            validate_id("gesture", id)?;

            if !gesture_ids.insert(id.to_string()) {
                bail!("duplicate gesture id: {id}");
            }

            match gesture {
                GestureConfig::EdgeSwipe {
                    width,
                    cancel_margin,
                    ..
                } => {
                    validate_finite_range("gesture width", id, *width, 0.001, 0.49)?;
                    validate_finite_range("gesture cancel_margin", id, *cancel_margin, 0.0, 0.49)?;
                }
            }
        }

        let mut action_ids = HashSet::new();
        for action in &self.actions {
            let id = action.id();
            validate_id("action", id)?;

            if !action_ids.insert(id.to_string()) {
                bail!("duplicate action id: {id}");
            }

            match action {
                ActionConfig::Brightness {
                    command, min, max, ..
                }
                | ActionConfig::Volume {
                    command, min, max, ..
                } => {
                    if command.trim().is_empty() {
                        bail!("action '{id}' command must not be empty");
                    }
                    validate_action_range(id, *min, *max)?;
                }
                ActionConfig::Print { label, .. } => {
                    if label
                        .as_deref()
                        .is_some_and(|label| label.trim().is_empty())
                    {
                        bail!("print action '{id}' label must not be empty when provided");
                    }
                }
                ActionConfig::Command {
                    command, threshold, ..
                } => {
                    if command.trim().is_empty() {
                        bail!("command action '{id}' command must not be empty");
                    }
                    if !threshold.is_finite() || *threshold <= 0.0 {
                        bail!(
                            "command action '{id}' threshold must be a finite value > 0; got {threshold}"
                        );
                    }
                }
                ActionConfig::MediaSeek {
                    command,
                    seconds_per_full_swipe,
                    update_interval_ms,
                    deadzone,
                    curve,
                    ..
                } => {
                    if command.trim().is_empty() {
                        bail!("media-seek action '{id}' command must not be empty");
                    }
                    if !seconds_per_full_swipe.is_finite() || *seconds_per_full_swipe <= 0.0 {
                        bail!(
                            "media-seek action '{id}' seconds_per_full_swipe must be a finite value > 0; got {seconds_per_full_swipe}"
                        );
                    }
                    if *update_interval_ms == 0 {
                        bail!("media-seek action '{id}' update_interval_ms must be > 0");
                    }
                    if !deadzone.is_finite() || !(0.0..0.5).contains(deadzone) {
                        bail!(
                            "media-seek action '{id}' deadzone must be a finite value in [0, 0.5); got {deadzone}"
                        );
                    }
                    if !curve.is_finite() || *curve <= 0.0 {
                        bail!(
                            "media-seek action '{id}' curve must be a finite value > 0; got {curve}"
                        );
                    }
                }
            }
        }

        let mut binding_pairs = HashSet::new();
        for binding in &self.bindings {
            if binding.gesture.trim().is_empty() {
                bail!("binding gesture id must not be empty");
            }
            if binding.action.trim().is_empty() {
                bail!("binding action id must not be empty");
            }

            if !gesture_ids.contains(&binding.gesture) {
                bail!("binding references unknown gesture: {}", binding.gesture);
            }
            if !action_ids.contains(&binding.action) {
                bail!("binding references unknown action: {}", binding.action);
            }

            if !binding.sensitivity.is_finite() || binding.sensitivity <= 0.0 {
                bail!(
                    "binding gesture='{}' action='{}' has invalid sensitivity {}; expected a finite value > 0",
                    binding.gesture,
                    binding.action,
                    binding.sensitivity
                );
            }

            if let Some(deadzone) = binding.deadzone {
                if !deadzone.is_finite() || !(0.0..0.5).contains(&deadzone) {
                    bail!(
                        "binding gesture='{}' action='{}' has invalid deadzone {}; expected a finite value in [0, 0.5)",
                        binding.gesture,
                        binding.action,
                        deadzone
                    );
                }
            }

            if let Some(curve) = binding.curve {
                if !curve.is_finite() || curve <= 0.0 {
                    bail!(
                        "binding gesture='{}' action='{}' has invalid curve {}; expected a finite value > 0",
                        binding.gesture,
                        binding.action,
                        curve
                    );
                }
            }

            let pair = (binding.gesture.clone(), binding.action.clone());
            if !binding_pairs.insert(pair) {
                bail!(
                    "duplicate binding for gesture '{}' and action '{}'",
                    binding.gesture,
                    binding.action
                );
            }
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct DeviceConfig {
    pub name: Option<String>,
    pub vendor: Option<u16>,
    pub product: Option<u16>,
}

impl DeviceConfig {
    fn validate(&self) -> Result<()> {
        if self
            .name
            .as_deref()
            .is_some_and(|name| name.trim().is_empty())
        {
            bail!("[device].name must not be empty");
        }

        if self.name.is_none() && self.vendor.is_none() && self.product.is_none() {
            bail!("[device] must define at least one of: name, vendor, product");
        }

        Ok(())
    }

    pub fn matches(&self, name: &str, vendor: u16, product: u16) -> bool {
        self.name.as_deref().is_none_or(|expected| expected == name)
            && self.vendor.is_none_or(|expected| expected == vendor)
            && self.product.is_none_or(|expected| expected == product)
    }

    pub fn description(&self) -> String {
        let mut parts = Vec::new();

        if let Some(name) = &self.name {
            parts.push(format!("name={name:?}"));
        }
        if let Some(vendor) = self.vendor {
            parts.push(format!("vendor=0x{vendor:04x}"));
        }
        if let Some(product) = self.product {
            parts.push(format!("product=0x{product:04x}"));
        }

        parts.join(" ")
    }
}

pub fn user_config_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path).join("trackpadd/config.toml"));
    }

    let home = env::var_os("HOME").ok_or_else(|| {
        anyhow!("cannot determine config directory: neither XDG_CONFIG_HOME nor HOME is set")
    })?;

    Ok(PathBuf::from(home).join(".config/trackpadd/config.toml"))
}

pub fn write_default_user_config(force: bool) -> Result<PathBuf> {
    let path = user_config_path()?;

    if path.exists() && !force {
        bail!(
            "{} already exists; use --force to overwrite it",
            path.display()
        );
    }

    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("invalid config path: {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;
    fs::write(&path, EXAMPLE_CONFIG)
        .with_context(|| format!("failed to write {}", path.display()))?;

    Ok(path)
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum GestureConfig {
    EdgeSwipe {
        id: String,
        edge: EdgeConfig,
        #[serde(default = "default_width")]
        width: f64,
        #[serde(default = "default_cancel_margin")]
        cancel_margin: f64,
    },
}

impl GestureConfig {
    fn id(&self) -> &str {
        match self {
            Self::EdgeSwipe { id, .. } => id,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeConfig {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ActionConfig {
    Brightness {
        id: String,
        #[serde(default = "default_brightness_command")]
        command: String,
        #[serde(default = "default_brightness_min")]
        min: f64,
        #[serde(default = "default_max")]
        max: f64,
    },
    Volume {
        id: String,
        #[serde(default = "default_volume_command")]
        command: String,
        #[serde(default)]
        min: f64,
        #[serde(default = "default_max")]
        max: f64,
    },
    Print {
        id: String,
        label: Option<String>,
    },
    Command {
        id: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        trigger: CommandTriggerConfig,
        #[serde(default)]
        direction: CommandDirectionConfig,
        #[serde(default = "default_command_threshold")]
        threshold: f64,
    },
    MediaSeek {
        id: String,
        #[serde(default = "default_media_seek_command")]
        command: String,
        #[serde(default = "default_media_seek_seconds_per_full_swipe")]
        seconds_per_full_swipe: f64,
        #[serde(default = "default_media_seek_update_interval_ms")]
        update_interval_ms: u64,
        #[serde(default = "default_media_seek_deadzone")]
        deadzone: f64,
        #[serde(default = "default_media_seek_curve")]
        curve: f64,
    },
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CommandTriggerConfig {
    Start,
    #[default]
    End,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CommandDirectionConfig {
    #[default]
    Any,
    Up,
    Down,
    Left,
    Right,
}

impl ActionConfig {
    fn id(&self) -> &str {
        match self {
            Self::Brightness { id, .. }
            | Self::Volume { id, .. }
            | Self::Print { id, .. }
            | Self::Command { id, .. }
            | Self::MediaSeek { id, .. } => id,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct BindingConfig {
    pub gesture: String,
    pub action: String,
    #[serde(default = "default_sensitivity")]
    pub sensitivity: f64,
    #[serde(default)]
    pub invert: bool,
    #[serde(default)]
    pub deadzone: Option<f64>,
    #[serde(default)]
    pub curve: Option<f64>,
}

impl BindingConfig {
    pub fn transform_delta(&self, delta: f64, legacy_media_response: Option<(f64, f64)>) -> f64 {
        let sign = if self.invert { -1.0 } else { 1.0 };

        if self.deadzone.is_some() || self.curve.is_some() {
            let deadzone = self.deadzone.unwrap_or(0.0);
            let curve = self.curve.unwrap_or(1.0);
            return shape_delta(delta, deadzone, curve) * self.sensitivity * sign;
        }

        let transformed = delta * self.sensitivity * sign;
        match legacy_media_response {
            Some((deadzone, curve)) => shape_delta(transformed, deadzone, curve),
            None => transformed,
        }
    }
}

fn shape_delta(delta: f64, deadzone: f64, curve: f64) -> f64 {
    let magnitude = delta.abs();
    if magnitude <= deadzone {
        return 0.0;
    }

    let normalized = ((magnitude - deadzone) / (1.0 - deadzone)).clamp(0.0, 1.0);
    delta.signum() * normalized.powf(curve)
}

fn validate_id(kind: &str, id: &str) -> Result<()> {
    if id.trim().is_empty() {
        bail!("{kind} id must not be empty");
    }
    Ok(())
}

fn validate_finite_range(label: &str, id: &str, value: f64, min: f64, max: f64) -> Result<()> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        bail!("{label} for '{id}' must be between {min} and {max}; got {value}");
    }
    Ok(())
}

fn validate_action_range(id: &str, min: f64, max: f64) -> Result<()> {
    if !min.is_finite()
        || !max.is_finite()
        || !(0.0..=1.5).contains(&min)
        || !(0.0..=1.5).contains(&max)
        || min >= max
    {
        bail!("action '{id}' has invalid range: min={min}, max={max}");
    }
    Ok(())
}

fn default_width() -> f64 {
    0.06
}

fn default_cancel_margin() -> f64 {
    0.04
}

fn default_sensitivity() -> f64 {
    1.0
}

fn default_brightness_min() -> f64 {
    0.05
}

fn default_max() -> f64 {
    1.0
}

fn default_brightness_command() -> String {
    "brightnessctl".to_string()
}

fn default_volume_command() -> String {
    "wpctl".to_string()
}

fn default_command_threshold() -> f64 {
    0.10
}

fn default_media_seek_command() -> String {
    "playerctl".to_string()
}

fn default_media_seek_seconds_per_full_swipe() -> f64 {
    60.0
}

fn default_media_seek_update_interval_ms() -> u64 {
    50
}

fn default_media_seek_deadzone() -> f64 {
    0.025
}

fn default_media_seek_curve() -> f64 {
    1.4
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Result<AppConfig> {
        let config: AppConfig = toml::from_str(source)?;
        config.validate()?;
        Ok(config)
    }

    const VALID_CONFIG: &str = r#"
        [device]
        vendor = 0x04f3
        product = 0x3140

        [[gestures]]
        id = "right-edge"
        type = "edge-swipe"
        edge = "right"
        width = 0.06
        cancel_margin = 0.04

        [[actions]]
        id = "brightness"
        type = "brightness"
        command = "brightnessctl"
        min = 0.05
        max = 1.0

        [[bindings]]
        gesture = "right-edge"
        action = "brightness"
        sensitivity = 1.0
    "#;

    #[test]
    fn valid_config_passes_validation() {
        parse(VALID_CONFIG).unwrap();
    }

    #[test]
    fn device_selector_parses_hex_vendor_and_product() {
        let config = parse(
            r#"
            [device]
            vendor = 0x04f3
            product = 0x3140
            "#,
        )
        .unwrap();

        let device = config.device.unwrap();
        assert_eq!(device.vendor, Some(0x04f3));
        assert_eq!(device.product, Some(0x3140));
    }

    #[test]
    fn empty_device_selector_is_rejected() {
        let error = parse("[device]\n").unwrap_err();
        assert!(error
            .to_string()
            .contains("[device] must define at least one of"));
    }

    #[test]
    fn device_selector_matches_all_configured_fields() {
        let selector = DeviceConfig {
            name: Some("Example Touchpad".to_string()),
            vendor: Some(0x1234),
            product: Some(0xabcd),
        };

        assert!(selector.matches("Example Touchpad", 0x1234, 0xabcd));
        assert!(!selector.matches("Other Touchpad", 0x1234, 0xabcd));
        assert!(!selector.matches("Example Touchpad", 0x9999, 0xabcd));
    }

    #[test]
    fn top_edge_gesture_config_parses() {
        let config = parse(
            r#"
            [[gestures]]
            id = "top-edge"
            type = "edge-swipe"
            edge = "top"
            width = 0.06
            cancel_margin = 0.04
            "#,
        )
        .unwrap();

        assert!(matches!(
            config.gestures.as_slice(),
            [GestureConfig::EdgeSwipe {
                id,
                edge: EdgeConfig::Top,
                ..
            }] if id == "top-edge"
        ));
    }

    #[test]
    fn bottom_edge_gesture_config_parses() {
        let config = parse(
            r#"
            [[gestures]]
            id = "bottom-edge"
            type = "edge-swipe"
            edge = "bottom"
            width = 0.06
            cancel_margin = 0.04
            "#,
        )
        .unwrap();

        assert!(matches!(
            config.gestures.as_slice(),
            [GestureConfig::EdgeSwipe {
                id,
                edge: EdgeConfig::Bottom,
                ..
            }] if id == "bottom-edge"
        ));
    }

    #[test]
    fn duplicate_gesture_id_is_rejected() {
        let error = parse(
            r#"
            [[gestures]]
            id = "edge"
            type = "edge-swipe"
            edge = "left"

            [[gestures]]
            id = "edge"
            type = "edge-swipe"
            edge = "right"
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("duplicate gesture id: edge"));
    }

    #[test]
    fn duplicate_action_id_is_rejected() {
        let error = parse(
            r#"
            [[actions]]
            id = "volume"
            type = "volume"

            [[actions]]
            id = "volume"
            type = "print"
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("duplicate action id: volume"));
    }

    #[test]
    fn binding_to_unknown_gesture_is_rejected() {
        let error = parse(
            r#"
            [[actions]]
            id = "debug"
            type = "print"

            [[bindings]]
            gesture = "missing"
            action = "debug"
            "#,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("binding references unknown gesture: missing"));
    }

    #[test]
    fn duplicate_binding_is_rejected() {
        let error = parse(
            r#"
            [[gestures]]
            id = "edge"
            type = "edge-swipe"
            edge = "left"

            [[actions]]
            id = "debug"
            type = "print"

            [[bindings]]
            gesture = "edge"
            action = "debug"

            [[bindings]]
            gesture = "edge"
            action = "debug"
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("duplicate binding"));
    }

    #[test]
    fn invalid_gesture_width_is_rejected() {
        let error = parse(
            r#"
            [[gestures]]
            id = "edge"
            type = "edge-swipe"
            edge = "left"
            width = 0.75
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("gesture width"));
    }

    #[test]
    fn non_positive_sensitivity_is_rejected() {
        let error = parse(
            r#"
            [[gestures]]
            id = "edge"
            type = "edge-swipe"
            edge = "left"

            [[actions]]
            id = "debug"
            type = "print"

            [[bindings]]
            gesture = "edge"
            action = "debug"
            sensitivity = 0.0
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("invalid sensitivity"));
    }

    #[test]
    fn binding_response_fields_default_to_legacy_mode() {
        let config = parse(VALID_CONFIG).unwrap();
        let binding = &config.bindings[0];

        assert_eq!(binding.deadzone, None);
        assert_eq!(binding.curve, None);
        assert!((binding.transform_delta(0.25, None) - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn binding_response_fields_parse_and_shape_before_sensitivity() {
        let config = parse(
            r#"
            [[gestures]]
            id = "edge"
            type = "edge-swipe"
            edge = "left"

            [[actions]]
            id = "debug"
            type = "print"

            [[bindings]]
            gesture = "edge"
            action = "debug"
            sensitivity = 1.5
            invert = true
            deadzone = 0.05
            curve = 2.0
            "#,
        )
        .unwrap();

        let binding = &config.bindings[0];
        assert_eq!(binding.deadzone, Some(0.05));
        assert_eq!(binding.curve, Some(2.0));
        assert_eq!(binding.transform_delta(0.04, None), 0.0);

        let transformed = binding.transform_delta(0.50, None);
        assert!(transformed < 0.0);
        assert!(transformed.abs() < 0.50);
    }

    #[test]
    fn binding_response_rejects_invalid_values() {
        for invalid in [
            "deadzone = -0.01",
            "deadzone = 0.5",
            "curve = 0",
            "curve = -1",
        ] {
            let source = format!(
                r#"
                [[gestures]]
                id = "edge"
                type = "edge-swipe"
                edge = "left"

                [[actions]]
                id = "debug"
                type = "print"

                [[bindings]]
                gesture = "edge"
                action = "debug"
                {invalid}
                "#
            );

            assert!(
                parse(&source).is_err(),
                "expected invalid binding response: {invalid}"
            );
        }
    }

    #[test]
    fn legacy_media_response_preserves_v03_transform_order() {
        let legacy = BindingConfig {
            gesture: "top-edge".to_string(),
            action: "media-position".to_string(),
            sensitivity: 2.0,
            invert: false,
            deadzone: None,
            curve: None,
        };

        assert!(legacy
            .transform_delta(0.020, Some((0.025, 1.4)))
            .is_sign_positive());

        let explicit = BindingConfig {
            deadzone: Some(0.025),
            curve: Some(1.4),
            ..legacy.clone()
        };

        assert_eq!(explicit.transform_delta(0.020, Some((0.025, 1.4))), 0.0);
    }

    #[test]
    fn explicit_binding_response_overrides_legacy_media_fallback() {
        let binding = BindingConfig {
            gesture: "top-edge".to_string(),
            action: "media-position".to_string(),
            sensitivity: 1.0,
            invert: false,
            deadzone: Some(0.0),
            curve: Some(1.0),
        };

        let transformed = binding.transform_delta(0.10, Some((0.20, 3.0)));
        assert!((transformed - 0.10).abs() < f64::EPSILON);
    }

    #[test]
    fn command_action_defaults_are_valid() {
        let config = parse(
            r#"
            [[gestures]]
            id = "edge"
            type = "edge-swipe"
            edge = "left"

            [[actions]]
            id = "lock"
            type = "command"
            command = "loginctl"
            args = ["lock-session"]

            [[bindings]]
            gesture = "edge"
            action = "lock"
            "#,
        )
        .unwrap();

        let ActionConfig::Command {
            trigger,
            direction,
            threshold,
            ..
        } = &config.actions[0]
        else {
            panic!("expected command action");
        };

        assert_eq!(*trigger, CommandTriggerConfig::End);
        assert_eq!(*direction, CommandDirectionConfig::Any);
        assert!((*threshold - 0.10).abs() < f64::EPSILON);
    }

    #[test]
    fn command_action_parses_direction_and_trigger() {
        let config = parse(
            r#"
            [[gestures]]
            id = "edge"
            type = "edge-swipe"
            edge = "left"

            [[actions]]
            id = "workspace"
            type = "command"
            command = "example"
            args = ["next"]
            trigger = "end"
            direction = "up"
            threshold = 0.20

            [[bindings]]
            gesture = "edge"
            action = "workspace"
            "#,
        )
        .unwrap();

        let ActionConfig::Command {
            trigger,
            direction,
            threshold,
            ..
        } = &config.actions[0]
        else {
            panic!("expected command action");
        };

        assert_eq!(*trigger, CommandTriggerConfig::End);
        assert_eq!(*direction, CommandDirectionConfig::Up);
        assert!((*threshold - 0.20).abs() < f64::EPSILON);
    }

    #[test]
    fn command_action_empty_command_is_rejected() {
        let error = parse(
            r#"
            [[actions]]
            id = "broken"
            type = "command"
            command = "   "
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("command must not be empty"));
    }

    #[test]
    fn command_action_non_positive_threshold_is_rejected() {
        let error = parse(
            r#"
            [[actions]]
            id = "broken"
            type = "command"
            command = "true"
            threshold = 0.0
            "#,
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("threshold must be a finite value > 0"));
    }

    #[test]
    fn media_seek_action_defaults_are_valid() {
        let config = parse(
            r#"
            [[actions]]
            id = "media"
            type = "media-seek"
            "#,
        )
        .unwrap();

        let ActionConfig::MediaSeek {
            command,
            seconds_per_full_swipe,
            update_interval_ms,
            deadzone,
            curve,
            ..
        } = &config.actions[0]
        else {
            panic!("expected media-seek action");
        };

        assert_eq!(command, "playerctl");
        assert!((*seconds_per_full_swipe - 60.0).abs() < f64::EPSILON);
        assert_eq!(*update_interval_ms, 50);
        assert!((*deadzone - 0.025).abs() < f64::EPSILON);
        assert!((*curve - 1.4).abs() < f64::EPSILON);
    }

    #[test]
    fn media_seek_action_parses_custom_values() {
        let config = parse(
            r#"
            [[actions]]
            id = "media"
            type = "media-seek"
            command = "playerctl"
            seconds_per_full_swipe = 90
            update_interval_ms = 75
            deadzone = 0.03
            curve = 1.6
            "#,
        )
        .unwrap();

        let ActionConfig::MediaSeek {
            seconds_per_full_swipe,
            update_interval_ms,
            deadzone,
            curve,
            ..
        } = &config.actions[0]
        else {
            panic!("expected media-seek action");
        };

        assert!((*seconds_per_full_swipe - 90.0).abs() < f64::EPSILON);
        assert_eq!(*update_interval_ms, 75);
        assert!((*deadzone - 0.03).abs() < f64::EPSILON);
        assert!((*curve - 1.6).abs() < f64::EPSILON);
    }

    #[test]
    fn media_seek_action_rejects_invalid_values() {
        for invalid in [
            "seconds_per_full_swipe = 0",
            "update_interval_ms = 0",
            "deadzone = 0.5",
            "curve = 0",
        ] {
            let source = format!(
                r#"
                [[actions]]
                id = "media"
                type = "media-seek"
                {invalid}
                "#
            );
            assert!(
                parse(&source).is_err(),
                "expected invalid config: {invalid}"
            );
        }
    }
}
