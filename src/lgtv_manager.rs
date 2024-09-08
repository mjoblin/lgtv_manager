mod message_senders;
mod network_utils;
mod orchestration;
mod out_emitters;
mod persisted_state;
mod setters;

use std::cmp::PartialEq;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::{debug, error, info, warn};
use macaddr::MacAddr;
use tokio::select;
use tokio::sync::{
    mpsc::{channel, Receiver, Sender},
    Mutex, Notify,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

pub use crate::connection_settings::Connection;
pub use crate::discovery::LgTvDevice;
pub use crate::ssap_payloads::{CurrentSwInfoPayload as TvSoftwareInfo, ExternalInput as TvInput};
pub use crate::state::{LastSeenTv, TvInfo, TvState};
pub use crate::state_machine::State as ManagerStatus;
pub use crate::tv_commands::TvCommand;

use crate::lgtv_manager::persisted_state::read_persisted_state;
use crate::ssap_payloads::LgTvResponse;
use crate::state_machine::{start_state_machine, Input, Output, StateMachineUpdateMessage};
use crate::tv_network_check::TvNetworkChecker;
use crate::websocket_client::{WsCommand, WsMessage, WsStatus, WsUpdateMessage};

#[cfg(doc)]
use crate::LgTvManagerBuilder;

// CHANNEL MESSAGES -------------------------------------------------------------------------------

/// Messages sent from the caller to the [`LgTvManager`].
#[derive(Debug, Clone, PartialEq)]
pub enum ManagerMessage {
    /// Stop attempting to reconnect to the TV. Only relevant if the connection has been closed
    /// by the TV (usually because it entered standby) and the `ConnectionSettings` have
    /// `auto_reconnect` enabled. Cancellation takes effect after the current retry cycle.
    CancelReconnect,
    /// Connect to an LG TV using either a host/IP or a UPnP device.
    Connect(Connection),
    /// Connect to an LG TV using the `Connection` details from the last good connection. Requires
    /// at least one successful prior `Connect` to a TV host/IP or UPnP device during the current
    /// manager session.
    ConnectLastGoodConnection,
    /// Disconnect from the TV.
    Disconnect,
    /// Perform UPnP discovery for LG TVs on the network.
    Discover,
    /// Request sending of all currently-known Manager and TV state as instances of
    /// `ManagerOutputMessage`.
    EmitAllState,
    /// Send the given [`TvCommand`] to the connected TV.
    SendTvCommand(TvCommand),
    /// Shut down the [`LgTvManager`]. Disconnects from the TV and stops the manager task.
    ShutDown,
    /// Test the active connection to the TV. The manager will respond with
    /// [`ManagerOutputMessage::ConnectionTestStatus`].
    TestConnection,
    /// Test whether the TV is visible on the network.
    TestTvOnNetwork,
    /// Send a Wake-on-LAN network request to the last-seen TV to wake from standby. Wake-on-LAN is
    /// unavailable when the TV is fully powered off (not responding to pings), or if the MAC
    /// address is unknown.
    WakeLastSeenTv,
}

/// Messages sent from the [`LgTvManager`] back to the caller.
#[derive(Debug, Clone, PartialEq)]
pub enum ManagerOutputMessage {
    /// Status of the current connection to the TV.
    ConnectionTestStatus(TestStatus),
    /// Any LG TVs discovered via UPnP discovery.
    DiscoveredDevices(Vec<LgTvDevice>),
    /// An [`LgTvManager`] error occurred.
    Error(ManagerError),
    /// Is UPnP discovery being performed.
    IsDiscovering(bool),
    /// The manager's current reconnect flow status. The reconnect flow status exists separately
    /// from the manager status. While the reconnect flow is active, the manager's status will
    /// still cycle through its normal connect phases (`Connecting`, `Connected`, etc).
    ReconnectFlowStatus(ReconnectFlowStatus),
    /// Is it possible to wake the last-seen TV from standby.
    IsWakeLastSeenTvAvailable(bool),
    /// Information on the TV last connected to by the Manager. Connection may have been
    /// established during a previous session.
    LastSeenTv(LastSeenTv),
    /// The manager is resetting. This is usually fine (perhaps caused by an inability to connect
    /// to a TV). The manager should be functional again once in the `Disconnected` state.
    Resetting(String),
    /// Current manager status.
    Status(ManagerStatus),
    /// Status of whether the last-seen TV is on the network.
    TvOnNetworkTestStatus(TestStatus),
    /// TV information (model name, etc).
    TvInfo(Option<TvInfo>),
    /// TV inputs (HDMI, etc).
    TvInputs(Option<Vec<TvInput>>),
    /// TV software information.
    TvSoftwareInfo(Box<Option<TvSoftwareInfo>>),
    /// TV state (current volume and mute settings, etc).
    TvState(TvState),
    /// A TV error occurred.
    TvError(String),
}

// ================================================================================================
// Additional structs

/// Errors sent from the [`LgTvManager`] back to the caller.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ManagerError {
    /// At attempt to act on a received [`ManagerMessage`] was not possible. For example,
    /// attempting to send a [`TvCommand`] without an established connection.
    Action(String),
    /// An error occurred with the connection to a TV.
    Connection(String),
    /// An error occurred during UPnP discovery of LG TVs.
    Discovery(String),
    /// A fatal error has occurred. A Fatal error is unrecoverable and the manager should be
    /// considered unavailable.
    Fatal(String),
    /// An error occurred during pairing with a TV.
    Pair(String),
}

