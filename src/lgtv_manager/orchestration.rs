use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::time::SystemTime;
use tokio::sync::mpsc::channel;
use tokio::time;

use log::{debug, error, info, warn};
use macaddr::MacAddr;

use super::LgTvManager;
use crate::helpers::{generate_register_request, url_ip_addr, websocket_url_for_connection};
use crate::lgtv_manager::read_persisted_state;
use crate::ssap_payloads::{LgTvResponse, LgTvResponsePayload};
use crate::state_machine::{Input, State};
use crate::websocket_client::{LgTvWebSocket, WsCommand, WsMessage};
use crate::{
    Connection, ManagerError, ManagerOutputMessage, ManagerStatus, PowerState, ReconnectDetails,
    ReconnectFlowStatus, TvCommand,
};

// ------------------------------------------------------------------------------------------------
// Orchestration between the Manager, State Machine, and WebSocket
//
// These functions are invoked as a result of:
//
//  * A received `ManagerMessage` from the caller.
//  * A received `Output` from the state machine.
//  * A received `WsUpdate` message from the WebSocket handler.
// ------------------------------------------------------------------------------------------------

impl LgTvManager {
    /// Initiate a connection to an LG TV.
    ///
    /// The provided `connection` determines whether to connect to a TV host (network name/IP
    /// address) or a previously-discovered UPnP device, as well as the `ConnectionSettings`.
    ///
    /// The connection flow is initiated by passing `Input::Connect` to the state machine.
    pub(crate) async fn initiate_connect_to_tv(
        &mut self,
        connection: Connection,
    ) -> Result<(), String> {
        // Check whether the Connection contains a client key override.
        if let Some(manual_client_key) = match &connection {
            Connection::Device(_, settings) => settings.client_key.clone(),
            Connection::Host(_, settings) => settings.client_key.clone(),
        } {
            info!("Using client key from connection settings");
            self.client_key = Some(manual_client_key);
        }

        if self.manager_status != ManagerStatus::Disconnected {
            // Note: There's a lag between a connection request and the state machine entering a
            // non-Disconnected state, so this check isn't perfect -- but it should be good enough
            // for most use cases.
            Err(format!(
                "Cannot connect while not in Disconnected state (current state: {:?})",
                &self.manager_status
            ))
        } else {
            if self.to_fsm_tx.is_some() {
                match websocket_url_for_connection(&connection) {
                    Ok(url) => {
                        self.connection_details = Some(connection.clone());

                        // Register this connection's IP address with the network checker, but only
                        // if the checker doesn't have an IP or the IP has changed
                        if let Some(conn_ip_addr) = url_ip_addr(&url) {
                            match self.tv_on_network_checker.get_tv_ip().await {
                                Some(existing_ip) => {
                                    if conn_ip_addr != existing_ip {
                                        self.tv_on_network_checker.set_tv_ip(conn_ip_addr).await;
                                    }
                                }
                                None => self.tv_on_network_checker.set_tv_ip(conn_ip_addr).await,
                            }
                        }

                        // Enable connect retries if requested in the connection settings
                        self.set_reconnect_flow_status(
                            match self.is_initial_connect_retries_enabled() {
                                true => ReconnectFlowStatus::Active,
                                false => ReconnectFlowStatus::Inactive,
                            },
                        )
                        .await;

                        let _ = self.send_to_fsm(Input::Connect(url.clone())).await;

                        // Store the WebSocket URL and MAC address for this connection.
                        self.ws_url = Some(url.clone());

                        // A UPnP Device connection should include the MAC address, but a Host
                        // connection won't. For a Host, we check if we can use the mac address
                        // last persisted to disk. This really just supports the case where a
                        // previous Device connection is being re-established as a Host connection.
                        self.mac_addr = match connection {
                            Connection::Host(_, _) => {
                                match read_persisted_state(self.data_dir.clone()) {
                                    Ok(persisted) => {
                                        if persisted.ws_url == Some(url) {
                                            debug!(
                                                "Using persisted MAC address for Host connection: {:?}",
                                                &persisted.mac_addr,
                                            );

                                            persisted.mac_addr.as_ref().and_then(|mac_addr| {
                                                MacAddr::from_str(mac_addr).ok()
                                            })
                                        } else {
                                            None
                                        }
                                    }
                                    Err(_) => None,
                                }
                            }
                            Connection::Device(device, _) => {
                                device.mac_addr.and_then(|mac_addr_string| {
                                    MacAddr::from_str(&mac_addr_string).ok()
                                })
                            }
                        };

                        Ok(())
                    }
                    Err(e) => Err(format!("Error determining TV URL: {}", e)),
                }
            } else {
                Err("Cannot connect before state machine channel is available".into())
            }
        }
    }

