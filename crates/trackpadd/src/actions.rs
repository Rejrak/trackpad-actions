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
    pub source: String,
    pub title: String,
    pub artist: String,
}

impl ActionValue {
    fn percent(kind: &'static str, percent: u32, max_percent: u32) -> Self {
        Self {
            kind,
            value: f64::from(percent),
            max_value: f64::from(max_percent),
            unit: "percent",
            source: String::new(),
            title: String::new(),
            artist: String::new(),
        }
    }

    fn percent_with_source(
        kind: &'static str,
        percent: u32,
        max_percent: u32,
        source: &str,
    ) -> Self {
        let mut value = Self::percent(kind, percent, max_percent);
        value.source = source.to_string();
        value
    }

    fn media(position: f64, metadata: &MediaMetadata) -> Self {
        Self {
            kind: "media-position",
            value: position,
            max_value: metadata.duration.unwrap_or(0.0),
            unit: "seconds",
            source: metadata.player_name.clone(),
            title: metadata.title.clone(),
            artist: metadata.artist.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct MediaMetadata {
    duration: Option<f64>,
    player_name: String,
    title: String,
    artist: String,
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
    source: String,
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
            source: String::new(),
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

    fn read_source(&self) -> String {
        let output = match Command::new(&self.command)
            .env("LC_ALL", "C")
            .args(["inspect", "@DEFAULT_AUDIO_SINK@"])
            .output()
        {
            Ok(output) if output.status.success() => output,
            _ => return String::new(),
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        select_wpctl_source(&stdout)
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
        let source = self.read_source();
        self.start_value = Some(value);
        self.source = source;
        self.last_sent_percent = Some((value * 100.0).round() as u32);
        Ok(())
    }

    fn update(&mut self, delta: f64) -> Result<Option<ActionValue>> {
        let start = self
            .start_value
            .ok_or_else(|| anyhow!("volume action updated before begin"))?;
        let target = (start + delta).clamp(self.min, self.max);
        let max_percent = (self.max * 100.0).round() as u32;
        Ok(self.write_value(target)?.map(|percent| {
            ActionValue::percent_with_source("volume", percent, max_percent, &self.source)
        }))
    }

    fn finish(&mut self) -> Result<Option<ActionValue>> {
        self.start_value = None;
        self.last_sent_percent = None;
        self.source.clear();
        Ok(None)
    }

    fn cancel(&mut self) -> Result<()> {
        self.start_value = None;
        self.last_sent_percent = None;
        self.source.clear();
        Ok(())
    }
}

pub struct MediaSeekAction {
    command: String,
    seconds_per_full_swipe: f64,
    update_interval: Duration,
    start_position: Option<f64>,
    metadata: MediaMetadata,
    last_delta: f64,
    last_update: Option<Instant>,
    last_sent_position: Option<f64>,
}

impl MediaSeekAction {
    pub fn new(
        command: String,
        seconds_per_full_swipe: f64,
        update_interval_ms: u64,
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
        Ok(Self {
            command,
            seconds_per_full_swipe,
            update_interval: Duration::from_millis(update_interval_ms),
            start_position: None,
            metadata: MediaMetadata::default(),
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

    fn read_metadata(&self) -> MediaMetadata {
        const SEPARATOR: char = '\u{1f}';
        const FORMAT: &str = "{{mpris:length}}\u{1f}{{playerName}}\u{1f}{{title}}\u{1f}{{artist}}";

        let output = match Command::new(&self.command)
            .env("LC_ALL", "C")
            .args(["metadata", "--format", FORMAT])
            .output()
        {
            Ok(output) if output.status.success() => output,
            _ => return MediaMetadata::default(),
        };

        parse_media_metadata(&String::from_utf8_lossy(&output.stdout), SEPARATOR)
    }

    fn target_position(&self, delta: f64) -> Result<f64> {
        let start = self
            .start_position
            .ok_or_else(|| anyhow!("media-seek action updated before begin"))?;
        let offset = delta.clamp(-1.0, 1.0) * self.seconds_per_full_swipe;
        let target = (start + offset).max(0.0);

        Ok(match self.metadata.duration {
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
        self.metadata = MediaMetadata::default();
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
        self.metadata = self.read_metadata();
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

        Ok(changed.then(|| ActionValue::media(target, &self.metadata)))
    }

    fn finish(&mut self) -> Result<Option<ActionValue>> {
        let update = if self.start_position.is_some() {
            let target = self.target_position(self.last_delta)?;
            self.write_position(target)?
                .then(|| ActionValue::media(target, &self.metadata))
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

fn parse_wpctl_property(raw: &str, key: &str) -> Option<String> {
    raw.lines().find_map(|line| {
        let line = line.trim();
        let line = line.strip_prefix('*').unwrap_or(line).trim();
        let (name, value) = line.split_once('=')?;

        if name.trim() != key {
            return None;
        }

        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value);
        let value = normalize_metadata_text(value);

        (!value.is_empty()).then_some(value)
    })
}

fn select_wpctl_source(raw: &str) -> String {
    ["node.nick", "node.description", "node.name"]
        .into_iter()
        .find_map(|key| parse_wpctl_property(raw, key))
        .unwrap_or_default()
}

fn parse_mpris_length(raw: &str) -> Option<f64> {
    let microseconds = raw.trim().parse::<f64>().ok()?;
    if !microseconds.is_finite() || microseconds <= 0.0 {
        return None;
    }

    Some(microseconds / 1_000_000.0)
}

fn normalize_metadata_text(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_media_metadata(raw: &str, separator: char) -> MediaMetadata {
    let cleaned = raw.trim_end_matches(['\r', '\n']);
    let mut fields = cleaned.splitn(4, separator);

    let length = fields.next().unwrap_or_default();
    let player_name = fields.next().unwrap_or_default();
    let title = fields.next().unwrap_or_default();
    let artist = fields.next().unwrap_or_default();

    MediaMetadata {
        duration: parse_mpris_length(length),
        player_name: normalize_metadata_text(player_name),
        title: normalize_metadata_text(title),
        artist: normalize_metadata_text(artist),
    }
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
    fn media_seek_rejects_invalid_backend_parameters() {
        assert!(MediaSeekAction::new("".to_string(), 60.0, 50).is_err());
        assert!(MediaSeekAction::new("playerctl".to_string(), 0.0, 50).is_err());
        assert!(MediaSeekAction::new("playerctl".to_string(), 60.0, 0).is_err());
    }

    #[test]
    fn action_values_expose_explicit_units_maximums_and_metadata() {
        let brightness = ActionValue::percent("brightness", 73, 100);
        assert_eq!(brightness.kind, "brightness");
        assert_eq!(brightness.value, 73.0);
        assert_eq!(brightness.max_value, 100.0);
        assert_eq!(brightness.unit, "percent");
        assert!(brightness.source.is_empty());
        assert!(brightness.title.is_empty());
        assert!(brightness.artist.is_empty());

        let metadata = MediaMetadata {
            duration: Some(312.0),
            player_name: "spotify".to_string(),
            title: "Example Song".to_string(),
            artist: "Example Artist".to_string(),
        };
        let media = ActionValue::media(123.456, &metadata);
        assert_eq!(media.kind, "media-position");
        assert_eq!(media.value, 123.456);
        assert_eq!(media.max_value, 312.0);
        assert_eq!(media.unit, "seconds");
        assert_eq!(media.source, "spotify");
        assert_eq!(media.title, "Example Song");
        assert_eq!(media.artist, "Example Artist");

        let unknown = ActionValue::media(12.0, &MediaMetadata::default());
        assert_eq!(unknown.max_value, 0.0);
        assert!(unknown.source.is_empty());
    }

    #[test]
    fn wpctl_property_parser_extracts_human_facing_names() {
        let inspect = r#"
            id 42, type PipeWire:Interface:Node/3
              * node.name = "alsa_output.pci-0000_00_1f.3.analog-stereo"
              * node.nick = "Speakers"
                node.description = "Built-in Audio Analog Stereo"
        "#;

        assert_eq!(
            parse_wpctl_property(inspect, "node.nick").as_deref(),
            Some("Speakers")
        );
        assert_eq!(
            parse_wpctl_property(inspect, "node.description").as_deref(),
            Some("Built-in Audio Analog Stereo")
        );
        assert_eq!(parse_wpctl_property(inspect, "missing"), None);
    }

    #[test]
    fn wpctl_source_prefers_nick_then_description_then_name() {
        let all_fields = r#"
            node.name = "alsa_output.pci-0000_03_00.6.analog-stereo"
            node.description = "Ryzen HD Audio Controller Analog Stereo"
            node.nick = "ALC256 Analog"
        "#;
        assert_eq!(select_wpctl_source(all_fields), "ALC256 Analog");

        let description_only = r#"
            node.name = "alsa_output.pci-0000_03_00.6.analog-stereo"
            node.description = "Ryzen HD Audio Controller Analog Stereo"
        "#;
        assert_eq!(
            select_wpctl_source(description_only),
            "Ryzen HD Audio Controller Analog Stereo"
        );

        let name_only = r#"
            node.name = "alsa_output.pci-0000_03_00.6.analog-stereo"
        "#;
        assert_eq!(
            select_wpctl_source(name_only),
            "alsa_output.pci-0000_03_00.6.analog-stereo"
        );

        let no_name = r#"
            id 42, type PipeWire:Interface:Node/3
        "#;
        assert_eq!(select_wpctl_source(no_name), "");
    }

    #[test]
    fn sourced_percent_value_keeps_audio_output_context() {
        let value = ActionValue::percent_with_source("volume", 42, 100, "Speakers");

        assert_eq!(value.kind, "volume");
        assert_eq!(value.value, 42.0);
        assert_eq!(value.max_value, 100.0);
        assert_eq!(value.source, "Speakers");
        assert!(value.title.is_empty());
        assert!(value.artist.is_empty());
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
    fn media_metadata_parser_handles_optional_fields() {
        let metadata = parse_media_metadata(
            "312000000\u{1f}spotify\u{1f}Example   Song\u{1f}Example Artist\n",
            '\u{1f}',
        );

        assert_eq!(metadata.duration, Some(312.0));
        assert_eq!(metadata.player_name, "spotify");
        assert_eq!(metadata.title, "Example Song");
        assert_eq!(metadata.artist, "Example Artist");

        let sparse = parse_media_metadata("\u{1f}firefox\u{1f}Video title\u{1f}", '\u{1f}');
        assert_eq!(sparse.duration, None);
        assert_eq!(sparse.player_name, "firefox");
        assert_eq!(sparse.title, "Video title");
        assert!(sparse.artist.is_empty());
    }

    #[test]
    fn media_target_is_clamped_to_known_duration() {
        let mut action = MediaSeekAction::new("playerctl".to_string(), 60.0, 50).unwrap();
        action.start_position = Some(300.0);
        action.metadata.duration = Some(312.0);

        assert_eq!(action.target_position(1.0).unwrap(), 312.0);
    }
}