/// General test status.
#[derive(Debug, Clone, PartialEq)]
pub enum TestStatus {
    Unknown,
    InProgress,
    Passed,
    Failed,
}

/// TV reconnect flow status.
#[derive(Clone, Debug, PartialEq)]
pub enum ReconnectFlowStatus {
    /// The manager is attempting to reconnect to the TV at regular intervals.
    Active,
    /// The reconnect flow has been cancelled by the caller.
    Cancelled,
    /// The manager is not attempting to reconnect to the TV (usually because the connection is
    /// established and valid, or reconnects are disabled).
    Inactive,
    /// The manager is waiting for the TV to reappear on the network and respond to ping requests.
    WaitingForTvOnNetwork,
}

// ================================================================================================
// LgTvManager
//
// Design notes:
//
//  - The manager expects to be run once by the caller, and to run forever.
//  - The manager is asynchronous.
//  - All input from the caller is received over a receiver channel.
//  - All output to the caller is sent over a sender channel.
//  - A state machine governs the state of the manager at any given time. The loss of the state
//    machine is considered fatal.
//  - Network communication with the TV is offloaded to a WebSocket handler. The loss of a
//    WebSocket connection is not fatal (it's fine for the caller to request a disconnect).
//  - A new WebSocket handler is instantiated for each TV connection. WebSocket handlers are not
//    repurposed for multiple connections over time.
//  - Only one TV connection is managed at a time.
//  - When connecting to a TV, the manager:
//      - May first need to pair if no client key already exists (or when forced to pair).
//      - Then it must initialize itself by requesting the TV's information, current state, and
//        subscribing to volume/mute updates.
//      - Then it can enter the long-lived Communicating state, during which it can send
//        conventional commands to the TV, such as increasing volume.
//  - The manager expects to receive information from the state machine and the WebSocket handler.
//    This information is used to potentially drive new state transitions and/or feedback to the
//    the caller.
//  - The manager relies heavily on state machine Outputs to drive behavior. For example, when
//    in the "Disconnected" state, the "Connect" Input will transition the state machine into the
//    "Connecting" state *and* emit the "ConnectToTv" Output. The "ConnectToTv" output is then
//    received by the manager, which proceeds to initiate the WebSocket connection.
//  - The manager is concerned with two "alive" states:
//      - The WebSocket connection to the TV (validated with WebSocket Ping/Pong).
//      - The TV being visible on the network (validated with ICMP ping).
//  - If the manager unexpectedly loses the connection to the TV then it will enter a reconnect
//    loop.
//  - If anything unrecoverable happens, then the manager enters the Zombie state and can no
//    longer do anything useful.
// ================================================================================================

