# lgtv_manager

Asynchronous control manager for LG TVs.

The `lgtv_manager` crate provides `LgTvManager`, which manages an asynchronous interface to LG
TVs supporting the webOS SSAP WebSocket protocol.

(`lgtv_manager` has only partial support for the full set of webOS commands; see `TvCommand`.
This crate was inspired by [LGWebOSRemote](https://github.com/klattimer/LGWebOSRemote), which
includes additional commands for reference).

`LgTvManager`:

1. Handles the WebSocket connection to an LG TV, including pairing.
2. Accepts `ManagerMessage` messages from the caller to:
    * Connect, disconnect, shut down, etc.
    * Send `TvCommand` messages (e.g. increase volume) to the TV.
3. Sends `ManagerOutputMessage` updates back to the caller:
    * After a successful connection:
        * The `LastSeenTv`.
        * The `TvInfo` (e.g. model name).
        * The `TvInput` list (e.g. HDMI).
        * The `TvSoftwareInfo` (e.g. webOS version).
        * The `TvState` (e.g. current volume level).
    * As required during the lifetime of a connection:
        * Updates to the `ManagerStatus`.
        * Updates to the `TvState`.
    * Errors.
4. Supports UPnP discovery of LG TVs.

To view the full documentation, clone the repository and run `cargo doc --open`.

Run the examples with:

```sh
cargo run --example control
cargo run --example discover
```

#### Note

Communication with `LgTvManager` is asynchronous. Commands are invoked on the TV by sending a
`ManagerMessage` to the manager. There is no guarantee that the manager will send an associated
`ManagerOutputMessage` back to the caller. However, any changes to the TV's state (like a new
volume setting) will be passed back to the caller via `ManagerOutputMessage::TvState`.

`LgTvManager` also subscribes to volume and mute updates from the TV. This means volume or mute
changes made by other sources, such as the TV's remote control, will be reflected in `TvState`
updates. `TvState` contains the entire state of the TV at the time the message was sent.

Most use cases will rely on sending `ManagerMessage::SendTvCommand` messages to control the TV;
and processing any received `ManagerOutputMessage::TvState` messages. This asynchronous pattern
may not always be desirable. It would be possible to extend `LgTvManager` to support associating
commands with responses via a message ID.
### Common usage flow

1. **Discover** LG TVs on the network using UPnP discovery.
2. **Choose** one of the UPnP TV devices to connect to.
3. **Connect** to the TV.
4. **Wait** for the manager to enter the `Communicating` state.
5. **Loop** for as long as desired:
    * **Send `TvCommand` messages** to the manager.
    * **Process `TvState` updates** from the manager.
6. **Disconnect** from the TV.

### Instantiating

Instantiate an `LgTvManager` with `LgTvManager::new()`, providing a channel that will be used to
send `ManagerMessage` messages to the manager. `LgTvManager::new()` will return a tuple of the
manager instance itself, and another channel over which the manager will send
`ManagerOutputMessage` messages back to the caller.

```rust
use lgtv_manager::LgTvManager;
use tokio::sync::mpsc;

let (to_manager, to_manager_rx) = mpsc::channel(32);
let (mut manager, mut from_manager) = LgTvManager::new(to_manager_rx);

// Send messages with to_manager.send()
// Receive messages with from_manager.recv()
```

Optionally, manager settings can be configured using the `LgTvManagerBuilder`.

### Connecting

Connect to a TV by sending a `ManagerMessage::Connect` message to the manager. Connections can be
made to a TV by host, or by discovered UPnP device.

Connection settings must be specified. Use `ConnectionSettings::default()` or the
`ConnectionSettingsBuilder`.

```rust
use lgtv_manager::{
    Connection, ConnectionSettingsBuilder, LgTvManager, ManagerMessage::{Connect}
};
use tokio::sync::mpsc;

let (to_manager, to_manager_rx) = mpsc::channel(32);

// <Instantiate and run the manager first>

to_manager
    .send(Connect(Connection::Host(
        "10.0.0.101".into(),
        ConnectionSettingsBuilder::new().with_forced_pairing().build(),
    )))
    .await;
```

