use std::path::Path;

use anyhow::{bail, Context, Result};
use zbus::{
    blocking::{connection::Builder, Connection, Proxy},
    interface,
};

pub const SERVICE_NAME: &str = "io.github.Rejrak.Trackpadd";
pub const OBJECT_PATH: &str = "/io/github/Rejrak/Trackpadd";
pub const INTERFACE_NAME: &str = "io.github.Rejrak.Trackpadd1";

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
}

pub struct IpcServer {
    _connection: Connection,
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

    Ok(IpcServer {
        _connection: connection,
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbus_identifiers_are_stable() {
        assert_eq!(SERVICE_NAME, "io.github.Rejrak.Trackpadd");
        assert_eq!(OBJECT_PATH, "/io/github/Rejrak/Trackpadd");
        assert_eq!(INTERFACE_NAME, "io.github.Rejrak.Trackpadd1");
    }
}