/// Manage a connection to an LG TV.
///
/// The interface to `LgTvManager` (after instantiation with [`LgTvManager::new()`] and running
/// with [`LgTvManager::run()`]) is mostly contained to the sending and receiving of
/// [`ManagerMessage`] and [`ManagerOutputMessage`].
///
/// Internally, the manager is driven by the state machine shown below. The state machine is shown
/// here only to provide context for how `LgTvManager` operates, and to give more detail for
/// [`ManagerStatus`]. (The `Zombie` state is omitted for brevity). Detailed awareness of the state
/// machine is not required to use the manager.
#[doc = include_str!("../resources/lgtvmanager_state_machine.svg")]
pub struct LgTvManager {
    // General manager state
    manager_status: ManagerStatus, // LgTvManager manager_status is just the state machine state
    tv_info: Option<TvInfo>,
    tv_inputs: Option<Vec<TvInput>>,
    tv_software_info: Option<TvSoftwareInfo>,
    tv_state: TvState,
    client_key: Option<String>, // Unique LG client key, provided by the TV after pairing
    mac_addr: Option<MacAddr>,  // MAC address of the TV, provided by UPnP discovery
    connection_details: Option<Connection>, // LgTvManager only handles up to 1 connection at a time
    last_good_connection: Option<Connection>,
    tv_host_ip: Arc<Mutex<Option<IpAddr>>>,
    tv_host_ip_notifier: Arc<Notify>,
    is_connection_initialized: bool, // TV connection has been made and initial setup commands are sent
    tv_on_network_checker: TvNetworkChecker,
    is_testing_tv_on_network: Arc<AtomicBool>,
    is_tv_on_network: Arc<AtomicBool>,
    reconnect_flow_status: ReconnectFlowStatus,
    reconnect_attempts: u64,
    session_connection_count: u64,

    // Manager in/out channels
    command_rx: Receiver<ManagerMessage>, // Receives ManagerMessage from the caller
    output_tx: Sender<ManagerOutputMessage>, // Sends ManagerOutputMessage back to the caller

    // State Machine details
    to_fsm_tx: Option<Sender<Input>>, // Sends Input messages to the state machine

    // WebSocket details
    ws_url: Option<String>,
    to_ws_tx: Option<Sender<WsMessage>>, // Sends WsMessage to the WebSocket
    from_ws_rx: Option<Receiver<WsUpdateMessage>>, // Receives WsUpdateMessage from the WebSocket
    ws_join_handle: Option<JoinHandle<()>>,

    // Additional config
    pub(crate) data_dir: Option<PathBuf>, // Where to store persisted data
}

/// Usage example:
///
/// ```no_run
/// use lgtv_manager::LgTvManager;
/// use tokio::sync::mpsc;
///
/// #[tokio::main]
/// async fn main() {
///     let (to_manager, to_manager_rx) = mpsc::channel(32);
///     let (mut manager, mut from_manager) = LgTvManager::new(to_manager_rx);
///
///     // Start a task to receive `ManagerOutputMessage` messages on `from_manager`
///     // Start a task to send `ManagerMessage` messages on `to_manager`
///
///     manager.run().await;
/// }
/// ```
impl LgTvManager {
    /// Creates an `LgTvManager` instance.
    ///
    /// Expects to be given a tokio mpsc `Receiver` of [`ManagerMessage`]s from the caller. Returns
    /// a tuple of itself and a `Receiver` of [`ManagerOutputMessage`]s back to the caller.
    ///
    /// Use [`LgTvManagerBuilder`] to override any `LgTvManager` defaults.
    pub fn new(
        command_rx: Receiver<ManagerMessage>,
    ) -> (LgTvManager, Receiver<ManagerOutputMessage>) {
        let (output_tx, manager_channel_rx) = channel(32);

        let mut manager = LgTvManager {
            manager_status: ManagerStatus::Disconnected,
            tv_info: None,
            tv_inputs: None,
            tv_software_info: None,
            tv_state: TvState::default(),
            client_key: None,
            mac_addr: None,
            connection_details: None,
            last_good_connection: None,
            tv_host_ip: Arc::new(Mutex::new(None)),
            tv_host_ip_notifier: Arc::new(Notify::new()),
            is_connection_initialized: false,
            tv_on_network_checker: TvNetworkChecker::new(),
            is_testing_tv_on_network: Arc::new(AtomicBool::new(false)),
            is_tv_on_network: Arc::new(AtomicBool::new(false)),
            reconnect_flow_status: ReconnectFlowStatus::Inactive,
            reconnect_attempts: 0,
            session_connection_count: 0,
            command_rx,
            output_tx,
            to_fsm_tx: None,
            ws_url: None,
            to_ws_tx: None,
            from_ws_rx: None,
            ws_join_handle: None,
            data_dir: None,
        };

        manager.import_persisted_state();

        (manager, manager_channel_rx)
    }