When using `Connection::Host`, a best effort is made to generate a valid LG TV WebSocket
server URL from the provided host string. For example, `tv.local` will be converted to
`wss://tv.local:3001/`. This behavior can be overridden by passing a fully-qualified URL
as the host, such as `ws://10.0.1.101:3000/`, in which case the host will be used unaltered.

When using `Connection::Device`, the WebSocket server URL is generated using the UPnP
device url. This assumes the `wss://` scheme on port `3001`.

After a successful connection, the manager will emit `LastSeenTv`, `TvInfo`, `TvInput`,
`TvSoftwareInfo`, and `TvState` details.

#### Pairing and client keys

Successfully connecting to a TV requires accepting a pair request, during which the TV will
prompt for input and the manager will be in the `ManagerStatus::Pairing` state. If the pair
request is accepted then a unique client key is generated by the TV. `LgTvManager` will persist
this client key to local storage and will use it automatically for future sessions. Pairing can be
forced when sending `ManagerMessage::Connect`.

#### UPnP discovery

UPnP discovery can be used to find LG TVs on the local network by sending a
`ManagerMessage::Discover` message to the manager. The manager will then send messages back to
the caller, providing discovery status information with `ManagerOutputMessage::IsDiscovering` and
a vector of discovered devices with `ManagerOutputMessage::DiscoveredDevices`.

#### Connection flow

`LgTvManager` automatically transitions through a number of states while establishing a connection
to a TV. The manager will return a `ManagerOutputMessage::Status` message for each updated
state. The flow is as follows:

1. `Disconnected`
2. `Connecting`
3. `Connected`
4. `Pairing`
5. `Initializing`
6. `Communicating`
6. `Disconnecting`

Most states can be safely ignored. It is enough to instantiate `LgTvManager`, send a
`ManagerMessage::Connect` message, and then wait for the manager to enter the `Communicating`
state before sending commands.

The WebSocket URL is provided as String data with most states (where applicable).

See `LgTvManager` for a diagram of the state flow.

### Sending LG commands

Once a successful connection has been established and the manager has returned
`ManagerOutputMessage::Status(Communicating)`, arbitrary commands can be sent to the TV.

Commands are sent using `ManagerMessage::SendTvCommand`. Supported commands can be seen in
`TvCommand`.

It is expected that the most common commands to send to the TV will be those that change the TV's
state (such as `SetMute`, `VolumeUp`, etc). Any changes to the TV's state will be received via
`TvState` updates from the manager, so invoking the "get" commands is usually not necessary.

### Disconnecting

Use `ManagerMessage::Disconnect` to disconnect from a TV. The manager will continue to run after
a disconnect, and can still accept future `ManagerMessage::Connect` messages. Send
`ManagerMessage::ShutDown` to instruct the manager to disconnect and exit.

### Limitations

The `lgtv_manager` crate is currently limited in scope. It only supports the `TvCommand` list
found in `src/commands.rs`, and does not provide much in the way of configuration (such as timeout
durations and automatic reconnects).

Extending `LgTvManager` to support additional TV commands should be fairly trivial, although
LG's SSAP protocol does not appear to be documented. A good place to start is the
[LGWebOSRemote](https://github.com/klattimer/LGWebOSRemote) project, which was a source of
inspiration for `lgtv_manager`.

### Examples

(Note: These examples rely on the third-party crates `env_logger` and `tokio`).

To run the examples:

```sh
cargo run --example control
cargo run --example discover
```

The `control` example creates an `LgTvManager` instance, sends commands to the manager via the
console, and prints all messages received from the manager. **This example likely won't work
without updating the TV IP address**.

```rust
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

#### UPnP device discovery

The `discover` example performs LG TV UPnP device discovery. Discovery returns a vector of
`LgTvDevice` instances, which can be passed to the manager using `ManagerMessage::Connect`.

```rust
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
