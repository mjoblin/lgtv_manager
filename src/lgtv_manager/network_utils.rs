use std::sync::atomic::Ordering;

use log::{info, warn};
use wol::send_wol;

use crate::discovery::discover_lgtv_devices;
use crate::helpers::url_ip_addr;
use crate::lgtv_manager::persisted_state::read_persisted_state;
use crate::{Connection, ManagerError, ManagerOutputMessage, ReconnectFlowStatus};

use super::LgTvManager;

// ------------------------------------------------------------------------------------------------
// Network-related helper utility functions for discovery, Wake-on-LAN, reconnect flow, etc.
// ------------------------------------------------------------------------------------------------

impl LgTvManager {
    /// Discover LG TVs on the local network using UPnP discovery.
    ///
    /// Discovered TVs are sent back to the caller as a
    /// `ManagerOutputMessage::DiscoveredDevices` message (containing a vector of `TvDevice`
    /// instances).
    pub(crate) fn discover(&mut self) {
        let output_tx = self.output_tx.clone();

        tokio::spawn(async move {
            let _ = LgTvManager::send_out_with_sender(
                &output_tx,
                ManagerOutputMessage::IsDiscovering(true),
            )
            .await;

            match discover_lgtv_devices().await {
                Ok(discovered_lgtv_devices) => {
                    let _ = LgTvManager::send_out_with_sender(
                        &output_tx,
                        ManagerOutputMessage::DiscoveredDevices(discovered_lgtv_devices),
                    )
                    .await;
                }
                Err(e) => {
                    let _ = LgTvManager::send_out_with_sender(
                        &output_tx,
                        ManagerOutputMessage::Error(ManagerError::Discovery(format!(
                            "Discovery failed: {e}"
                        ))),
                    )
                    .await;
                }
            }

            let _ = LgTvManager::send_out_with_sender(
                &output_tx,
                ManagerOutputMessage::IsDiscovering(false),
            )
            .await;
        });
    }

    /// Send a Wake-on-LAN request to the TV persisted to disk.
    pub(crate) async fn wake_last_seen_tv(&self) {
        let mut result_msg: Option<String> = None;

        match read_persisted_state(self.data_dir.clone()) {
            Ok(persisted_state) => {
                if let (Some(ws_url), Some(mac_addr)) =
                    (persisted_state.ws_url, persisted_state.mac_addr)
                {
                    if let (Some(ip), Ok(mac)) = (url_ip_addr(&ws_url), mac_addr.parse()) {
                        info!("Sending Wake-on-LAN to {}, {}", &ip, &mac);

                        if let Err(e) = send_wol(wol::MacAddr::from(mac), Some(ip), None) {
                            result_msg = Some(format!("Send error: {}", e))
                        }
                    } else {
                        result_msg = Some(format!(
                            "Cannot determine host IP and MAC from URL: {} MAC: {}",
                            &ws_url, &mac_addr
                        ));
                    }
                } else {
                    result_msg = Some("Missing WebSocket URL or MAC address".into());
                }
            }
            Err(e) => {
                result_msg = Some(format!("Cannot read persisted data: {}", e));
            }
        }

        if let Some(msg) = result_msg {
            let msg_out = format!("Cannot send Wake-on-LAN: {}", msg);
            warn!("{}", &msg_out);

            let _ = self
                .send_out(ManagerOutputMessage::Error(ManagerError::Action(
                    msg_out.into(),
                )))
                .await;
        }
    }

    /// Are initial connect retries enabled for the current `Connection`.
    pub(crate) fn is_initial_connect_retries_enabled(&self) -> bool {
        if let Some(connection) = &self.connection_details {
            return match &connection {
                Connection::Host(_, settings) => settings.initial_connect_retries,
                Connection::Device(_, settings) => settings.initial_connect_retries,
            };
        }

        false
    }

    /// Are auto-reconnects enabled for the current `Connection`.
    pub(crate) fn is_auto_reconnect_enabled(&self) -> bool {
        if let Some(connection) = &self.connection_details {
            return match &connection {
                Connection::Host(_, settings) => settings.auto_reconnect,
                Connection::Device(_, settings) => settings.auto_reconnect,
            };
        }

        false
    }

    /// Prepare the manager for a new reconnect cycle if reconnects are enabled.
    ///
    /// The reconnect flow won't begin until the state machine reaches the Disconnected state.
    ///
    /// A reconnect flow will not be initiated if the caller has requested a manual disconnect or
    /// if an existing reconnect flow has been cancelled.
    pub(crate) async fn optionally_prepare_for_reconnect(&mut self) {
        if self.is_manual_disconnect_requested
            || self.reconnect_flow_status == ReconnectFlowStatus::Cancelled
        {
            return;
        }

        self.set_reconnect_flow_status(match self.is_auto_reconnect_enabled() {
            true => {
                if self.is_tv_on_network.load(Ordering::SeqCst) {
                    ReconnectFlowStatus::Active
                } else if !self.is_tv_on_network.load(Ordering::SeqCst)
                    && !self.tv_on_network_checker.is_operational
                {
                    // TV network checker isn't running, so we have to fall back on Active to
                    // ensure reconnects are still attempted
                    ReconnectFlowStatus::Active
                } else {
                    ReconnectFlowStatus::WaitingForTvOnNetwork
                }
            }
            false => ReconnectFlowStatus::Inactive,
        })
        .await;
    }

    /// Should a connect failure be retried.
    pub(crate) fn should_retry_connect_failure(&self) -> bool {
        if self.is_initial_connect_retries_enabled() && self.session_connection_count == 0 {
            return true;
        } else if self.is_auto_reconnect_enabled() && self.session_connection_count > 0 {
            return true;
        }

        false
    }
}