    /// Run the manager.
    ///
    /// This is the main hub of `LgTvManager`. It attempts to run forever, performing the following
    /// tasks:
    ///
    /// 1. Starting its state machine (only done once for the lifetime of the manager).
    /// 2. Starting the TV network ping task (only done once for the lifetime of the manager).
    /// 3. Looping forever, doing the following:
    ///     * Accepting (and acting on) `ManagerMessage` messages from the caller.
    ///     * Sending `ManagerOutputMessage` messages back to the caller.
    ///     * Managing a single WebSocket connection to a TV at once.
    ///     * Offloading various responsibilities to the state machine and WebSocket handler (based
    ///       on inputs from the caller, state machine, and WebSocket connection).
    ///     * Handling updates from the TV network ping task.
    pub async fn run(&mut self) {
        info!("Manager starting up");

        // Using a TaskTracker is likely overkill since we only use it to track the state machine
        // task. The websocket task is handled differently, as it potentially needs to be replaced
        // over the lifetime of the manager (which TaskTracker does not support; JoinSet might be
        // a useful alternative).
        let task_tracker = TaskTracker::new();
        let cancel_token = CancellationToken::new();

        let (to_fsm_tx, to_fsm_rx) = channel(32);
        let (from_fsm_tx, mut from_fsm_rx) = channel(32);

        self.to_fsm_tx = Some(to_fsm_tx.clone());

        start_state_machine(
            &task_tracker,
            to_fsm_rx,
            from_fsm_tx.clone(),
            cancel_token.clone(),
        );
        task_tracker.close();

        self.initialize_manager_state().await;
        self.set_host_ip_from_ws_url().await;
        self.emit_manager_status().await;
        self.set_reconnect_flow_status(ReconnectFlowStatus::Inactive)
            .await;

        info!("Manager ready to receive commands (send a Connect command first if desired)");

        // Note: An inability to send messages to the state machine is considered fatal and will
        //  result in exiting the loop.

        // Note on retries: Retries are optionally supported for both the initial connection
        //  (ManagerMessage::Connect) and lost connections (WsStatus::ServerClosedConnection). In
        //  both cases, ReconnectFlowStatus::Active is enabled which will result in an infinite
        //  retry loop (resetting the manager and trying again to connect) until either: a
        //  successful connection is achieved (identified by being asked to send a register payload
        //  via Output::SendRegisterPayload); or the retry loop is cancelled by the caller
        //  (ManagerMessage::CancelReconnect).
        //
        //  If the TV is no longer on the network, then ReconnectFlowStatus::WaitingForTvOnNetwork
        //  is set until the TV comes back online (as determined by the TV network checker), at
        //  which point the above reconnect flow resumes.

        let mut is_tv_on_network_notify: Arc<Notify> = Arc::new(Notify::new());
        let mut is_testing_tv_on_network_notify: Arc<Notify> = Arc::new(Notify::new());

        match &self.tv_on_network_checker.start() {
            Ok((is_on_network, is_on_network_notify, is_testing_tv, is_testing_tv_notify)) => {
                is_tv_on_network_notify = is_on_network_notify.clone();
                is_testing_tv_on_network_notify = is_testing_tv_notify.clone();

                self.is_tv_on_network = is_on_network.clone();
                self.is_testing_tv_on_network = is_testing_tv.clone();
            }
            Err(e) => warn!(
                "Could not start TV network checker; network ping checks are unavailable: {:?}",
                e
            ),
        }

        let mut was_on_network = false; // Identify when on-network status changes

        loop {
            select! {
                // TODO: recv() will return None when channel is closed. Consider handling here.

                // FROM THE CALLER ----------------------------------------------------------------

                Some(manager_msg) = self.command_rx.recv() => {
                    match manager_msg {
                        ManagerMessage::CancelReconnect => {
                            if self.reconnect_flow_status == ReconnectFlowStatus::Active {
                                info!("Disabling reconnects");
                                self.set_reconnect_flow_status(ReconnectFlowStatus::Cancelled).await;
                            } else {
                                warn!("Cannot disable reconnects while manager is not attempting to reconnect");
                            }
                        }
                        ManagerMessage::Connect(connection) => {
                            if let Err(e) = self.initiate_connect_to_tv(connection).await {
                                let _ = self.send_out(ManagerOutputMessage::Error(ManagerError::Connection(e))).await;
                            }
                        }
                        ManagerMessage::ConnectLastGoodConnection => {
                            if let Some(last_good_connection) = &self.last_good_connection {
                                info!("Connecting with last-good connection details: {:?}", &last_good_connection);

                                if let Err(e) = self.initiate_connect_to_tv(last_good_connection.clone()).await {
                                    let _ = self.send_out(ManagerOutputMessage::Error(ManagerError::Connection(e))).await;
                                }
                            } else {
                                warn!("No last-good connection details found; connect request ignored");
                            }
                        }
                        ManagerMessage::Disconnect => {
                            let _ = self.send_to_fsm(Input::Disconnect).await;
                        }
                        ManagerMessage::Discover => {
                            self.discover();
                        }
                        ManagerMessage::EmitAllState => {
                            self.emit_manager_status().await;
                            self.emit_all_tv_details().await;
                        }
                        ManagerMessage::SendTvCommand(lgtv_command) => {
                            let _ = self.send_to_fsm(Input::SendCommand(lgtv_command)).await;
                        }
                        ManagerMessage::ShutDown => {
                            info!("Manager shutting down");
                            cancel_token.cancel();

                            info!("Manager waiting for tasks to shut down");

                            task_tracker.wait().await;
                            self.initiate_disconnect_from_tv().await;

                            info!("Tasks shut down successfully");

                            break;
                        }
                        ManagerMessage::TestConnection => {
                            // There is no state machine state for testing the connection. If the
                            // connection test passes then the state machine is unchanged; otherwise
                            // the test failure will trigger an Error input to the state machine.
                            if self.is_connection_initialized {
                                let _ = self.send_out(
                                    ManagerOutputMessage::ConnectionTestStatus(TestStatus::InProgress)
                                ).await;
                                let _ = self.send_to_ws(WsMessage::TestConnection).await;
                            } else {
                                warn!("Cannot test connection while not connected");
                            }
                        }
                        ManagerMessage::TestTvOnNetwork => {
                            self.tv_on_network_checker.check();
                        }
                        ManagerMessage::WakeLastSeenTv => {
                            self.wake_last_seen_tv().await;
                        }
                    }
                }

                // FROM THE STATE MACHINE ---------------------------------------------------------

                Some(state_machine_update_msg) = from_fsm_rx.recv() => {
                    match state_machine_update_msg {
                        StateMachineUpdateMessage::State(sm_state) => {
                            self.set_manager_status(sm_state).await;
                        },
                        StateMachineUpdateMessage::Output(sm_output) => match sm_output {
                            Output::ConnectToTv(host) => {
                                self.start_websocket_handler_and_connect(&host).await;
                            },
                            Output::SendRegisterPayload => {
                                // If we got this far then we've connected to the TV, so no need to
                                // remain in a possible reconnecting phase.
                                self.set_reconnect_flow_status(ReconnectFlowStatus::Inactive).await;
                                self.session_connection_count += 1;

                                let _ = self.send_register_payload().await;
                            },
                            Output::PairWithTv => {
                                // We don't need to take an action here as the registration flow
                                // will have automatically triggered a prompt for pairing if required.
                            },
                            Output::InitializeConnection => {
                                let _ = self.initialize_connection().await;
                            },
                            Output::SendCommand(lgtv_command) => {
                                let payload: String = lgtv_command.into();
                                let _ = self.send_to_ws(WsMessage::Payload(payload)).await;
                            },
                            Output::DisconnectFromTv => {
                                self.optionally_prepare_for_reconnect().await;
                                self.initiate_disconnect_from_tv().await;
                            },
                            Output::HandleSuccessfulDisconnect => {
                                // A clean WebSocket disconnect has taken place
                                self.handle_successful_disconnect().await;
                            },
                            Output::HandleConnectError => {
                                self.force_manager_reset("A WebSocket connection error occurred").await;

                                if self.should_retry_connect_failure() {
                                    match self.reconnect_flow_status {
                                        ReconnectFlowStatus::Active => self.initiate_reconnect(false).await,
                                        ReconnectFlowStatus::Cancelled =>
                                            self.set_reconnect_flow_status(ReconnectFlowStatus::Inactive).await,
                                        _ => {},
                                    }
                                }
                            },
                            Output::HandleDisconnectError => {
                                self.force_manager_reset("A WebSocket disconnect error occurred").await;
                            }
                        }
                        StateMachineUpdateMessage::TransitionError(error) => {
                            error!("State machine transition error: {:?}", &error);
                            let _ = self.send_out(ManagerOutputMessage::Error(ManagerError::Action(error))).await;
                        }
                    }
                }

                // FROM THE WEBSOCKET -------------------------------------------------------------

                // NOTE: An Error from the WebSocket client is treated as a fatal error for the
                //  active connection, which will result in the LgTvManager reverting to a
                //  Disconnected state. These errors can optionally result in the initiation of a
                //  reconnect.

                ws_message = async {
                    self.from_ws_rx.as_mut().expect("WebSocket handler crash").recv().await
                }, if &self.from_ws_rx.is_some() => {
                    if let Some(ws_msg) = ws_message {
                        match ws_msg {
                            WsUpdateMessage::Status(status) => {
                                match status {
                                    WsStatus::Idle => {},
                                    WsStatus::Connected => {
                                        info!("WebSocket has connected to TV");
                                        let _ = self.send_to_fsm(Input::AttemptRegister).await;
                                    }
                                    WsStatus::Disconnected => {
                                        info!("WebSocket has disconnected from TV");
                                        let _ = self.send_to_fsm(Input::DisconnectionComplete).await;
                                    }
                                    WsStatus::ConnectError(error) => {
                                        let msg = format!("WebSocket connect error: {:?}", &error);
                                        warn!("{}", &msg);

                                        self.optionally_prepare_for_reconnect().await;
                                        self.attempt_fsm_error(ManagerError::Connection(msg)).await;
                                    },
                                    WsStatus::MessageReadError(error) => {
                                        let msg = format!("WebSocket message read error: {:?}", &error);
                                        warn!("{}", &msg);

                                        self.optionally_prepare_for_reconnect().await;
                                        self.attempt_fsm_error(ManagerError::Connection(msg)).await;
                                    },
                                    WsStatus::ServerClosedConnection => {
                                        let msg = "Server closed connection";
                                        warn!("{}", &msg);

                                        self.optionally_prepare_for_reconnect().await;
                                        self.attempt_fsm_error(ManagerError::Connection(msg.into())).await;
                                    },
                                }
                            },
                            WsUpdateMessage::Payload(ws_payload) => {
                                debug!("Manager received WebSocket payload: {:?}", ws_payload);

                                match serde_json::from_str::<LgTvResponse>(&ws_payload) {
                                    Ok(lgtv_response) => {
                                        if let Err(e) = self.handle_lgtv_response(lgtv_response).await {
                                            // Note: LG errors are handled in handle_lgtv_response()
                                            //  and will result in a separate TvError message being
                                            //  sent to the caller.
                                            error!("{}", &e);
                                        }
                                    },
                                    Err(error) => {
                                        // This is not a fatal error
                                        warn!(
                                            "Received unknown response payload from TV: {:?} :: {:?}",
                                            error,
                                            &ws_payload,
                                        );
                                    }
                                }
                            },
                            WsUpdateMessage::IsConnectionOk(is_ok) => {
                                let _ = self.send_out(ManagerOutputMessage::ConnectionTestStatus(
                                    if is_ok { TestStatus::Passed } else { TestStatus::Failed }
                                )).await;

                                if is_ok {
                                    debug!("WebSocket connection test passed");
                                } else {
                                    let msg = "WebSocket connection test failed";
                                    warn!("{}", &msg);
                                    self.attempt_fsm_error(ManagerError::Connection(msg.into())).await;
                                }
                            }
                        }
                    }
                }

                // FROM THE TV NETWORK-CHECK PING TASK --------------------------------------------

                _ = is_tv_on_network_notify.notified() => {
                    // The TV is either on of off the network. This bool state will arrive once per
                    // ping interval and may not have changed since the last notification.
                    let is_on_network = self.is_tv_on_network.load(Ordering::SeqCst);

                    let _ = self
                        .send_out(ManagerOutputMessage::TvOnNetworkTestStatus(
                            if is_on_network { TestStatus::Passed } else { TestStatus::Failed }
                        )).await;

                    self.emit_is_wake_tv_available().await;

                    // Handle the TV going offline.
                    if was_on_network && !is_on_network {
                        debug!("TV no longer on the network");

                        if self.manager_status == ManagerStatus::Disconnected {
                            self.set_reconnect_flow_status(match self.is_auto_reconnect_enabled() {
                                true => ReconnectFlowStatus::WaitingForTvOnNetwork,
                                false => ReconnectFlowStatus::Inactive,
                            })
                            .await;
                        } else {
                            self.attempt_fsm_error(
                                ManagerError::Connection("TV host is not available on the network".into())
                            ).await;
                        }
                    }

                    // Handle the TV coming back online.
                    if !was_on_network && is_on_network {
                        debug!("TV is on the network again; reconnect status: {:?}", &self.reconnect_flow_status);
                    }

                    let is_waiting_for_tv= self.reconnect_flow_status == ReconnectFlowStatus::WaitingForTvOnNetwork;

                    if !was_on_network && is_on_network && is_waiting_for_tv {
                        info!("TV is on the network again; restarting reconnect flow");
                        self.initiate_reconnect(true).await;
                    }

                    was_on_network = is_on_network;
                }

                _ = is_testing_tv_on_network_notify.notified() => {
                    // Notify the caller that a network test is being performed.
                    if self.is_testing_tv_on_network.load(Ordering::SeqCst) {
                        let _ = self
                            .send_out(ManagerOutputMessage::TvOnNetworkTestStatus(
                                TestStatus::InProgress
                            )).await;
                    }
                }
            }
        }

        info!("Manager shut down successfully");
    }

