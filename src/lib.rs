/*!
Asynchronous control manager for LG TVs.

The `lgtv_manager` crate provides [`LgTvManager`], which manages an asynchronous interface to LG
TVs supporting the webOS SSAP WebSocket protocol.

(`lgtv_manager` has only partial support for the full set of webOS commands; see [`TvCommand`].
This crate was inspired by [LGWebOSRemote](https://github.com/klattimer/LGWebOSRemote), which
includes additional commands for reference).

`LgTvManager`:

1. Handles the WebSocket connection to an LG TV, including pairing.
2. Accepts [`ManagerMessage`] messages from the caller to:
    * Connect, disconnect, shut down, etc.
    * Send [`TvCommand`] messages (e.g. increase volume) to the TV.
3. Sends [`ManagerOutputMessage`] updates back to the caller:
    * After a successful connection:
        * The [`LastSeenTv`].
        * The [`TvInfo`] (e.g. model name).
        * The [`TvInput`] list (e.g. HDMI).
        * The [`TvSoftwareInfo`] (e.g. webOS version).
        * The [`TvState`] (e.g. current volume level).
    * As required during the lifetime of a connection:
        * Updates to the [`ManagerStatus`].
        * Updates to the [`TvState`].
    * Errors.
4. Supports UPnP discovery of LG TVs.

To view the full documentation, clone the repository and run `cargo doc --open`.

Run the examples with:

```sh
cargo run --example control
cargo run --example discover
```

### Note

Communication with `LgTvManager` is asynchronous. Commands are invoked on the TV by sending a
[`ManagerMessage`] to the manager. There is no guarantee that the manager will send an associated
[`ManagerOutputMessage`] back to the caller. However, any changes to the TV's state (like a new
volume setting) will be passed back to the caller via [`ManagerOutputMessage::TvState`].

`LgTvManager` also subscribes to volume and mute updates from the TV. This means volume or mute
changes made by other sources, such as the TV's remote control, will be reflected in `TvState`
updates. `TvState` contains the entire state of the TV at the time the message was sent.

Most use cases will rely on sending [`ManagerMessage::SendTvCommand`] messages to control the TV;
and processing any received [`ManagerOutputMessage::TvState`] messages. This asynchronous pattern
may not always be desirable. It would be possible to extend `LgTvManager` to support associating
commands with responses via a message ID.
## Common usage flow

1. **Discover** LG TVs on the network using UPnP discovery.
2. **Choose** one of the UPnP TV devices to connect to.
3. **Connect** to the TV.
4. **Wait** for the manager to enter the `Communicating` state.
5. **Loop** for as long as desired:
    * **Send [`TvCommand`] messages** to the manager.
    * **Process [`TvState`] updates** from the manager.
6. **Disconnect** from the TV.

## Instantiating

Instantiate an `LgTvManager` with [`LgTvManager::new()`], providing a channel that will be used to
send [`ManagerMessage`] messages to the manager. `LgTvManager::new()` will return a tuple of the
manager instance itself, and another channel over which the manager will send
[`ManagerOutputMessage`] messages back to the caller.

```
use lgtv_manager::LgTvManager;
use tokio::sync::mpsc;

let (to_manager, to_manager_rx) = mpsc::channel(32);
let (mut manager, mut from_manager) = LgTvManager::new(to_manager_rx);

// Send messages with to_manager.send()
// Receive messages with from_manager.recv()
```

Optionally, manager settings can be configured using the [`LgTvManagerBuilder`].

## Connecting

Connect to a TV by sending a [`ManagerMessage::Connect`] message to the manager. Connections can be
made to a TV by host, or by discovered UPnP device.

Connection settings must be specified. Use `ConnectionSettings::default()` or the
[`ConnectionSettingsBuilder`].

```
use lgtv_manager::{
    Connection, ConnectionSettingsBuilder, LgTvManager, ManagerMessage::{Connect}
};
use tokio::sync::mpsc;

# #[tokio::main]
# async fn main() {
let (to_manager, to_manager_rx) = mpsc::channel(32);

// <Instantiate and run the manager first>

to_manager
    .send(Connect(Connection::Host(
        "10.0.0.101".into(),
        ConnectionSettingsBuilder::new().with_forced_pairing().build(),
    )))
    .await;
# }
```

When using [`Connection::Host`], a best effort is made to generate a valid LG TV WebSocket
server URL from the provided host string. For example, `tv.local` will be converted to
`wss://tv.local:3001/`. This behavior can be overridden by passing a fully-qualified URL
as the host, such as `ws://10.0.1.101:3000/`, in which case the host will be used unaltered.

When using [`Connection::Device`], the WebSocket server URL is generated using the UPnP
device url. This assumes the `wss://` scheme on port `3001`.

After a successful connection, the manager will emit [`LastSeenTv`], [`TvInfo`], [`TvInput`],
[`TvSoftwareInfo`], and [`TvState`] details.

### Pairing and client keys

Successfully connecting to a TV requires accepting a pair request, during which the TV will
prompt for input and the manager will be in the [`ManagerStatus::Pairing`] state. If the pair
request is accepted then a unique client key is generated by the TV. `LgTvManager` will persist
this client key to local storage and will use it automatically for future sessions. Pairing can be
forced when sending [`ManagerMessage::Connect`].

### UPnP discovery

UPnP discovery can be used to find LG TVs on the local network by sending a
[`ManagerMessage::Discover`] message to the manager. The manager will then send messages back to
the caller, providing discovery status information with [`ManagerOutputMessage::IsDiscovering`] and
a vector of discovered devices with [`ManagerOutputMessage::DiscoveredDevices`].

### Connection flow

`LgTvManager` automatically transitions through a number of states while establishing a connection
to a TV. The manager will return a [`ManagerOutputMessage::Status`] message for each updated
state. The flow is as follows:

1. `Disconnected`
2. `Connecting`
3. `Connected`
4. `Pairing`
5. `Initializing`
6. `Communicating`
6. `Disconnecting`

Most states can be safely ignored. It is enough to instantiate `LgTvManager`, send a
[`ManagerMessage::Connect`] message, and then wait for the manager to enter the `Communicating`
state before sending commands.

The WebSocket URL is provided as String data with most states (where applicable).

See [`LgTvManager`] for a diagram of the state flow.

## Sending LG commands

Once a successful connection has been established and the manager has returned
`ManagerOutputMessage::Status(Communicating)`, arbitrary commands can be sent to the TV.

Commands are sent using [`ManagerMessage::SendTvCommand`]. Supported commands can be seen in
[`TvCommand`].

It is expected that the most common commands to send to the TV will be those that change the TV's
state (such as `SetMute`, `VolumeUp`, etc). Any changes to the TV's state will be received via
`TvState` updates from the manager, so invoking the "get" commands is usually not necessary.

## Disconnecting

Use [`ManagerMessage::Disconnect`] to disconnect from a TV. The manager will continue to run after
a disconnect, and can still accept future [`ManagerMessage::Connect`] messages. Send
[`ManagerMessage::ShutDown`] to instruct the manager to disconnect and exit.

## Limitations

The `lgtv_manager` crate is currently limited in scope. It only supports the [`TvCommand`] list
found in `src/commands.rs`, and does not provide much in the way of configuration (such as timeout
durations and automatic reconnects).

Extending `LgTvManager` to support additional TV commands should be fairly trivial, although
LG's SSAP protocol does not appear to be documented. A good place to start is the
[LGWebOSRemote](https://github.com/klattimer/LGWebOSRemote) project, which was a source of
inspiration for `lgtv_manager`.

## Examples

(Note: These examples rely on the third-party crates `env_logger` and `tokio`).

To run the examples:

```sh
cargo run --example control
cargo run --example discover
```

The `control` example creates an `LgTvManager` instance, sends commands to the manager via the
console, and prints all messages received from the manager. **This example likely won't work
without updating the TV IP address**.

```no_run
use env_logger;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use lgtv_manager::ManagerMessage::{Connect, Disconnect, SendTvCommand, ShutDown};
use lgtv_manager::ManagerStatus::Disconnected;
use lgtv_manager::{
    Connection, ConnectionSettings, LgTvManager, ManagerOutputMessage,
    TvCommand::{VolumeDown, VolumeUp},
};

#[tokio::main]
async fn main() -> Result<(), ()> {
    // Initialize manager and associated send/receive channels
    let (to_manager, to_manager_rx) = mpsc::channel(32);
    let (mut manager, mut from_manager) = LgTvManager::new(to_manager_rx);

    // Print all logs to stdout
    // TODO: Set LevelFilter::Debug to see debug logging
    env_logger::Builder::new()
        .filter(None, log::LevelFilter::Info)
        .init();

    // Task to print all messages received from the manager
    tokio::spawn(async move {
        loop {
            if let Some(manager_output_msg) = from_manager.recv().await {
                println!(
                    "<<< Received message from LgTvManager: {:?}",
                    manager_output_msg
                );

                if ManagerOutputMessage::Status(Disconnected) == manager_output_msg {
                    println!("\n>>> Manager is disconnected and ready to receive messages");
                    println!(">>> SEND CONNECT ('c') COMMAND FIRST; TV IP MUST BE VALID");
                    println!(concat!(
                        ">>> Enter command: c (connect), u (volume up), d (volume down), ",
                        "i (disconnect), s (shut down)\n"
                    ));
                }
            }
        }
    });

    let to_manager_clone = to_manager.clone();

    // Task to accept commands from the console to send to the manager
    let stdin_handle = tokio::spawn(async move {
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);

        loop {
            let mut buf = String::new();

            if reader
                .read_line(&mut buf)
                .await
                .expect("Failed to read line")
                == 0
            {
                break;
            }

            match buf.trim() {
                "c" => {
                    // TODO: Set the IP address to a valid TV on the local network
                    to_manager_clone
                        .send(Connect(Connection::Host(
                            "10.0.0.101".into(),
                            ConnectionSettings::default(),
                        )))
                        .await
                        .map_err(|_| ())?;
                }
                "u" => {
                    to_manager_clone
                        .send(SendTvCommand(VolumeUp))
                        .await
                        .map_err(|_| ())?;
                }
                "d" => {
                    to_manager_clone
                        .send(SendTvCommand(VolumeDown))
                        .await
                        .map_err(|_| ())?;
                }
                "i" => {
                    to_manager_clone.send(Disconnect).await.map_err(|_| ())?;
                }
                "s" => {
                    to_manager_clone.send(ShutDown).await.map_err(|_| ())?;
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    });

    // Run the manager until instructed to shut down (ManagerMessage::ShutDown)
    manager.run().await;

    stdin_handle.await.map_err(|_| ())?
}
```

### UPnP device discovery

The `discover` example performs LG TV UPnP device discovery. Discovery returns a vector of
[`LgTvDevice`] instances, which can be passed to the manager using [`ManagerMessage::Connect`].

```no_run
use env_logger;
use tokio::sync::mpsc;

use lgtv_manager::{
    LgTvManager,
    ManagerMessage::{Discover, ShutDown},
    ManagerOutputMessage,
};

#[tokio::main]
async fn main() -> Result<(), ()> {
    // Initialize manager and associated send/receive channels
    let (to_manager, to_manager_rx) = mpsc::channel(32);
    let (mut manager, mut from_manager) = LgTvManager::new(to_manager_rx);

    let to_manager_clone = to_manager.clone();

    // Print all logs to stdout
    env_logger::Builder::new()
        .filter(None, log::LevelFilter::Debug)
        .init();

    // Wait for a DiscoveredDevices message from the manager and display the devices
    tokio::spawn(async move {
        loop {
            if let Some(manager_output_msg) = from_manager.recv().await {
                match manager_output_msg {
                    ManagerOutputMessage::DiscoveredDevices(devices) => {
                        match devices.len() {
                            0 => println!("\nNo LG TV devices found.\n"),
                            _ => println!("\nDiscovered devices:\n\n{:?}\n", devices),
                        }

                        let _ = to_manager_clone.send(ShutDown).await;
                        break;
                    }
                    _ => {}
                }
            }
        }
    });

    // Instruct the manager to start discovering devices
    println!("Discovering LG TV devices...");
    to_manager.send(Discover).await.map_err(|_| ())?;

    manager.run().await;

    Ok(())
}
```
*/

