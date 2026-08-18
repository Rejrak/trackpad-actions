use std::{
    process::Command,
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct ActionValue {
    pub kind: &'static str,
    pub value: f64,
    pub max_value: f64,
    pub unit: &'static str,
}

impl ActionValue {
    fn percent(kind: &'static str, percent: u32, max_percent: u32) -> Self {
        Self {
            kind,
            value: f64::from(percent),
            max_value: f64::from(max_percent),
            unit: "percent",
        }
    }

    fn seconds(position: f64, duration: Option<f64>) -> Self {
        Self {
            kind: "media-position",
            value: position,
            max_value: duration.unwrap_or(0.0),
            unit: "seconds",
        }
    }
}

pub trait ContinuousAction: Send {
    fn begin(&mut self) -> Result<()>;
    fn update(&mut self, delta: f64) -> Result<Option<ActionValue>>;
    fn finish(&mut self) -> Result<Option<ActionValue>>;
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

    fn write_value(&mut self, value: f64) -> Result<Option<u32>> {
        let percent = (value.clamp(self.min, self.max) * 100.0).round() as u32;
        if self.last_sent_percent == Some(percent) {
            return Ok(None);
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
        Ok(Some(percent))
    }
}

impl ContinuousAction for BrightnessAction {
    fn begin(&mut self) -> Result<()> {
        let value = self.read_value()?;
        self.start_value = Some(value);
        self.last_sent_percent = Some((value * 100.0).round() as u32);
        Ok(())
    }

    fn update(&mut self, delta: f64) -> Result<Option<ActionValue>> {
        let start = self
            .start_value
            .ok_or_else(|| anyhow!("brightness action updated before begin"))?;
        let target = (start + delta).clamp(self.min, self.max);
        let max_percent = (self.max * 100.0).round() as u32;
        Ok(self
            .write_value(target)?
            .map(|percent| ActionValue::percent("brightness", percent, max_percent)))
    }

    fn finish(&mut self) -> Result<Option<ActionValue>> {
        self.start_value = None;
        self.last_sent_percent = None;
        Ok(None)
    }

    fn cancel(&mut self) -> Result<()> {
        self.start_value = None;
        self.last_sent_percent = None;
        Ok(())
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

    fn write_value(&mut self, value: f64) -> Result<Option<u32>> {
        let target = value.clamp(self.min, self.max);
        let percent = (target * 100.0).round() as u32;
        if self.last_sent_percent == Some(percent) {
            return Ok(None);
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
        Ok(Some(percent))
    }
}

impl ContinuousAction for VolumeAction {
    fn begin(&mut self) -> Result<()> {
        let value = self.read_value()?;
        self.start_value = Some(value);
        self.last_sent_percent = Some((value * 100.0).round() as u32);
        Ok(())
    }

    fn update(&mut self, delta: f64) -> Result<Option<ActionValue>> {
        let start = self
            .start_value
            .ok_or_else(|| anyhow!("volume action updated before begin"))?;
        let target = (start + delta).clamp(self.min, self.max);
        let max_percent = (self.max * 100.0).round() as u32;
        Ok(self
            .write_value(target)?
            .map(|percent| ActionValue::percent("volume", percent, max_percent)))
    }

    fn finish(&mut self) -> Result<Option<ActionValue>> {
        self.start_value = None;
        self.last_sent_percent = None;
        Ok(None)
    }

    fn cancel(&mut self) -> Result<()> {
        self.start_value = None;
        self.last_sent_percent = None;
        Ok(())
    }
}

pub struct MediaSeekAction {
    command: String,
    seconds_per_full_swipe: f64,
    update_interval: Duration,
    deadzone: f64,
    curve: f64,
    start_position: Option<f64>,
    duration: Option<f64>,
    last_delta: f64,
    last_update: Option<Instant>,
    last_sent_position: Option<f64>,
}

impl MediaSeekAction {
    pub fn new(
        command: String,
        seconds_per_full_swipe: f64,
        update_interval_ms: u64,
        deadzone: f64,
        curve: f64,
    ) -> Result<Self> {
        if command.trim().is_empty() {
            bail!("media-seek command must not be empty");
        }
        if !seconds_per_full_swipe.is_finite() || seconds_per_full_swipe <= 0.0 {
            bail!("media-seek seconds_per_full_swipe must be a finite value > 0");
        }
        if update_interval_ms == 0 {
            bail!("media-seek update_interval_ms must be > 0");
        }
        if !deadzone.is_finite() || !(0.0..0.5).contains(&deadzone) {
            bail!("media-seek deadzone must be a finite value in [0, 0.5)");
        }
        if !curve.is_finite() || curve <= 0.0 {
            bail!("media-seek curve must be a finite value > 0");
        }

        Ok(Self {
            command,
            seconds_per_full_swipe,
            update_interval: Duration::from_millis(update_interval_ms),
            deadzone,
            curve,
            start_position: None,
            duration: None,
            last_delta: 0.0,
            last_update: None,
            last_sent_position: None,
        })
    }

    fn read_position(&self) -> Result<f64> {
        let output = Command::new(&self.command)
            .env("LC_ALL", "C")
            .arg("position")
            .output()
            .with_context(|| format!("failed to execute {}", self.command))?;

        if !output.status.success() {
            bail!(
                "{} position exited with {}: {}",
                self.command,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let position = stdout
            .trim()
            .parse::<f64>()
            .with_context(|| format!("could not parse media position from: {}", stdout.trim()))?;

        if !position.is_finite() || position < 0.0 {
            bail!("media position must be a finite non-negative value; got {position}");
        }

        Ok(position)
    }

    fn read_duration(&self) -> Option<f64> {
        let output = Command::new(&self.command)
            .env("LC_ALL", "C")
            .args(["metadata", "--format", "{{mpris:length}}"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        parse_mpris_length(&String::from_utf8_lossy(&output.stdout))
    }

    fn shaped_delta(&self, delta: f64) -> f64 {
        let magnitude = delta.abs();
        if magnitude <= self.deadzone {
            return 0.0;
        }

        let normalized = ((magnitude - self.deadzone) / (1.0 - self.deadzone)).clamp(0.0, 1.0);
        delta.signum() * normalized.powf(self.curve)
    }

    fn target_position(&self, delta: f64) -> Result<f64> {
        let start = self
            .start_position
            .ok_or_else(|| anyhow!("media-seek action updated before begin"))?;
        let offset = self.shaped_delta(delta) * self.seconds_per_full_swipe;
        let target = (start + offset).max(0.0);

        Ok(match self.duration {
            Some(duration) => target.min(duration),
            None => target,
        })
    }

    fn should_update(&self) -> bool {
        match self.last_update {
            None => true,
            Some(last_update) => last_update.elapsed() >= self.update_interval,
        }
    }

    fn write_position(&mut self, position: f64) -> Result<bool> {
        if self
            .last_sent_position
            .is_some_and(|last| (last - position).abs() < 0.05)
        {
            return Ok(false);
        }

        let position_arg = format!("{position:.3}");
        let status = Command::new(&self.command)
            .env("LC_ALL", "C")
            .args(["position", position_arg.as_str()])
            .status()
            .with_context(|| format!("failed to execute {}", self.command))?;

        if !status.success() {
            bail!("{} position exited with {status}", self.command);
        }

        self.last_sent_position = Some(position);
        Ok(true)
    }

    fn reset(&mut self) {
        self.start_position = None;
        self.duration = None;
        self.last_delta = 0.0;
        self.last_update = None;
        self.last_sent_position = None;
    }
}

impl ContinuousAction for MediaSeekAction {
    fn begin(&mut self) -> Result<()> {
        self.reset();
        let position = self.read_position()?;
        self.start_position = Some(position);
        self.duration = self.read_duration();
        self.last_sent_position = Some(position);
        Ok(())
    }

    fn update(&mut self, delta: f64) -> Result<Option<ActionValue>> {
        self.last_delta = delta;

        if !self.should_update() {
            return Ok(None);
        }

        let target = self.target_position(delta)?;
        let changed = self.write_position(target)?;
        self.last_update = Some(Instant::now());

        Ok(changed.then(|| ActionValue::seconds(target, self.duration)))
    }

    fn finish(&mut self) -> Result<Option<ActionValue>> {
        let update = if self.start_position.is_some() {
            let target = self.target_position(self.last_delta)?;
            self.write_position(target)?
                .then(|| ActionValue::seconds(target, self.duration))
        } else {
            None
        };

        self.reset();
        Ok(update)
    }

    fn cancel(&mut self) -> Result<()> {
        self.reset();
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandTrigger {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandDirection {
    Any,
    Up,
    Down,
    Left,
    Right,
}

pub struct CommandAction {
    id: String,
    command: String,
    args: Vec<String>,
    trigger: CommandTrigger,
    direction: CommandDirection,
    threshold: f64,
    max_delta: f64,
    min_delta: f64,
    executed: bool,
}

impl CommandAction {
    pub fn new(
        id: String,
        command: String,
        args: Vec<String>,
        trigger: CommandTrigger,
        direction: CommandDirection,
        threshold: f64,
    ) -> Result<Self> {
        if command.trim().is_empty() {
            bail!("command action command must not be empty");
        }
        if !threshold.is_finite() || threshold <= 0.0 {
            bail!("command action threshold must be a finite value > 0");
        }

        Ok(Self {
            id,
            command,
            args,
            trigger,
            direction,
            threshold,
            max_delta: 0.0,
            min_delta: 0.0,
            executed: false,
        })
    }

    fn direction_matches(&self) -> bool {
        match self.direction {
            CommandDirection::Any => {
                self.max_delta >= self.threshold || self.min_delta <= -self.threshold
            }
            CommandDirection::Up | CommandDirection::Right => self.max_delta >= self.threshold,
            CommandDirection::Down | CommandDirection::Left => self.min_delta <= -self.threshold,
        }
    }

    fn spawn_command(&mut self) -> Result<()> {
        if self.executed {
            return Ok(());
        }

        let mut child = Command::new(&self.command)
            .args(&self.args)
            .spawn()
            .with_context(|| format!("failed to execute command action '{}'", self.id))?;

        let action_id = self.id.clone();
        let pid = child.id();
        println!("COMMAND action={} pid={pid}", self.id);

        std::thread::spawn(move || match child.wait() {
            Ok(status) if !status.success() => {
                eprintln!("COMMAND ERROR action='{action_id}' pid={pid} exited with {status}");
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("COMMAND ERROR action='{action_id}' pid={pid} wait failed: {error}");
            }
        });

        self.executed = true;
        Ok(())
    }

    fn reset(&mut self) {
        self.max_delta = 0.0;
        self.min_delta = 0.0;
        self.executed = false;
    }
}

impl ContinuousAction for CommandAction {
    fn begin(&mut self) -> Result<()> {
        self.reset();

        if self.trigger == CommandTrigger::Start {
            self.spawn_command()?;
        }

        Ok(())
    }

    fn update(&mut self, delta: f64) -> Result<Option<ActionValue>> {
        self.max_delta = self.max_delta.max(delta);
        self.min_delta = self.min_delta.min(delta);
        Ok(None)
    }

    fn finish(&mut self) -> Result<Option<ActionValue>> {
        if self.trigger == CommandTrigger::End && self.direction_matches() {
            self.spawn_command()?;
        }

        self.reset();
        Ok(None)
    }

    fn cancel(&mut self) -> Result<()> {
        self.reset();
        Ok(())
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

    fn update(&mut self, delta: f64) -> Result<Option<ActionValue>> {
        println!("ACTION {} delta={delta:+.3}", self.label);
        Ok(None)
    }

    fn finish(&mut self) -> Result<Option<ActionValue>> {
        println!("ACTION {} ended", self.label);
        Ok(None)
    }

    fn cancel(&mut self) -> Result<()> {
        println!("ACTION {} cancelled", self.label);
        Ok(())
    }
}

fn parse_mpris_length(raw: &str) -> Option<f64> {
    let microseconds = raw.trim().parse::<f64>().ok()?;
    if !microseconds.is_finite() || microseconds <= 0.0 {
        return None;
    }

    Some(microseconds / 1_000_000.0)
}

fn validate_range(min: f64, max: f64) -> Result<()> {
    if !(0.0..=1.5).contains(&min) || !(0.0..=1.5).contains(&max) || min >= max {
        bail!("invalid action range: min={min}, max={max}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(direction: CommandDirection, threshold: f64) -> CommandAction {
        CommandAction::new(
            "test".to_string(),
            "true".to_string(),
            Vec::new(),
            CommandTrigger::End,
            direction,
            threshold,
        )
        .unwrap()
    }

    #[test]
    fn command_direction_any_uses_absolute_delta() {
        let mut action = action(CommandDirection::Any, 0.10);
        action.min_delta = -0.20;
        assert!(action.direction_matches());
    }

    #[test]
    fn command_direction_up_requires_positive_threshold() {
        let mut action = action(CommandDirection::Up, 0.10);
        action.max_delta = 0.11;
        assert!(action.direction_matches());

        action.max_delta = 0.0;
        action.min_delta = -0.50;
        assert!(!action.direction_matches());
    }

    #[test]
    fn command_direction_down_requires_negative_threshold() {
        let mut action = action(CommandDirection::Down, 0.10);
        action.min_delta = -0.11;
        assert!(action.direction_matches());

        action.min_delta = 0.0;
        action.max_delta = 0.50;
        assert!(!action.direction_matches());
    }

    #[test]
    fn command_direction_right_uses_positive_threshold() {
        let mut action = action(CommandDirection::Right, 0.10);
        action.max_delta = 0.11;
        assert!(action.direction_matches());

        action.max_delta = 0.0;
        action.min_delta = -0.50;
        assert!(!action.direction_matches());
    }

    #[test]
    fn command_direction_left_uses_negative_threshold() {
        let mut action = action(CommandDirection::Left, 0.10);
        action.min_delta = -0.11;
        assert!(action.direction_matches());

        action.min_delta = 0.0;
        action.max_delta = 0.50;
        assert!(!action.direction_matches());
    }

    #[test]
    fn media_seek_deadzone_filters_small_motion() {
        let action = MediaSeekAction::new("playerctl".to_string(), 60.0, 50, 0.025, 1.4).unwrap();

        assert_eq!(action.shaped_delta(0.020), 0.0);
        assert_eq!(action.shaped_delta(-0.020), 0.0);
        assert!(action.shaped_delta(0.25) > 0.0);
        assert!(action.shaped_delta(-0.25) < 0.0);
    }

    #[test]
    fn media_seek_curve_preserves_full_scale_and_direction() {
        let action = MediaSeekAction::new("playerctl".to_string(), 60.0, 50, 0.025, 1.4).unwrap();

        assert!((action.shaped_delta(1.0) - 1.0).abs() < f64::EPSILON);
        assert!((action.shaped_delta(-1.0) + 1.0).abs() < f64::EPSILON);
        assert!(action.shaped_delta(0.25) < 0.25);
    }

    #[test]
    fn media_seek_rejects_invalid_parameters() {
        assert!(MediaSeekAction::new("".to_string(), 60.0, 50, 0.025, 1.4).is_err());
        assert!(MediaSeekAction::new("playerctl".to_string(), 0.0, 50, 0.025, 1.4).is_err());
        assert!(MediaSeekAction::new("playerctl".to_string(), 60.0, 0, 0.025, 1.4).is_err());
        assert!(MediaSeekAction::new("playerctl".to_string(), 60.0, 50, 0.5, 1.4).is_err());
        assert!(MediaSeekAction::new("playerctl".to_string(), 60.0, 50, 0.025, 0.0).is_err());
    }

    #[test]
    fn action_values_expose_explicit_units_and_maximums() {
        let brightness = ActionValue::percent("brightness", 73, 100);
        assert_eq!(brightness.kind, "brightness");
        assert_eq!(brightness.value, 73.0);
        assert_eq!(brightness.max_value, 100.0);
        assert_eq!(brightness.unit, "percent");

        let media = ActionValue::seconds(123.456, Some(312.0));
        assert_eq!(media.kind, "media-position");
        assert_eq!(media.value, 123.456);
        assert_eq!(media.max_value, 312.0);
        assert_eq!(media.unit, "seconds");

        let unknown_duration = ActionValue::seconds(12.0, None);
        assert_eq!(unknown_duration.max_value, 0.0);
    }

    #[test]
    fn mpris_length_is_converted_from_microseconds_to_seconds() {
        assert_eq!(parse_mpris_length("312000000\n"), Some(312.0));
        assert_eq!(parse_mpris_length("136400000"), Some(136.4));
        assert_eq!(parse_mpris_length(""), None);
        assert_eq!(parse_mpris_length("not-a-number"), None);
        assert_eq!(parse_mpris_length("0"), None);
        assert_eq!(parse_mpris_length("-1"), None);
    }

    #[test]
    fn media_target_is_clamped_to_known_duration() {
        let mut action =
            MediaSeekAction::new("playerctl".to_string(), 60.0, 50, 0.025, 1.4).unwrap();
        action.start_position = Some(300.0);
        action.duration = Some(312.0);

        assert_eq!(action.target_position(1.0).unwrap(), 312.0);
    }
}
