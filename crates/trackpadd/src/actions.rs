use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};

pub trait ContinuousAction: Send {
    fn begin(&mut self) -> Result<()>;
    fn update(&mut self, delta: f64) -> Result<Option<f64>>;
    fn finish(&mut self) -> Result<()>;
    fn cancel(&mut self) -> Result<()>;
}

pub struct BrightnessAction {
    command: String,
    min: f64,
    max: f64,
    start_value: Option<f64>,
    last_sent_percent: Option<u32>,
}

impl BrightnessAction {
    pub fn new(command: String, min: f64, max: f64) -> Result<Self> {
        validate_range(min, max)?;
        Ok(Self {
            command,
            min,
            max,
            start_value: None,
            last_sent_percent: None,
        })
    }

    fn read_value(&self) -> Result<f64> {
        let output = Command::new(&self.command)
            .arg("-m")
            .output()
            .with_context(|| format!("failed to execute {}", self.command))?;

        if !output.status.success() {
            bail!(
                "{} -m exited with {}: {}",
                self.command,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout
            .lines()
            .find(|line| !line.trim().is_empty())
            .ok_or_else(|| anyhow!("{} -m returned no data", self.command))?;

        let percentage = line
            .split(',')
            .find_map(|part| part.trim().strip_suffix('%'))
            .and_then(|value| value.parse::<f64>().ok())
            .ok_or_else(|| anyhow!("could not parse brightness percentage from: {line}"))?;

        Ok((percentage / 100.0).clamp(0.0, 1.0))
    }

    fn write_value(&mut self, value: f64) -> Result<()> {
        let percent = (value.clamp(self.min, self.max) * 100.0).round() as u32;
        if self.last_sent_percent == Some(percent) {
            return Ok(());
        }

        let value = format!("{percent}%");
        let status = Command::new(&self.command)
            .args(["set", value.as_str()])
            .status()
            .with_context(|| format!("failed to execute {}", self.command))?;

        if !status.success() {
            bail!("{} set exited with {status}", self.command);
        }

        self.last_sent_percent = Some(percent);
        Ok(())
    }
}

impl ContinuousAction for BrightnessAction {
    fn begin(&mut self) -> Result<()> {
        let value = self.read_value()?;
        self.start_value = Some(value);
        self.last_sent_percent = Some((value * 100.0).round() as u32);
        Ok(())
    }

    fn update(&mut self, delta: f64) -> Result<Option<f64>> {
        let start = self
            .start_value
            .ok_or_else(|| anyhow!("brightness action updated before begin"))?;
        let target = (start + delta).clamp(self.min, self.max);
        self.write_value(target)?;
        Ok(Some(target))
    }

    fn finish(&mut self) -> Result<()> {
        self.start_value = None;
        self.last_sent_percent = None;
        Ok(())
    }

    fn cancel(&mut self) -> Result<()> {
        self.finish()
    }
}

pub struct VolumeAction {
    command: String,
    min: f64,
    max: f64,
    start_value: Option<f64>,
    last_sent_percent: Option<u32>,
}

impl VolumeAction {
    pub fn new(command: String, min: f64, max: f64) -> Result<Self> {
        validate_range(min, max)?;
        Ok(Self {
            command,
            min,
            max,
            start_value: None,
            last_sent_percent: None,
        })
    }

    fn read_value(&self) -> Result<f64> {
        let output = Command::new(&self.command)
            .env("LC_ALL", "C")
            .args(["get-volume", "@DEFAULT_AUDIO_SINK@"]) 
            .output()
            .with_context(|| format!("failed to execute {}", self.command))?;

        if !output.status.success() {
            bail!(
                "{} get-volume exited with {}: {}",
                self.command,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let value = stdout
            .split_whitespace()
            .find_map(|token| token.trim().replace(',', ".").parse::<f64>().ok())
            .ok_or_else(|| anyhow!("could not parse volume from: {}", stdout.trim()))?;

        Ok(value.clamp(0.0, 1.5))
    }

    fn write_value(&mut self, value: f64) -> Result<()> {
        let target = value.clamp(self.min, self.max);
        let percent = (target * 100.0).round() as u32;
        if self.last_sent_percent == Some(percent) {
            return Ok(());
        }

        let value = format!("{percent}%");
        let status = Command::new(&self.command)
            .args(["set-volume", "@DEFAULT_AUDIO_SINK@", value.as_str()])
            .status()
            .with_context(|| format!("failed to execute {}", self.command))?;

        if !status.success() {
            bail!("{} set-volume exited with {status}", self.command);
        }

        self.last_sent_percent = Some(percent);
        Ok(())
    }
}

impl ContinuousAction for VolumeAction {
    fn begin(&mut self) -> Result<()> {
        let value = self.read_value()?;
        self.start_value = Some(value);
        self.last_sent_percent = Some((value * 100.0).round() as u32);
        Ok(())
    }

    fn update(&mut self, delta: f64) -> Result<Option<f64>> {
        let start = self
            .start_value
            .ok_or_else(|| anyhow!("volume action updated before begin"))?;
        let target = (start + delta).clamp(self.min, self.max);
        self.write_value(target)?;
        Ok(Some(target))
    }

    fn finish(&mut self) -> Result<()> {
        self.start_value = None;
        self.last_sent_percent = None;
        Ok(())
    }

    fn cancel(&mut self) -> Result<()> {
        self.finish()
    }
}

pub struct PrintAction {
    label: String,
}

impl PrintAction {
    pub fn new(label: String) -> Self {
        Self { label }
    }
}

impl ContinuousAction for PrintAction {
    fn begin(&mut self) -> Result<()> {
        println!("ACTION {} started", self.label);
        Ok(())
    }

    fn update(&mut self, delta: f64) -> Result<Option<f64>> {
        println!("ACTION {} delta={delta:+.3}", self.label);
        Ok(None)
    }

    fn finish(&mut self) -> Result<()> {
        println!("ACTION {} ended", self.label);
        Ok(())
    }

    fn cancel(&mut self) -> Result<()> {
        println!("ACTION {} cancelled", self.label);
        Ok(())
    }
}

fn validate_range(min: f64, max: f64) -> Result<()> {
    if !(0.0..=1.5).contains(&min) || !(0.0..=1.5).contains(&max) || min >= max {
        bail!("invalid action range: min={min}, max={max}");
    }
    Ok(())
}