mod commands;
mod connection_settings;
mod discovery;
mod helpers;
mod messages;
mod state;
mod state_machine;
mod websocket;

use std::path::PathBuf;

use log::{debug, error, info, warn};
use tokio::select;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::state_machine::State;
pub use commands::TvCommand;
pub use connection_settings::{Connection, ConnectionSettings, ConnectionSettingsBuilder};
use discovery::discover_lgtv_devices;
pub use discovery::LgTvDevice;
use helpers::{
    device_host, generate_lgtv_message_id, generate_possible_websocket_url,
    generate_register_request,
};
pub use messages::{CurrentSwInfoPayload as TvSoftwareInfo, ExternalInput as TvInput};
use messages::{LgTvResponse, LgTvResponsePayload};
use state::{read_persisted_state, write_persisted_state, PersistedState};
pub use state::{LastSeenTv, TvInfo, TvState};
pub use state_machine::State as ManagerStatus;
use state_machine::{start_state_machine, Input, Output, StateMachineUpdateMessage};
use websocket::{LgTvWebSocket, WsCommand, WsMessage, WsStatus, WsUpdateMessage};

// CHANNEL MESSAGES -------------------------------------------------------------------------------

/// Messages sent from the caller to the [`LgTvManager`].
#[derive(Debug, Clone, PartialEq)]
pub enum ManagerMessage {
    /// Connect to an LG TV using either a host/IP or a UPnP device.
    Connect(Connection),
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
}