    // --------------------------------------------------------------------------------------------
    // Private

    /// Reset the manager back to its original startup state.
    ///
    /// This should be invoked when something unexpected has happened and the goal is to put the
    /// manager back into its original state to (hopefully) allow for subsequent successful
    /// Connect messages.
    ///
    /// This is effectively a non-fatal error handler. Fatal errors (such as not being able to
    /// talk to the state machine task) are treated separately.
    async fn force_manager_reset(&mut self, reason: &str) {
        warn!("Forcing a manager reset: {}", reason);

        let _ = self
            .send_out(ManagerOutputMessage::Resetting(reason.to_string()))
            .await;

        debug!("Forcing WebSocket handler shutdown");

        // Not using self.send_to_ws() to avoid getting into infinite shutdown loops
        if let Some(to_ws_tx) = &self.to_ws_tx {
            debug!("Instructing WebSocket handler to shut down");
            // Ignore channel send problems
            let _ = to_ws_tx.send(WsMessage::Command(WsCommand::ShutDown)).await;
        }

        if let Some(ws_join_handle) = self.ws_join_handle.take() {
            match ws_join_handle.await {
                Ok(_) => debug!("WebSocket join handle successfully awaited"),
                Err(e) => debug!("Could not await WebSocket join handle: {:?}", e),
            }
        }

        info!("WebSocket handler forced shutdown complete");

        self.initialize_manager_state().await;

        info!("Resetting the state machine");
        let _ = self.send_to_fsm(Input::Reset).await;

        self.emit_manager_status().await;

        info!("Manager reset complete");
    }

