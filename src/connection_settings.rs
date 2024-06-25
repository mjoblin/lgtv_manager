use crate::discovery::LgTvDevice;

/// Connection type.
#[derive(Debug, Clone, PartialEq)]
pub enum Connection {
    /// Connection to a TV using a network name or IP address.
    Host(String, ConnectionSettings),
    /// Connection to a TV using a previously-discovered [`LgTvDevice`].
    Device(LgTvDevice, ConnectionSettings),
}

/// Settings to use when connecting to a TV. Can be created with [`ConnectionSettingsBuilder`].
#[derive(Debug, Clone, PartialEq)]
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
    use super::{ConnectionSettings, ConnectionSettingsBuilder};

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
}
