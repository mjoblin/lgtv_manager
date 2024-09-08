use log::debug;

use crate::helpers::url_ip_addr;
use crate::{ManagerOutputMessage, ManagerStatus, ReconnectFlowStatus};

use super::LgTvManager;

// ------------------------------------------------------------------------------------------------
// Various LgTvManager state setters.
// ------------------------------------------------------------------------------------------------

impl LgTvManager {
    /// Update the current manager status and announce the change to the caller.
    pub(crate) async fn set_manager_status(&mut self, status: ManagerStatus) {
        if status == self.manager_status {
            return;
        }

        self.manager_status = status;
        self.emit_manager_status().await;

        // When entering a Communicating state from a non-Communicating state, we assume that we've
        // established a new connection so we announce this as a new LastSeenTv.
        if let ManagerStatus::Communicating(_) = self.manager_status {
            self.emit_last_seen_tv().await;
        }
    }

    /// Set the reconnect flow status and notify the caller.
    ///
    /// Note: The reconnect flow status exists separately from the manager status. While the
    ///     reconnect flow is active, the manager's status will cycle through its normal connect
    ///     phases (Connecting, Communicating, etc).
    pub(crate) async fn set_reconnect_flow_status(
        &mut self,
        reconnect_status: ReconnectFlowStatus,
    ) {
        if self.reconnect_flow_status != ReconnectFlowStatus::Active {
            self.reconnect_attempts = 0;
        }

        self.reconnect_flow_status = reconnect_status;
        debug!(
            "Reconnect flow status set: {:?}",
            &self.reconnect_flow_status
        );

        let _ = self
            .send_out(ManagerOutputMessage::ReconnectFlowStatus(
                self.reconnect_flow_status.clone(),
            ))
            .await;
    }

    /// Determine the TV host_ip (an IpAddr) from the current WebSocket URL (a String).
    pub(crate) async fn set_host_ip_from_ws_url(&mut self) {
        if let Some(ws_url) = &self.ws_url {
            if let Some(ip_addr) = url_ip_addr(ws_url) {
                let mut host_ip = self.tv_host_ip.lock().await;
                *host_ip = Some(ip_addr);

                // Inform the TV network checker of the new IP.
                self.tv_host_ip_notifier.notify_one();
            }
        }
    }
}