    /// Initialize the manager state fields to "no TV seen" values.
    async fn initialize_manager_state(&mut self) {
        self.tv_info = None;
        self.tv_inputs = None;
        self.tv_software_info = None;
        self.tv_state = TvState::default();
        self.is_connection_initialized = false;

        // If we're in a reconnect flow then we need to retain some state for later use, otherwise
        // it can be reset.
        if self.reconnect_flow_status != ReconnectFlowStatus::Active
            && self.reconnect_flow_status != ReconnectFlowStatus::WaitingForTvOnNetwork
        {
            self.connection_details = None;
            self.ws_url = None;
            self.mac_addr = None;
            self.session_connection_count = 0;
        }

        if self.reconnect_flow_status == ReconnectFlowStatus::Cancelled {
            self.set_reconnect_flow_status(ReconnectFlowStatus::Inactive)
                .await;
        }

        self.import_persisted_state();
        self.tv_on_network_checker.check();
        self.emit_all_tv_details().await;

        let _ = self
            .send_out(ManagerOutputMessage::ConnectionTestStatus(
                TestStatus::Unknown,
            ))
            .await;
        let _ = self
            .send_out(ManagerOutputMessage::TvOnNetworkTestStatus(
                TestStatus::Unknown,
            ))
            .await;
    }
}