    /// Start a WebSocket handler and have it attempt to connect to the TV.
    pub(crate) async fn start_websocket_handler_and_connect(&mut self, host: &str) {
        info!("Initiating connection to TV: {:?}", &host);

        let (to_ws_tx, to_ws_rx) = channel(32);
        let (from_ws_tx, from_ws_rx) = channel(32);

        self.to_ws_tx = Some(to_ws_tx.clone());
        self.from_ws_rx = Some(from_ws_rx);

        let mut ws_handler = LgTvWebSocket::new();

        info!(
            "Starting WebSocket handler; connection details: {:?}",
            self.connection_details
        );

        let is_tls = match &self.connection_details {
            Some(connection) => match connection {
                Connection::Host(_, settings) => settings.is_tls,
                Connection::Device(_, settings) => settings.is_tls,
            },
            None => true,
        };

        self.ws_join_handle = Some(ws_handler.start(host, is_tls, to_ws_rx, from_ws_tx));
    }

    /// Send an LG registration payload to the TV.
    ///
    /// Registration may or may not result in a pair flow, depending on whether a valid client
    /// key is available.
    pub(crate) async fn send_register_payload(&mut self) {
        info!("Registering with TV");

        let force_pairing = match &self.connection_details {
            Some(connection) => match connection {
                Connection::Host(_, settings) => settings.force_pairing,
                Connection::Device(_, settings) => settings.force_pairing,
            },
            None => false,
        };

        let result = match generate_register_request(self.client_key.clone(), force_pairing) {
            Ok(register_request) => match serde_json::to_string(&register_request) {
                Ok(register_payload_string) => {
                    let _ = self
                        .send_to_ws(WsMessage::Payload(register_payload_string))
                        .await;

                    // A failed send will result in a manager reset, so this is considered Ok
                    Ok(())
                }
                Err(e) => Err(format!(
                    "Could not generate register payload string: {:?}",
                    e
                )),
            },
            Err(e) => Err(e),
        };

        if let Err(e) = result {
            self.force_manager_reset(&e).await;
        }
    }

    /// Initialize the manager with TV information retrieved from a new WebSocket connection.
    ///
    /// Performs steps that need to take place at the beginning of a new TV connection, such as
    /// retrieving TV details and subscribing to TV updates.
    pub(crate) async fn initialize_connection(&mut self) {
        info!("Initializing TV connection");

        // TODO: See if screen on/off state can be determined

        let _ = self
            .send_to_ws(WsMessage::Payload(TvCommand::GetCurrentSWInfo.into()))
            .await;
        let _ = self
            .send_to_ws(WsMessage::Payload(TvCommand::GetExternalInputList.into()))
            .await;
        let _ = self
            .send_to_ws(WsMessage::Payload(TvCommand::GetPowerState.into()))
            .await;
        let _ = self
            .send_to_ws(WsMessage::Payload(TvCommand::GetSystemInfo.into()))
            .await;
        let _ = self
            .send_to_ws(WsMessage::Payload(TvCommand::SubscribeGetVolume.into()))
            .await;
        let _ = self
            .send_to_ws(WsMessage::Payload(TvCommand::SubscribeGetPowerState.into()))
            .await;

        // Track this connection as our last-known-good-connection, if it's a Host or Device.
        if let Some(current_connection) = &self.connection_details {
            self.last_good_connection = Some(current_connection.clone());
        }
    }

