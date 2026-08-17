use std::{
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

    fn validate(&self) -> Result<()> {
        if let Some(device) = &self.device {
            device.validate()?;
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

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
pub enum EdgeConfig {
    Left,
    Right,
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct BindingConfig {
    pub gesture: String,
    pub action: String,
    #[serde(default = "default_sensitivity")]
    pub sensitivity: f64,
    #[serde(default)]
    pub invert: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Result<AppConfig> {
        let config: AppConfig = toml::from_str(source)?;
        config.validate()?;
        Ok(config)
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
}
