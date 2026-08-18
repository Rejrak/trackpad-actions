use std::path::Path;

use anyhow::{bail, Context, Result};
use zbus::{
    blocking::{connection::Builder, Connection, Proxy},
    interface,
};

pub const SERVICE_NAME: &str = "io.github.Rejrak.Trackpadd";
pub const OBJECT_PATH: &str = "/io/github/Rejrak/Trackpadd";
pub const INTERFACE_NAME: &str = "io.github.Rejrak.Trackpadd1";
pub const ACTION_VALUE_CHANGED_SIGNAL: &str = "ActionValueChanged";

struct DaemonInterface {
    version: String,
    device_path: String,
    config_path: String,
    dry_run: bool,
}

#[interface(name = "io.github.Rejrak.Trackpadd1")]
impl DaemonInterface {
    fn ping(&self) -> String {
        "pong".to_string()
    }

    fn version(&self) -> String {
        self.version.clone()
    }

    fn device_path(&self) -> String {
        self.device_path.clone()
    }

    fn config_path(&self) -> String {
        self.config_path.clone()
    }

    fn dry_run(&self) -> bool {
        self.dry_run
    }

    #[zbus(signal)]
    async fn action_value_changed(
        signal_emitter: &zbus::object_server::SignalEmitter<'_>,
        action_id: &str,
        kind: &str,
        value: f64,
        unit: &str,
    ) -> zbus::Result<()>;
}

pub struct IpcServer {
    connection: Connection,
}

impl IpcServer {
    pub fn emit_action_value(
        &self,
        action_id: &str,
        kind: &str,
        value: f64,
        unit: &str,
    ) -> Result<()> {
        self.connection
            .emit_signal(
                None::<&str>,
                OBJECT_PATH,
                INTERFACE_NAME,
                ACTION_VALUE_CHANGED_SIGNAL,
                &(action_id, kind, value, unit),
            )
            .context("failed to emit ActionValueChanged D-Bus signal")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStatus {
    pub version: String,
    pub device_path: String,
    pub config_path: String,
    pub dry_run: bool,
}

pub fn start_server(device: &Path, config: &Path, dry_run: bool) -> Result<IpcServer> {
    let interface = DaemonInterface {
        version: env!("CARGO_PKG_VERSION").to_string(),
        device_path: device.display().to_string(),
        config_path: config.display().to_string(),
        dry_run,
    };

    let connection = Builder::session()
        .context("failed to connect to the D-Bus session bus")?
        .name(SERVICE_NAME)
        .context("failed to request the trackpadd D-Bus service name")?
        .serve_at(OBJECT_PATH, interface)
        .context("failed to register the trackpadd D-Bus object")?
        .build()
        .context("failed to start the trackpadd D-Bus service")?;

    Ok(IpcServer { connection })
}

pub fn query_status() -> Result<DaemonStatus> {
    let connection = Connection::session().context("failed to connect to the D-Bus session bus")?;
    let proxy = Proxy::new(&connection, SERVICE_NAME, OBJECT_PATH, INTERFACE_NAME)
        .context("failed to create the trackpadd D-Bus proxy")?;

    let pong: String = proxy
        .call("Ping", &())
        .context("trackpadd daemon did not answer D-Bus Ping")?;
    if pong != "pong" {
        bail!("unexpected trackpadd D-Bus Ping response: {pong:?}");
    }

    Ok(DaemonStatus {
        version: proxy
            .call("Version", &())
            .context("failed to read daemon version")?,
        device_path: proxy
            .call("DevicePath", &())
            .context("failed to read daemon device path")?,
        config_path: proxy
            .call("ConfigPath", &())
            .context("failed to read daemon config path")?,
        dry_run: proxy
            .call("DryRun", &())
            .context("failed to read daemon dry-run state")?,
    })
}

pub fn watch_action_values() -> Result<()> {
    let connection = Connection::session().context("failed to connect to the D-Bus session bus")?;
    let proxy = Proxy::new(&connection, SERVICE_NAME, OBJECT_PATH, INTERFACE_NAME)
        .context("failed to create the trackpadd D-Bus proxy")?;
    let signals = proxy
        .receive_signal(ACTION_VALUE_CHANGED_SIGNAL)
        .context("failed to subscribe to ActionValueChanged")?;

    println!(
        "Watching {} action values. Press Ctrl+C to stop.",
        SERVICE_NAME
    );

    for message in signals {
        let (action_id, kind, value, unit): (String, String, f64, String) = message
            .body()
            .deserialize()
            .context("failed to decode ActionValueChanged")?;

        println!(
            "ACTION VALUE action={} kind={} value={value:.3} unit={}",
            action_id, kind, unit
        );
    }

    bail!("trackpadd D-Bus action-value stream ended")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbus_identifiers_are_stable() {
        assert_eq!(SERVICE_NAME, "io.github.Rejrak.Trackpadd");
        assert_eq!(OBJECT_PATH, "/io/github/Rejrak/Trackpadd");
        assert_eq!(INTERFACE_NAME, "io.github.Rejrak.Trackpadd1");
        assert_eq!(ACTION_VALUE_CHANGED_SIGNAL, "ActionValueChanged");
    }
}