    /// Process an incoming `LgTvResponse` received over the WebSocket.
    ///
    /// This usually entails one of:
    ///
    /// * Treating the response as a trigger to send an `Input` to the state machine (especially
    ///   during the initial connection/pairing/initialization phases).
    /// * Treating the response as information about the TV's state, and announcing the change via
    ///   an `ManagerOutputMessage`.
    pub(crate) async fn handle_lgtv_response(
        &mut self,
        lgtv_response: LgTvResponse,
    ) -> Result<(), String> {
        debug!("LG TV response payload received: {:?}", &lgtv_response);

        if self.to_fsm_tx.is_none() {
            return Err("Cannot handle LG response payload without a FSM tx channel".into());
        };

        match lgtv_response {
            LgTvResponse::Command(command_response) => {
                match &command_response.payload {
                    LgTvResponsePayload::CurrentSwInfo(software_info_payload) => {
                        self.tv_software_info = Some((*software_info_payload).clone());
                        self.emit_tv_software_info().await;
                    }
                    LgTvResponsePayload::GetExternalInputList(external_inputs_payload) => {
                        self.tv_inputs = Some(external_inputs_payload.devices.clone());
                        self.emit_tv_inputs().await;
                    }
                    LgTvResponsePayload::GetPowerState(power_state_payload) => {
                        self.tv_state.power_state = Some((*power_state_payload).clone().into());
                        self.emit_tv_state().await;
                    }
                    LgTvResponsePayload::GetSystemInfo(system_info_payload) => {
                        self.tv_info = Some((*system_info_payload).clone().into());
                        self.emit_tv_info().await;
                    }
                    LgTvResponsePayload::GetVolume(volume_payload) => {
                        // This GetVolume payload will be received during both the initialization
                        // phase (due to the GetVolume subscription) and any subsequent
                        // SetVolume/SetMute calls. This means we can safely ignore the
                        // LgTvResponsePayload::{SetVolume, SetMute} enum variants.
                        //
                        // Note: We also treat a GetVolume response as completion of the
                        // initialization phase, even though initialization also includes
                        // SystemInfo and PowerStatus requests.
                        //
                        // TODO: If initialization needs to be more precise, consider storing the
                        //  initialization request IDs and detecting when all responses with the
                        //  same IDs have been received.
                        if !self.is_connection_initialized {
                            info!("TV connection initialized");
                            let _ = self.send_to_fsm(Input::StartCommunication).await;
                            self.is_connection_initialized = true;

                            // This connection is considered valid, so we set the host IP which
                            // will inform the alive checker of the TV's IP address.
                            self.set_host_ip_from_ws_url().await;

                            info!("Manager ready to send commands to TV");
                        }

                        self.tv_state.volume = Some(volume_payload.volume_status.volume);
                        self.tv_state.is_muted = Some(volume_payload.volume_status.mute_status);
                        self.tv_state.is_volume_settable =
                            Some(volume_payload.volume_status.adjust_volume);
                        self.emit_tv_state().await;
                    }
                    LgTvResponsePayload::Pair(pair_info) => {
                        info!("Got pair request");

                        match pair_info.pairing_type.as_str() {
                            "PROMPT" => {
                                info!("Prompting user for TV pair");
                                let _ = self.send_to_fsm(Input::Pair).await;
                            }
                            pair_type => {
                                let msg = format!("Unexpected pair type: {pair_type}");
                                error!("Pair request error: {}", &msg);
                                self.attempt_fsm_error(ManagerError::Pair(msg)).await;
                            }
                        }
                    }
                    LgTvResponsePayload::PlainReturnValue(_) => {}
                    LgTvResponsePayload::SetScreenOn(screen_on_payload) => {
                        self.tv_state.is_screen_on = Some(screen_on_payload.state == "Active");
                        self.emit_tv_state().await;
                    }
                    // Registered and Error payloads are handled as separate response types
                    _ => {}
                }
            }
            LgTvResponse::Registered(registered_response) => {
                info!("TV client registration complete");

                self.client_key = Some(registered_response.payload.client_key);
                self.write_persisted_state().await;

                let _ = self.send_to_fsm(Input::Initialize).await;
            }
            LgTvResponse::Error(error_response) => {
                // Pairing errors are an expected TvError, which we want to manage via the state
                // machine. Non-pairing errors will be reported back to the caller, but are
                // otherwise ignored.
                error!("Received error from TV: {:?}", &error_response);

                let is_pairing_error = match &error_response.error {
                    Some(error_string) => {
                        error_string.contains("pairing") || error_string.contains("403 cancelled")
                    }
                    None => false,
                };

                let _ = self
                    .send_out(ManagerOutputMessage::TvError(
                        error_response
                            .error
                            .unwrap_or_else(|| String::from("Unknown TV error")),
                    ))
                    .await;

                if is_pairing_error {
                    let _ = self.send_to_fsm(Input::Error).await;
                }
            }
        }

        Ok(())
    }

    /// Initiate a disconnect from the TV.
    ///
    /// This starts a disconnect flow by sending a ShutDown request to the WebSocket server. This
    /// is only the beginning of the disconnect, and further steps will come later.
    pub(crate) async fn initiate_disconnect_from_tv(&mut self) {
        // If we don't have a WebSocket manager task join handle then we probably got here via a
        // system shutdown while not connected. All this check does is prevent an unnecessary
        // (and likely harmless) error-state-followed-by-cleanup flow from triggering.
        if self.ws_join_handle.is_some() {
            info!("Disconnecting from TV");
            let _ = self
                .send_to_ws(WsMessage::Command(WsCommand::ShutDown))
                .await;
        }
    }

