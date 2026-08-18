mod actions;
mod config;
mod ipc;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use actions::{
    ActionValue, BrightnessAction, CommandAction, CommandDirection, CommandTrigger,
    ContinuousAction, MediaSeekAction, PrintAction, VolumeAction,
};
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use config::{
    user_config_path, write_default_user_config, ActionConfig, AppConfig, BindingConfig,
    CommandDirectionConfig, CommandTriggerConfig, DeviceConfig, EdgeConfig, GestureConfig,
};
use trackpad_core::{Edge, EdgeSwipeRecognizer, GestureEngine, GestureEvent, GesturePhase};
use trackpad_linux::{
    auto_select_touchpad, compatible_devices, diagnose_devices, DeviceDiagnostic, DeviceInfo,
    TouchpadReader,
};

#[derive(Debug, Parser)]
#[command(name = "trackpadd")]
#[command(version)]
#[command(about = "Configurable Linux trackpad gesture daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Diagnose Linux input devices and touchpad compatibility.
    Devices {
        /// Show every /dev/input/event* node, including non-touchpad and unreadable devices.
        #[arg(long)]
        all: bool,

        /// Optional config path used to show which devices match [device].
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Create ~/.config/trackpadd/config.toml (or XDG_CONFIG_HOME equivalent).
    InitConfig {
        /// Overwrite an existing config file.
        #[arg(long)]
        force: bool,
    },

    /// Parse and validate the configuration without opening a touchpad.
    CheckConfig {
        /// Config path. Defaults to $XDG_CONFIG_HOME/trackpadd/config.toml or ~/.config/trackpadd/config.toml.
        #[arg(long)]
        config: Option<PathBuf>,
    },

    /// Query the running daemon over D-Bus without opening the touchpad.
    Status,

    /// Watch action value changes emitted by the running daemon over D-Bus.
    Watch,

    /// Print normalized touch contacts and edge gesture events.
    Monitor {
        /// Explicit evdev path. If omitted, auto-select when exactly one compatible touchpad exists.
        #[arg(long)]
        device: Option<PathBuf>,
    },

    /// Run configured gesture -> action mappings.
    Run {
        /// Explicit evdev path. Overrides the configured device selector when present.
        #[arg(long)]
        device: Option<PathBuf>,

        /// Config path. Defaults to $XDG_CONFIG_HOME/trackpadd/config.toml or ~/.config/trackpadd/config.toml.
        #[arg(long)]
        config: Option<PathBuf>,

        /// Recognize gestures but never execute external actions.
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Devices { all, config } => devices(all, config),
        Commands::InitConfig { force } => init_config(force),
        Commands::CheckConfig { config } => check_config(resolve_config(config)?),
        Commands::Status => status(),
        Commands::Watch => ipc::watch_action_values(),
        Commands::Monitor { device } => monitor(resolve_device(device)?),
        Commands::Run {
            device,
            config,
            dry_run,
        } => run(device, resolve_config(config)?, dry_run),
    }
}

fn resolve_device(device: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(device) = device {
        return Ok(device);
    }

    let selected = auto_select_touchpad()?;
    println!(
        "Auto-selected touchpad: {} ({})",
        selected.path.display(),
        selected.name
    );
    Ok(selected.path)
}

fn resolve_run_device(
    explicit_device: Option<PathBuf>,
    configured_device: Option<&DeviceConfig>,
) -> Result<PathBuf> {
    if let Some(device) = explicit_device {
        println!("Using explicit touchpad: {}", device.display());
        return Ok(device);
    }

    if let Some(selector) = configured_device {
        let selected = select_configured_touchpad(compatible_devices(), selector)?;
        println!(
            "Selected configured touchpad: {} ({})",
            selected.path.display(),
            selected.name
        );
        return Ok(selected.path);
    }

    resolve_device(None)
}

