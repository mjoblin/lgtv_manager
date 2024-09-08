use std::sync::atomic::Ordering;

use crate::{LastSeenTv, ManagerOutputMessage};

use super::LgTvManager;

// ------------------------------------------------------------------------------------------------
// Emit various ManagerOutputMessages to the caller.
// ------------------------------------------------------------------------------------------------

impl LgTvManager {
    /// Emit all current TV details to the caller.
    pub(crate) async fn emit_all_tv_details(&mut self) {
        self.emit_last_seen_tv().await;
        self.emit_tv_info().await;
        self.emit_tv_inputs().await;
        self.emit_tv_software_info().await;
        self.emit_tv_state().await;
        self.emit_is_wake_tv_available().await;
    }

    /// Send the current `ManagerStatus` to the caller.
    pub(crate) async fn emit_manager_status(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::Status(self.manager_status.clone()))
            .await;
    }

    /// Send `LastSeenTv` details to the caller.
    pub(crate) async fn emit_last_seen_tv(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::LastSeenTv(LastSeenTv {
                websocket_url: self.ws_url.clone(),
                client_key: self.client_key.clone(),
                mac_addr: match self.mac_addr {
                    Some(mac_addr) => Some(mac_addr),
                    None => None,
                },
            }))
            .await;
    }

    /// Send the current `TvInfo` to the caller.
    pub(crate) async fn emit_tv_info(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::TvInfo(self.tv_info.clone()))
            .await;
    }

    /// Send the current `TvInput` list to the caller.
    pub(crate) async fn emit_tv_inputs(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::TvInputs(self.tv_inputs.clone()))
            .await;
    }

    /// Send the current `TvSoftwareInfo` to the caller.
    pub(crate) async fn emit_tv_software_info(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::TvSoftwareInfo(Box::new(
                self.tv_software_info.clone(),
            )))
            .await;
    }

    /// Send the current `TvState` to the caller.
    pub(crate) async fn emit_tv_state(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::TvState(self.tv_state.clone()))
            .await;
    }

    /// Send the current `IsWakeTvAvailable` state to the caller.
    pub(crate) async fn emit_is_wake_tv_available(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::IsWakeLastSeenTvAvailable(
                self.is_tv_on_network.load(Ordering::SeqCst) && self.mac_addr.is_some(),
            ))
            .await;
    }
}