/// Messages sent from the [`LgTvManager`] back to the caller.
#[derive(Debug, Clone, PartialEq)]
pub enum ManagerOutputMessage {
    /// Any LG TVs discovered via UPnP discovery.
    DiscoveredDevices(Vec<LgTvDevice>),
    /// An [`LgTvManager`] error occurred.
    Error(ManagerError),
    /// Is UPnP discovery being performed.
    IsDiscovering(bool),
    /// Information on the TV last connected to by the Manager. Connection may have been
    /// established during a previous session.
    LastSeenTv(LastSeenTv),
    /// The manager is resetting. This is usually fine (perhaps caused by an inability to connect
    /// to a TV). The manager should be functional again once in the `Disconnected` state.
    Resetting(String),
    /// Current manager status.
    Status(ManagerStatus),
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
// Errors

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

// ================================================================================================
// LgTvManagerBuilder

/// Build an [`LgTvManager`] instance.
///
/// ```
/// use std::path::Path;
///
/// use lgtv_manager::LgTvManagerBuilder;
/// use tokio::sync::mpsc;
///
/// let (to_manager_tx, to_manager_rx) = mpsc::channel(32);
///
/// let (mut manager, mut from_manager_rx) = LgTvManagerBuilder::new(to_manager_rx)
///     .with_data_dir(Path::new("/data/file/path/test").to_path_buf())
///     .build();
/// ```
pub struct LgTvManagerBuilder {
    manager: LgTvManager,
    out_channel: Receiver<ManagerOutputMessage>,
}

impl LgTvManagerBuilder {
    pub fn new(command_receiver: Receiver<ManagerMessage>) -> Self {
        debug!("Builder is instantiating an LgTvManager instance");
        let (manager, out_channel) = LgTvManager::new(command_receiver);

        LgTvManagerBuilder {
            manager,
            out_channel,
        }
    }