    /// Handle a successful disconnect from the WebSocket server.
    pub(crate) async fn handle_successful_disconnect(&mut self) {
        if let Some(ws_join_handle) = self.ws_join_handle.take() {
            match ws_join_handle.await {
                Ok(_) => debug!("WebSocket join handle successfully awaited"),
                Err(e) => error!("Could not await WebSocket join handle: {:?}", e),
            }
        }

        self.initialize_manager_state().await;

        // A lost connection, once detected, still triggers the standard disconnect flow -- so if
        // we end up here handling a successful disconnect then we may want to trigger a reconnect.
        if self.reconnect_flow_status == ReconnectFlowStatus::Active {
            self.initiate_reconnect(false).await;
        }
    }

    /// Initiate a TV reconnect flow.
    ///
    /// Waits for the reconnect interval to pass. While waiting, it emits a
    /// `ManagerStatus::Reconnecting` message to the caller once per second. Once the reconnect
    /// interval has passed, it initiates a conventional Connect flow using the previously-provided
    /// `Connection` details.
    pub(crate) async fn initiate_reconnect(&mut self, immediate: bool) {
        if !self.is_tv_on_network.load(Ordering::SeqCst)
            && self.tv_on_network_checker.is_operational
        {
            warn!("Preventing reconnect attempt while TV host is down");
            return;
        }

        // Forcing Active as we might be re-emerging from WaitingForTvOnNetwork
        self.set_reconnect_flow_status(ReconnectFlowStatus::Active)
            .await;

        let url = match &self.ws_url {
            Some(url) => url.clone(),
            None => {
                self.set_reconnect_flow_status(ReconnectFlowStatus::Inactive)
                    .await;
                self.force_manager_reset(
                    "Unable to initiate reconnect: cannot determine URL to re-connect to",
                )
                .await;

                return;
            }
        };

        if let Some(connection_details) = &self.connection_details {
            let reconnect_start_time = SystemTime::now();
            let retry_interval = 5;
            let mut feedback_interval = time::interval(time::Duration::from_secs(1));
            feedback_interval.tick().await;

            if immediate {
                info!("Attempting immediate reconnect");
            } else {
                info!("Will attempt reconnect in {retry_interval}s");
            }

            loop {
                if immediate {
                    break;
                }

                let now = SystemTime::now();

                if let Ok(elapsed_duration) = now.duration_since(reconnect_start_time) {
                    if elapsed_duration.as_secs() >= retry_interval {
                        break;
                    }

                    let time_to_retry = retry_interval - elapsed_duration.as_secs();
                    debug!("Time to retry: {time_to_retry}s, {:?}", &connection_details);

                    let _ = self
                        .send_out(ManagerOutputMessage::Status(ManagerStatus::Reconnecting(
                            ReconnectDetails {
                                url: url.clone(),
                                attempts: self.reconnect_attempts,
                                next_attempt_secs: time_to_retry,
                            },
                        )))
                        .await;

                    feedback_interval.tick().await;
                }
            }

            self.reconnect_attempts += 1;
            info!(
                "Attempting reconnect #{} to TV: {}",
                self.reconnect_attempts, &url
            );

            if let Err(e) = self
                .initiate_connect_to_tv(connection_details.clone())
                .await
            {
                let _ = self
                    .send_out(ManagerOutputMessage::Error(ManagerError::Connection(e)))
                    .await;
            }
        }
    }

    /// Ask the state machine to begins its Error flow.
    ///
    /// This should trigger an attempt to cleanly shut down an existing WebSocket connected and
    /// return to a clean Disconnected state.
    pub(crate) async fn attempt_fsm_error(&mut self, manager_error: ManagerError) {
        let _ = self.send_to_fsm(Input::Error).await;
        let _ = self
            .send_out(ManagerOutputMessage::Error(manager_error))
            .await;
    }

    /// Enter a zombie state.
    ///
    /// A manager in the zombie state is effectively dormant and can no longer transition into
    /// new states. It should likely be shut down with `ManagerMessage::ShutDown`.
    pub(crate) async fn enter_zombie_state(&mut self, reason: &str) {
        error!(
            "{}{}{}",
            "Entering zombie (unresponsive) state; the manager should probably be shut down by ",
            "sending ManagerMessage:ShutDown. Reason: ",
            reason
        );

        let _ = self
            .send_out(ManagerOutputMessage::Error(ManagerError::Fatal(
                reason.into(),
            )))
            .await;

        self.set_manager_status(State::Zombie).await;
    }
}