fn select_configured_touchpad(
    devices: Vec<DeviceInfo>,
    selector: &DeviceConfig,
) -> Result<DeviceInfo> {
    let matches = devices
        .into_iter()
        .filter(|device| selector.matches(&device.name, device.vendor, device.product))
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [] => bail!(
            "no compatible readable touchpad matches configured selector ({}); \
             run `trackpadd devices` and verify [device]",
            selector.description()
        ),
        [device] => Ok(device.clone()),
        many => {
            let candidates = many
                .iter()
                .map(|device| {
                    format!(
                        "{} ({}, vendor=0x{:04x}, product=0x{:04x})",
                        device.path.display(),
                        device.name,
                        device.vendor,
                        device.product
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");

            bail!(
                "configured device selector ({}) matches multiple compatible touchpads \
                 ({candidates}); make [device] more specific or pass --device explicitly",
                selector.description()
            )
        }
    }
}

fn resolve_config(config: Option<PathBuf>) -> Result<PathBuf> {
    let path = match config {
        Some(path) => path,
        None => user_config_path()?,
    };

    if !path.exists() {
        bail!(
            "config {} does not exist; run `trackpadd init-config` first or pass --config",
            path.display()
        );
    }

    Ok(path)
}

fn init_config(force: bool) -> Result<()> {
    let path = write_default_user_config(force)?;
    println!("Wrote config: {}", path.display());
    Ok(())
}

fn check_config(config_path: PathBuf) -> Result<()> {
    let config = AppConfig::load(&config_path)?;

    println!("Config OK: {}", config_path.display());
    if let Some(device) = &config.device {
        println!("Device selector: {}", device.description());
    } else {
        println!("Device selector: automatic");
    }
    println!("Gestures: {}", config.gestures.len());
    println!("Actions: {}", config.actions.len());
    println!("Bindings: {}", config.bindings.len());

    Ok(())
}

fn status() -> Result<()> {
    let status = ipc::query_status()?;

    println!("trackpadd {}", status.version);
    println!("D-Bus service: {}", ipc::SERVICE_NAME);
    println!("Device: {}", status.device_path);
    println!("Config: {}", status.config_path);
    println!("Dry run: {}", status.dry_run);

    Ok(())
}

fn devices(all: bool, config_path: Option<PathBuf>) -> Result<()> {
    let selector = match config_path {
        Some(path) => {
            let path = resolve_config(Some(path))?;
            AppConfig::load(&path)?.device
        }
        None => None,
    };

    let diagnostics = diagnose_devices()?;

    if diagnostics.is_empty() {
        println!("No /dev/input/event* devices were found.");
        return Ok(());
    }

    let visible = diagnostics
        .iter()
        .filter(|device| all || device.is_touch_candidate())
        .collect::<Vec<_>>();

    if visible.is_empty() {
        println!("No touchpad candidates were found.");
        println!("Use `trackpadd devices --all` to inspect every input event node.");
        println!();
    }

    for (index, device) in visible.iter().enumerate() {
        println!("[{index}] {}", device.path.display());

        let status = if !device.readable {
            "unreadable"
        } else if device.compatible {
            "compatible"
        } else {
            "rejected"
        };

        println!("    status:     {status}");
        println!("    readable:   {}", device.readable);

        if let Some(name) = &device.name {
            println!("    name:       {name}");
        }
        if let Some(vendor) = device.vendor {
            println!("    vendor:     {vendor:04x}");
        }
        if let Some(product) = device.product {
            println!("    product:    {product:04x}");
        }
        if let Some(pointer) = device.pointer {
            println!("    pointer:    {pointer}");
        }
        if let Some(direct) = device.direct {
            println!("    direct:     {direct}");
        }
        if let Some(semi_mt) = device.semi_mt {
            println!("    semi-mt:    {semi_mt}");
        }

        println!("    mt axes:    {}/4", device.required_mt_axes);

        if let Some(slots) = device.slots {
            println!("    slots:      {slots}");
        }
        if let Some((min, max)) = device.x_range {
            println!("    x range:    {min}..{max}");
        }
        if let Some((min, max)) = device.y_range {
            println!("    y range:    {min}..{max}");
        }

        println!("    compatible: {}", device.compatible);

        if device.compatible {
            if let (Some(vendor), Some(product)) = (device.vendor, device.product) {
                println!("    selector:   vendor=0x{vendor:04x} product=0x{product:04x}");
            }
        }

        if let Some(selector) = &selector {
            println!(
                "    config-match: {}",
                diagnostic_matches_selector(device, selector)
            );
        }

        if !device.issues.is_empty() {
            println!("    issues:");
            for issue in &device.issues {
                println!("      - {issue}");
            }
        }

        println!();
    }

    let compatible = diagnostics
        .iter()
        .filter(|device| device.compatible)
        .collect::<Vec<_>>();
    let rejected_candidates = diagnostics
        .iter()
        .filter(|device| device.readable && device.is_touch_candidate() && !device.compatible)
        .count();
    let unreadable = diagnostics.iter().filter(|device| !device.readable).count();

    println!(
        "Summary: {} compatible, {} rejected touch candidate(s), {} unreadable event node(s)",
        compatible.len(),
        rejected_candidates,
        unreadable
    );

    if let Some(selector) = &selector {
        let matches = compatible
            .iter()
            .filter(|device| diagnostic_matches_selector(device, selector))
            .collect::<Vec<_>>();

        println!("Configured selector: {}", selector.description());

        match matches.as_slice() {
            [] => println!("Configured selection: no compatible match"),
            [device] => println!("Configured selection: {}", device.path.display()),
            many => println!(
                "Configured selection: ambiguous ({} compatible matches)",
                many.len()
            ),
        }
    } else {
        match compatible.as_slice() {
            [] => println!("Automatic selection: unavailable"),
            [device] => println!("Automatic selection: {}", device.path.display()),
            many => println!(
                "Automatic selection: ambiguous ({} compatible devices)",
                many.len()
            ),
        }
    }

    if !all {
        let hidden = diagnostics.len().saturating_sub(visible.len());
        if hidden > 0 {
            println!("Hidden event nodes: {hidden}. Use `trackpadd devices --all` for the complete list.");
        }
    }

    Ok(())
}

fn diagnostic_matches_selector(device: &DeviceDiagnostic, selector: &DeviceConfig) -> bool {
    match (device.name.as_deref(), device.vendor, device.product) {
        (Some(name), Some(vendor), Some(product)) => selector.matches(name, vendor, product),
        _ => false,
    }
}

fn monitor(device: PathBuf) -> Result<()> {
    let mut reader = TouchpadReader::open(&device)?;
    println!("Reading {} ({})", device.display(), reader.name());
    println!("Press Ctrl+C to stop.\n");

    let mut engine = GestureEngine::new();
    engine.add(EdgeSwipeRecognizer::new(
        "left-edge",
        Edge::Left,
        0.06,
        0.04,
    ));
    engine.add(EdgeSwipeRecognizer::new(
        "right-edge",
        Edge::Right,
        0.06,
        0.04,
    ));
    engine.add(EdgeSwipeRecognizer::new("top-edge", Edge::Top, 0.06, 0.10));

    loop {
        let frame = reader.next_frame()?;

        if !frame.contacts.is_empty() {
            let contacts = frame
                .contacts
                .iter()
                .map(|c| format!("id={} x={:.3} y={:.3}", c.id, c.x, c.y))
                .collect::<Vec<_>>()
                .join(" | ");
            println!("TOUCH {contacts}");
        }

        for event in engine.process(&frame) {
            print_gesture(&event);
        }
    }
}

fn run(device: Option<PathBuf>, config_path: PathBuf, dry_run: bool) -> Result<()> {
    let config = AppConfig::load(&config_path)?;
    let device = resolve_run_device(device, config.device.as_ref())?;

    let AppConfig {
        device: _,
        gestures,
        actions,
        bindings,
    } = config;

    let mut engine = GestureEngine::new();

    for gesture in gestures {
        match gesture {
            GestureConfig::EdgeSwipe {
                id,
                edge,
                width,
                cancel_margin,
            } => {
                let edge = match edge {
                    EdgeConfig::Left => Edge::Left,
                    EdgeConfig::Right => Edge::Right,
                    EdgeConfig::Top => Edge::Top,
                };

                engine.add(EdgeSwipeRecognizer::new(id, edge, width, cancel_margin));
            }
        }
    }

    let mut action_map: HashMap<String, Box<dyn ContinuousAction>> = HashMap::new();
    for action in actions {
        let (id, implementation): (String, Box<dyn ContinuousAction>) = match action {
            ActionConfig::Brightness {
                id,
                command,
                min,
                max,
            } => {
                let implementation = BrightnessAction::new(command, min, max)?;
                (id, Box::new(implementation))
            }
            ActionConfig::Volume {
                id,
                command,
                min,
                max,
            } => {
                let implementation = VolumeAction::new(command, min, max)?;
                (id, Box::new(implementation))
            }
            ActionConfig::Print { id, label } => {
                let label = label.unwrap_or_else(|| id.clone());
                (id, Box::new(PrintAction::new(label)))
            }
            ActionConfig::Command {
                id,
                command,
                args,
                trigger,
                direction,
                threshold,
            } => {
                let trigger = match trigger {
                    CommandTriggerConfig::Start => CommandTrigger::Start,
                    CommandTriggerConfig::End => CommandTrigger::End,
                };
                let direction = match direction {
                    CommandDirectionConfig::Any => CommandDirection::Any,
                    CommandDirectionConfig::Up => CommandDirection::Up,
                    CommandDirectionConfig::Down => CommandDirection::Down,
                    CommandDirectionConfig::Left => CommandDirection::Left,
                    CommandDirectionConfig::Right => CommandDirection::Right,
                };
                let implementation =
                    CommandAction::new(id.clone(), command, args, trigger, direction, threshold)?;
                (id, Box::new(implementation))
            }
            ActionConfig::MediaSeek {
                id,
                command,
                seconds_per_full_swipe,
                update_interval_ms,
                deadzone,
                curve,
            } => {
                let implementation = MediaSeekAction::new(
                    command,
                    seconds_per_full_swipe,
                    update_interval_ms,
                    deadzone,
                    curve,
                )?;
                (id, Box::new(implementation))
            }
        };

        action_map.insert(id, implementation);
    }

    let mut bindings_by_gesture: HashMap<String, Vec<BindingConfig>> = HashMap::new();
    for binding in bindings {
        bindings_by_gesture
            .entry(binding.gesture.clone())
            .or_default()
            .push(binding);
    }

    let mut reader = TouchpadReader::open(&device)
        .with_context(|| format!("failed to initialize touchpad {}", device.display()))?;

    let _ipc_server = match ipc::start_server(&device, &config_path, dry_run) {
        Ok(server) => {
            println!("D-Bus: {}", ipc::SERVICE_NAME);
            Some(server)
        }
        Err(error) => {
            eprintln!("IPC WARNING: D-Bus status service unavailable: {error:#}");
            None
        }
    };

    println!("Reading {} ({})", device.display(), reader.name());
    println!("Config: {}", config_path.display());
    println!("Dry run: {dry_run}");
    println!("Press Ctrl+C to stop.\n");

    // If an action backend fails during one gesture, suppress repeated errors until
    // that gesture ends. The next gesture gets a fresh attempt.
    let mut failed_bindings: HashSet<(String, String)> = HashSet::new();

    loop {
        let frame = reader.next_frame()?;
        let events = engine.process(&frame);

        for event in events {
            print_gesture(&event);

            let Some(bindings) = bindings_by_gesture.get(&event.gesture_id) else {
                continue;
            };

            for binding in bindings {
                if dry_run {
                    if event.phase == GesturePhase::Updated {
                        let sign = if binding.invert { -1.0 } else { 1.0 };
                        let delta = event.delta * binding.sensitivity * sign;
                        println!(
                            "DRY  gesture={} action={} delta={delta:+.3}",
                            binding.gesture, binding.action
                        );
                    }
                    continue;
                }

                let key = (binding.gesture.clone(), binding.action.clone());

                if event.phase == GesturePhase::Started {
                    failed_bindings.remove(&key);
                } else if failed_bindings.contains(&key) {
                    if matches!(event.phase, GesturePhase::Ended | GesturePhase::Cancelled) {
                        failed_bindings.remove(&key);
                    }
                    continue;
                }

                let Some(action) = action_map.get_mut(&binding.action) else {
                    continue;
                };

                match dispatch_action(action.as_mut(), binding, &event) {
                    Ok(Some(value)) => {
                        print_action_value(binding, &value);

                        if let Some(ipc_server) = &_ipc_server {
                            if let Err(error) = ipc_server.emit_action_value(
                                &binding.action,
                                value.kind,
                                value.value,
                                value.max_value,
                                value.unit,
                                (&value.source, &value.title, &value.artist),
                            ) {
                                eprintln!(
                                    "IPC WARNING: failed to emit action value for '{}': {error:#}",
                                    binding.action
                                );
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!(
                            "ACTION ERROR phase={:?} action='{}' gesture='{}': {error:#}",
                            event.phase, binding.action, binding.gesture
                        );
                        let _ = action.cancel();

                        if !matches!(event.phase, GesturePhase::Ended | GesturePhase::Cancelled) {
                            failed_bindings.insert(key);
                        }
                    }
                }
            }
        }
    }
}

fn dispatch_action(
    action: &mut dyn ContinuousAction,
    binding: &BindingConfig,
    event: &GestureEvent,
) -> Result<Option<ActionValue>> {
    match event.phase {
        GesturePhase::Started => {
            action.begin()?;
            Ok(None)
        }
        GesturePhase::Updated => {
            let sign = if binding.invert { -1.0 } else { 1.0 };
            let delta = event.delta * binding.sensitivity * sign;
            action.update(delta)
        }
        GesturePhase::Ended => action.finish(),
        GesturePhase::Cancelled => {
            action.cancel()?;
            Ok(None)
        }
    }
}

fn print_action_value(binding: &BindingConfig, value: &ActionValue) {
    println!(
        "VALUE action={} kind={} value={:.3} max={:.3} unit={} source={:?} title={:?} artist={:?}",
        binding.action,
        value.kind,
        value.value,
        value.max_value,
        value.unit,
        value.source,
        value.title,
        value.artist
    );
}

fn print_gesture(event: &GestureEvent) {
    match event.phase {
        GesturePhase::Started => println!("GESTURE {} started", event.gesture_id),
        GesturePhase::Updated => println!(
            "GESTURE {} updated delta={:+.3}",
            event.gesture_id, event.delta
        ),
        GesturePhase::Ended => println!("GESTURE {} ended", event.gesture_id),
        GesturePhase::Cancelled => println!("GESTURE {} cancelled", event.gesture_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(path: &str, name: &str, vendor: u16, product: u16) -> DeviceInfo {
        DeviceInfo {
            path: PathBuf::from(path),
            name: name.to_string(),
            vendor,
            product,
            pointer: true,
            direct: false,
            semi_mt: false,
            slots: 5,
            x_min: 0,
            x_max: 1000,
            y_min: 0,
            y_max: 700,
            compatible: true,
        }
    }

    #[test]
    fn configured_selector_selects_unique_device() {
        let selector = DeviceConfig {
            name: None,
            vendor: Some(0x04f3),
            product: Some(0x3140),
        };

        let selected = select_configured_touchpad(
            vec![
                device("/dev/input/event4", "Touchpad A", 0x04f3, 0x3140),
                device("/dev/input/event5", "Touchpad B", 0x1234, 0x5678),
            ],
            &selector,
        )
        .unwrap();

        assert_eq!(selected.path, PathBuf::from("/dev/input/event4"));
    }

    #[test]
    fn configured_selector_rejects_ambiguous_match() {
        let selector = DeviceConfig {
            name: None,
            vendor: Some(0x04f3),
            product: Some(0x3140),
        };

        let error = select_configured_touchpad(
            vec![
                device("/dev/input/event4", "Touchpad A", 0x04f3, 0x3140),
                device("/dev/input/event5", "Touchpad B", 0x04f3, 0x3140),
            ],
            &selector,
        )
        .unwrap_err();

        assert!(error.to_string().contains("matches multiple"));
    }
}