    /// Override the default persisted data directory (where manager data, such as the client key,
    /// are stored).
    pub fn with_data_dir(mut self, data_dir: PathBuf) -> Self {
        debug!("Builder is overriding data_dir: {:?}", data_dir);

        self.manager.data_dir = Some(data_dir);
        self.manager.clear_persisted_state_on_manager();
        self.manager.read_persisted_state();

        self
    }

    pub fn build(self) -> (LgTvManager, Receiver<ManagerOutputMessage>) {
        (self.manager, self.out_channel)
    }
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
//  - If anything unrecoverable happens, then the manager enters the Zombie state and can no
//    longer do anything useful.

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
#[doc=include_str!("../resources/lgtvmanager_state_machine.svg")]
pub struct LgTvManager {
    // General manager state
    manager_status: ManagerStatus, // LgTvManager manager_status is just the state machine state
    tv_info: Option<TvInfo>,
    tv_inputs: Option<Vec<TvInput>>,
    tv_software_info: Option<TvSoftwareInfo>,
    tv_state: TvState,
    client_key: Option<String>, // Unique LG client key, provided by the TV after pairing
    connection_details: Option<Connection>, // LgTvManager only handles up to 1 connection at a time
    connection_initialized: bool, // TV connection has been made and initial setup commands are sent

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
    data_dir: Option<PathBuf>, // Where to store persisted data
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
    // --------------------------------------------------------------------------------------------
    // Public

