use std::fmt;

use crate::discovery::LgTvDevice;

/// Connection type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Connection {
    /// Connection to a TV using a network name or IP address.
    Host(String, ConnectionSettings),
    /// Connection to a TV using a previously-discovered [`LgTvDevice`].
    Device(LgTvDevice, ConnectionSettings),
}

impl fmt::Display for Connection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Connection::Host(host, settings) => {
                write!(f, "Connection to host '{}', {:?}", host, settings)
            }
            Connection::Device(device, settings) => write!(
                f,
                "Connection to UPnP device '{}', {:?}",
                device.friendly_name, settings
            ),
        }
    }
}

/// Settings to use when connecting to a TV. Can be created with [`ConnectionSettingsBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionSettings {
    pub is_tls: bool,
    pub force_pairing: bool,
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        ConnectionSettingsBuilder::new().build()
    }
}

/// Build a [`ConnectionSettings`] instance.
///
/// Examples:
/// ```
/// use lgtv_manager::ConnectionSettingsBuilder;
///
/// // Default connection settings
/// ConnectionSettingsBuilder::default();
///
/// // Connection settings with overrides
/// ConnectionSettingsBuilder::new()
///     .with_no_tls()
///     .with_forced_pairing()
///     .build();
/// ```
pub struct ConnectionSettingsBuilder {
    is_tls: bool,
    force_pairing: bool,
}

impl Default for ConnectionSettingsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionSettingsBuilder {
    pub fn new() -> Self {
        Self {
            is_tls: true,
            force_pairing: false,
            // auto_reconnect: false,
        }
    }

    pub fn with_no_tls(mut self) -> Self {
        self.is_tls = false;
        self
    }

    pub fn with_forced_pairing(mut self) -> Self {
        self.force_pairing = true;
        self
    }

    pub fn build(&mut self) -> ConnectionSettings {
        ConnectionSettings {
            is_tls: self.is_tls,
            force_pairing: self.force_pairing,
            // auto_reconnect: self.auto_reconnect,
        }
    }
}

// ================================================================================================
// Tests

#[cfg(test)]
mod tests {
    use super::{Connection, ConnectionSettings, ConnectionSettingsBuilder};
    use crate::LgTvDevice;

    #[test]
    fn connection_settings_default() {
        assert_eq!(
            ConnectionSettings::default(),
            ConnectionSettings {
                is_tls: true,
                force_pairing: false,
            }
        );
    }

    #[test]
    fn connection_settings_builder() {
        assert_eq!(
            ConnectionSettingsBuilder::new()
                .with_no_tls()
                .with_forced_pairing()
                .build(),
            ConnectionSettings {
                is_tls: false,
                force_pairing: true,
            }
        );
    }

    #[test]
    fn connection_display_host() {
        assert_eq!(
            Connection::Host("127.0.0.1".to_string(), ConnectionSettings::default()).to_string(),
            "Connection to host '127.0.0.1', ConnectionSettings { is_tls: true, force_pairing: false }"
        );
    }

    #[test]
    fn connection_display_device() {
        assert_eq!(
            Connection::Device(LgTvDevice {
                friendly_name: "LG WebOS TV".to_string(),
                model: "model".to_string(),
                model_number: None,
                serial_number: None,
                url: "http://38.0.101.76:3000".to_string(),
                udn: "uuid:00000000-0000-0000-0000-000000000000".to_string(),
            }, ConnectionSettings::default()).to_string(),
            "Connection to UPnP device 'LG WebOS TV', ConnectionSettings { is_tls: true, force_pairing: false }"
        );
    }
}
