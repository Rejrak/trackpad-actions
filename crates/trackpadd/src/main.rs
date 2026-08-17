mod actions;
mod config;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use actions::{BrightnessAction, ContinuousAction, PrintAction, VolumeAction};
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use config::{
    user_config_path, write_default_user_config, ActionConfig, AppConfig, BindingConfig,
    EdgeConfig, GestureConfig,
};
use trackpad_core::{Edge, EdgeSwipeRecognizer, GestureEngine, GestureEvent, GesturePhase};
use trackpad_linux::{auto_select_touchpad, list_devices, TouchpadReader};

#[derive(Debug, Parser)]
#[command(name = "trackpadd")]
#[command(about = "Configurable Linux trackpad gesture daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// List multitouch input devices visible to the current user.
    Devices,

    /// Create ~/.config/trackpadd/config.toml (or XDG_CONFIG_HOME equivalent).
    InitConfig {
        /// Overwrite an existing config file.
        #[arg(long)]
        force: bool,
    },

    /// Print normalized touch contacts and edge gesture events.
    Monitor {
        /// Explicit evdev path. If omitted, auto-select when exactly one compatible touchpad exists.
        #[arg(long)]
        device: Option<PathBuf>,
    },

    /// Run configured gesture -> action mappings.
    Run {
        /// Explicit evdev path. If omitted, auto-select when exactly one compatible touchpad exists.
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
        Commands::Devices => devices(),
        Commands::InitConfig { force } => init_config(force),
        Commands::Monitor { device } => monitor(resolve_device(device)?),
        Commands::Run {
            device,
            config,
            dry_run,
        } => run(resolve_device(device)?, resolve_config(config)?, dry_run),
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

fn devices() -> Result<()> {
    let devices = list_devices();

    if devices.is_empty() {
        println!("No multitouch candidates are readable by the current user.");
        println!(
            "Install the trackpadd udev/uaccess rule or grant a temporary ACL for development."
        );
        return Ok(());
    }

    for (index, device) in devices.iter().enumerate() {
        println!("[{index}] {}", device.path.display());
        println!("    name:       {}", device.name);
        println!("    vendor:     {:04x}", device.vendor);
        println!("    product:    {:04x}", device.product);
        println!("    pointer:    {}", device.pointer);
        println!("    direct:     {}", device.direct);
        println!("    semi-mt:    {}", device.semi_mt);
        println!("    slots:      {}", device.slots);
        println!("    x range:    {}..{}", device.x_min, device.x_max);
        println!("    y range:    {}..{}", device.y_min, device.y_max);
        println!("    compatible: {}", device.compatible);
        println!();
    }

    Ok(())
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

fn run(device: PathBuf, config_path: PathBuf, dry_run: bool) -> Result<()> {
    let config = AppConfig::load(&config_path)?;
    let AppConfig {
        gestures,
        actions,
        bindings,
    } = config;

    let mut gesture_ids = HashSet::new();
    let mut engine = GestureEngine::new();

    for gesture in gestures {
        match gesture {
            GestureConfig::EdgeSwipe {
                id,
                edge,
                width,
                cancel_margin,
            } => {
                if !gesture_ids.insert(id.clone()) {
                    bail!("duplicate gesture id: {id}");
                }

                let edge = match edge {
                    EdgeConfig::Left => Edge::Left,
                    EdgeConfig::Right => Edge::Right,
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
        };

        if action_map.insert(id.clone(), implementation).is_some() {
            bail!("duplicate action id: {id}");
        }
    }

    let mut bindings_by_gesture: HashMap<String, Vec<BindingConfig>> = HashMap::new();
    for binding in bindings {
        if !gesture_ids.contains(&binding.gesture) {
            bail!("binding references unknown gesture: {}", binding.gesture);
        }
        if !action_map.contains_key(&binding.action) {
            bail!("binding references unknown action: {}", binding.action);
        }

        bindings_by_gesture
            .entry(binding.gesture.clone())
            .or_default()
            .push(binding);
    }

    let mut reader = TouchpadReader::open(&device)
        .with_context(|| format!("failed to initialize touchpad {}", device.display()))?;
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

                if let Err(error) = dispatch_action(action.as_mut(), binding, &event) {
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

fn dispatch_action(
    action: &mut dyn ContinuousAction,
    binding: &BindingConfig,
    event: &GestureEvent,
) -> Result<()> {
    match event.phase {
        GesturePhase::Started => action.begin(),
        GesturePhase::Updated => {
            let sign = if binding.invert { -1.0 } else { 1.0 };
            let delta = event.delta * binding.sensitivity * sign;
            if let Some(value) = action.update(delta)? {
                println!(
                    "VALUE action={} value={:.0}%",
                    binding.action,
                    value * 100.0
                );
            }
            Ok(())
        }
        GesturePhase::Ended => action.finish(),
        GesturePhase::Cancelled => action.cancel(),
    }
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