    /// Creates an `LgTvManager` instance.
    ///
    /// Expects to be given a [`Receiver`] of [`ManagerMessage`]s from the caller. Returns a tuple
    /// of itself and a [`Receiver`] of [`ManagerOutputMessage`]s back to the caller.
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
            connection_details: None,
            connection_initialized: false,
            command_rx,
            output_tx,
            to_fsm_tx: None,
            ws_url: None,
            to_ws_tx: None,
            from_ws_rx: None,
            ws_join_handle: None,
            data_dir: None,
        };

        manager.read_persisted_state();

        (manager, manager_channel_rx)
    }

    /// Run the manager.
    ///
    /// This is the main hub of `LgTvManager`. It attempts to run forever, performing the following
    /// tasks:
    ///
    /// 1. Starting its state machine (only done once for the lifetime of the manager).
    /// 2. Looping forever, doing the following:
    ///     * Accepting (and acting on) `ManagerMessage` messages from the caller.
    ///     * Sending `ManagerOutputMessage` messages back to the caller.
    ///     * Managing a single WebSocket connection to a TV at once.
    ///     * Offloading various responsibilities to the state machine and WebSocket handler (based
    ///       on inputs from the caller, state machine, and WebSocket connection).
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
        self.emit_manager_status().await;

        info!("Manager ready to receive commands (send a Connect command first if desired)");

        // Note: An inability to send messages to the state machine is considered fatal and will
        //  result in exiting the loop.

        loop {
            select! {
                // TODO: recv() will return None when channel is closed. Consider handling here.

                // FROM THE CALLER ----------------------------------------------------------------

                Some(manager_msg) = self.command_rx.recv() => {
                    match manager_msg {
                        ManagerMessage::Connect(connection) => {
                            if let Err(e) = self.initiate_connect_to_tv(connection).await {
                                let _ = self.send_out(ManagerOutputMessage::Error(ManagerError::Connection(e))).await;
                            }
                        }
                        ManagerMessage::Disconnect => {
                            let _ = self.send_to_fsm(Input::Disconnect).await;
                        },
                        ManagerMessage::Discover => {
                            self.discover();
                        },
                        ManagerMessage::EmitAllState => {
                            self.emit_manager_status().await;
                            self.emit_all_tv_details().await;
                        }
                        ManagerMessage::SendTvCommand(lgtv_command) => {
                            let _ = self.send_to_fsm(Input::SendCommand(lgtv_command)).await;
                        },
                        ManagerMessage::ShutDown => {
                            info!("Manager shutting down");
                            cancel_token.cancel();

                            info!("Manager waiting for tasks to shut down");

                            task_tracker.wait().await;
                            self.initiate_disconnect_from_tv().await;

                            info!("Tasks shut down successfully");

                            break;
                        },
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
                                self.initiate_disconnect_from_tv().await;
                            },
                            Output::HandleSuccessfulDisconnect => {
                                // A clean WebSocket disconnect has taken place
                                self.handle_successful_disconnect().await;
                            },
                            Output::HandleConnectionError => {
                                self.force_manager_reset("A WebSocket error occurred").await;
                            }
                        }
                        StateMachineUpdateMessage::TransitionError(error) => {
                            error!("State machine transition error: {:?}", &error);
                            let _ = self.send_out(ManagerOutputMessage::Error(ManagerError::Action(error))).await;
                        }
                    }
                }

                // FROM THE WEBSOCKET -------------------------------------------------------------

                // NOTE: An Error from the WebSocket client is treated as a fatal error which will
                //  result in the LgTvManager reverting a Disconnected state.

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
                                    WsStatus::Error(error) => {
                                        let msg = format!("WebSocket error: {:?}", &error);
                                        error!("{}", &msg);
                                        self.attempt_fsm_error(ManagerError::Connection(msg)).await;
                                    }
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
                        }
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
        self.connection_details = None;
        self.connection_initialized = false;
        self.ws_url = None;

        self.read_persisted_state();
        self.emit_all_tv_details().await;
    }

    /// Emit all current TV details to the caller.
    async fn emit_all_tv_details(&mut self) {
        self.emit_last_seen_tv().await;
        self.emit_tv_info().await;
        self.emit_tv_inputs().await;
        self.emit_tv_software_info().await;
        self.emit_tv_state().await;
    }

    /// Discover LG TVs on the local network using UPnP discovery.
    ///
    /// Discovered TVs are sent back to the caller as a
    /// `ManagerOutputMessage::DiscoveredDevices` message (containing a vector of `TvDevice`
    /// instances).
    fn discover(&mut self) {
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

    /// Process an incoming `LgTvResponse` received over the WebSocket.
    ///
    /// This usually entails one of:
    ///
    /// * Treating the response as a trigger to send an `Input` to the state machine (especially
    ///   during the initial connection/pairing/initialization phases).
    /// * Treating the response as information about the TV's state, and announcing the change via
    ///   an `ManagerOutputMessage`.
    async fn handle_lgtv_response(&mut self, lgtv_response: LgTvResponse) -> Result<(), String> {
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
                        self.tv_state.power_state = Some(power_state_payload.state.clone());
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
                        if !self.connection_initialized {
                            info!("TV connection initialized");
                            let _ = self.send_to_fsm(Input::StartCommunication).await;
                            self.connection_initialized = true;
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
                self.write_persisted_state();

                let _ = self.send_to_fsm(Input::Initialize).await;
            }
            LgTvResponse::Error(error_response) => {
                // Pairing errors are an expected TvError, which we want to manage via the state
                // machine. Non-pairing errors will be reported back to the caller, but are
                // otherwise ignored.
                error!("Received error from TV: {:?}", &error_response);

                let is_pairing_error = match &error_response.error {
                    Some(error_string) => error_string.contains("pairing"),
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

    // --------------------------------------------------------------------------------------------
    // Flow management between the Manager, State Machine, and WebSocket
    //
    // These functions are invoked as a result of:
    //
    //  * A received `ManagerMessage` from the caller.
    //  * A received `Output` from the state machine.
    //  * A received `WsUpdate` message from the WebSocket handler.

    /// Initiate a connection to an LG TV.
    ///
    /// The provided `connection` determines whether to connect to a TV host (network name/IP
    /// address) or a previously-discovered UPnP device, as well as the `ConnectionSettings`.
    ///
    /// The connection flow is initiated by passing `Input::Connect` to the state machine.
    async fn initiate_connect_to_tv(&mut self, connection: Connection) -> Result<(), String> {
        if self.manager_status != ManagerStatus::Disconnected {
            // Note: There's a lag between a connection request and the state machine entering a
            // non-Disconnected state, so this check isn't perfect -- but it should be good enough
            // for most use cases.
            Err(format!(
                "Cannot connect while not in Disconnected state (current state: {:?})",
                &self.manager_status
            ))
        } else {
            // Determine the WebSocket URL to connect to
            let tv_url = match &connection {
                Connection::Host(host, _) => host.clone(),
                Connection::Device(device, _) => match device_host(device) {
                    Ok(host) => host.clone(),
                    Err(e) => {
                        return Err(e);
                    }
                },
            };

            // Initiate the connection. This will provide the Connect input to the FSM. The FSM
            // will then take care of entering the Connecting state which will then drive the
            // actual connection to the WebSocket server.
            let is_tls = match &connection {
                Connection::Host(_, settings) => settings.is_tls,
                Connection::Device(_, settings) => settings.is_tls,
            };

            if self.to_fsm_tx.is_some() {
                match generate_possible_websocket_url(&tv_url, is_tls) {
                    Ok(url) => {
                        self.connection_details = Some(connection);
                        let _ = self.send_to_fsm(Input::Connect(url.clone())).await;
                        self.ws_url = Some(url);

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
    async fn start_websocket_handler_and_connect(&mut self, host: &str) {
        info!("Initiating connection to TV: {:?}", &host);

        let (to_ws_tx, to_ws_rx) = channel(32);
        let (from_ws_tx, from_ws_rx) = channel(32);

        self.to_ws_tx = Some(to_ws_tx.clone());
        self.from_ws_rx = Some(from_ws_rx);

        let ws_handler = LgTvWebSocket::new();

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
    async fn send_register_payload(&mut self) {
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

    /// Initialize the manager from a new WebSocket connection.
    ///
    /// Performs steps that need to take place at the beginning of a new TV connection, such as
    /// retrieving TV details and subscribing to TV updates.
    async fn initialize_connection(&mut self) {
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
    }

    /// Initiate a disconnect from the TV.
    ///
    /// This starts a disconnect flow by sending a ShutDown request to the WebSocket server. This
    /// is only the beginning of the disconnect, and further steps will come later.
    async fn initiate_disconnect_from_tv(&mut self) {
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
    async fn handle_successful_disconnect(&mut self) {
        if let Some(ws_join_handle) = self.ws_join_handle.take() {
            match ws_join_handle.await {
                Ok(_) => debug!("WebSocket join handle successfully awaited"),
                Err(e) => error!("Could not await WebSocket join handle: {:?}", e),
            }
        }

        self.initialize_manager_state().await;
    }

    /// Ask the state machine to begins its Error flow.
    ///
    /// This should trigger an attempt to cleanly shut down an existing WebSocket connected and
    /// return to a clean Disconnected state.
    async fn attempt_fsm_error(&mut self, manager_error: ManagerError) {
        let _ = self.send_to_fsm(Input::Error).await;
        let _ = self
            .send_out(ManagerOutputMessage::Error(manager_error))
            .await;
    }

    /// Enter a zombie state.
    ///
    /// A manager in the zombie state is effectively dormant and can no longer transition into
    /// new states. It should likely be shut down with `ManagerMessage::ShutDown`.
    async fn enter_zombie_state(&mut self, reason: &str) {
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

    // --------------------------------------------------------------------------------------------
    // Message senders

    /// Send a `ManagerOutputMessage` back to the caller.
    async fn send_out(&self, message: ManagerOutputMessage) -> Result<(), ()> {
        self.output_tx.send(message).await.map_err(|_| {
            warn!("Output channel unexpectedly closed");
        })
    }

    /// Send a `ManagerOutputMessage` back to the caller using the given `sender`.
    async fn send_out_with_sender(
        sender: &Sender<ManagerOutputMessage>,
        message: ManagerOutputMessage,
    ) -> Result<(), ()> {
        sender.send(message).await.map_err(|_| {
            warn!("Output channel unexpectedly closed");
        })
    }

    /// Send an `Input` to the state machine.
    ///
    /// Not being able to send the message is treated as a fatal error. In theory this should never
    /// occur, but if the state machine can't be provided with a new Input then the manager is
    /// effectively unable to proceed -- so it enters its zombie state.
    async fn send_to_fsm(&mut self, message: Input) -> Result<(), ()> {
        match &self.to_fsm_tx {
            Some(to_fsm_tx) => match to_fsm_tx.send(message).await {
                Ok(_) => Ok(()),
                Err(_) => {
                    let msg = "The channel to the state machine has unexpectedly closed";
                    error!("{}", &msg);

                    self.enter_zombie_state(msg).await;

                    Err(())
                }
            },
            // The state machine task may nto have started yet, so no channel is OK
            None => Ok(()),
        }
    }

    /// Send a `WsMessage` to the WebSocket handler.
    ///
    /// Not being able to send the message is treated as a non-fatal error. In theory this should
    /// never occur, but if the manager can't send to the WebSocket manager then it attempts to
    /// reset itself.
    async fn send_to_ws(&mut self, message: WsMessage) -> Result<(), ()> {
        match &self.to_ws_tx {
            Some(to_ws_tx) => match to_ws_tx.send(message).await {
                Ok(_) => Ok(()),
                Err(_) => {
                    self.force_manager_reset("WebSocket handler channel unexpectedly closed")
                        .await;

                    Err(())
                }
            },
            // It's OK to not have a WebSocket sender available, especially between connects,
            // although in theory we shouldn't encounter this case in a normal/happy path
            None => Ok(()),
        }
    }

    // --------------------------------------------------------------------------------------------
    // State management

    /// Send the current `ManagerStatus` to the caller.
    async fn emit_manager_status(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::Status(self.manager_status.clone()))
            .await;
    }

    /// Update the current manager status and announce the change to the caller.
    async fn set_manager_status(&mut self, status: ManagerStatus) {
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

    /// Send `LastSeenTv` details to the caller.
    async fn emit_last_seen_tv(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::LastSeenTv(LastSeenTv {
                websocket_url: self.ws_url.clone(),
                client_key: self.client_key.clone(),
            }))
            .await;
    }

    /// Send the current `TvInfo` to the caller.
    async fn emit_tv_info(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::TvInfo(self.tv_info.clone()))
            .await;
    }

    /// Send the current `TvInput` list to the caller.
    async fn emit_tv_inputs(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::TvInputs(self.tv_inputs.clone()))
            .await;
    }

    /// Send the current `TvSoftwareInfo` to the caller.
    async fn emit_tv_software_info(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::TvSoftwareInfo(Box::new(
                self.tv_software_info.clone(),
            )))
            .await;
    }

    /// Send the current `TvState` to the caller.
    async fn emit_tv_state(&mut self) {
        let _ = self
            .send_out(ManagerOutputMessage::TvState(self.tv_state.clone()))
            .await;
    }

    /// Import any previously-persisted `PersistedState` details from disk and store on the manager.
    fn read_persisted_state(&mut self) {
        match read_persisted_state(self.data_dir.clone()) {
            Ok(persisted_state) => {
                self.ws_url = persisted_state.ws_url;
                self.client_key = persisted_state.client_key;
            }
            Err(e) => {
                warn!("{}", e);
            }
        }
    }

    /// Persist any `PersistedState` details associated with the manager to disk.
    fn write_persisted_state(&mut self) {
        if let Err(e) = write_persisted_state(
            PersistedState {
                ws_url: self.ws_url.clone(),
                client_key: self.client_key.clone(),
            },
            self.data_dir.clone(),
        ) {
            warn!("{}", e);
        }
    }

    fn clear_persisted_state_on_manager(&mut self) {
        self.client_key = None;
        self.ws_url = None;
    }
}
