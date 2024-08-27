//! Manages a WebSocket connection to an LG TV.
//!
//! Does not have any awareness of the LG protocol. This WebSocket client manager only understands
//! sending and receiving Strings. See [`crate::lgtv_manager::messages`] for the LG-specific payload
//! descriptions.
//!
//! The caller can send [`WsMessage`] channel messages to instruct the WebSocket manager to send a
//! `String` payload or shut down command. The WebSocket manager will send [`WsUpdateMessage`]
//! channel messages back to the caller when a `String` payload is received, or when there is a
//! WebSocket status change (see [`WsStatus`]).

use std::time::SystemTime;

use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::task;
use tokio::task::JoinHandle;
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{
    connect_async_tls_with_config, Connector, MaybeTlsStream, WebSocketStream,
};
use tungstenite::{Error, Message};

// CHANNEL MESSAGES -------------------------------------------------------------------------------

/// Channel message types sent from the caller to the WebSocket manager.
///
/// [`WsMessage::Command`] - Send a [`WsCommand`] to the WebSocket manager.
/// [`WsMessage::Payload`] - Send a `String` payload to the TV's WebSocket.
/// [`WsMessage::TestConnection`] - Test whether the WebSocket server responds to pings.
#[derive(Debug)]
pub(crate) enum WsMessage {
    Command(WsCommand),
    Payload(String),
    TestConnection,
}

/// Channel message types sent from the WebSocket manager back to the caller.
///
/// [`WsUpdateMessage::Status`] - Send WebSocket status information ([`WsStatus`]) back to the
/// caller.
/// [`WsUpdateMessage::Payload`] - Send `String` payloads received from the WebSocket server back
/// to the caller.
/// [`WsUpdateMessage::IsConnectionOk`] - Send a connection test result to the caller.
#[derive(Debug)]
pub(crate) enum WsUpdateMessage {
    Status(WsStatus),
    Payload(String),
    IsConnectionOk(bool),
}

// ------------------------------------------------------------------------------------------------

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);
const TEST_CONNECTION_DURATION: Duration = Duration::from_millis(1500);

#[derive(Debug)]
pub(crate) enum WsCommand {
    ShutDown,
}

#[derive(Debug)]
pub(crate) enum WsStatus {
    Idle,
    Connected,
    Disconnected,
    ConnectError(String),
    MessageReadError(String),
    ServerClosedConnection,
}

// ================================================================================================
// LgTvWebSocket

pub(crate) struct LgTvWebSocket {}

impl LgTvWebSocket {
    pub fn new() -> Self {
        LgTvWebSocket {}
    }

    /// Start the `LgTvWebSocket` handler.
    ///
    /// Spawns a task which:
    ///
    /// * Connects to the TV at the given `host`.
    /// * Loops forever, waiting for:
    ///     * `WsMessage::Command` and `WsMessage::Payload` messages from the caller over the
    ///       provided `rx` channel.
    ///     * Incoming String payloads from the TV's WebSocket server, to pass back to the caller
    ///       in a `WsUpdateMessage::Payload` message over the provided `tx` channel.
    ///
    /// Non-payload information is passed back to the caller via `WsUpdateMessage::Status`
    /// messages.
    pub fn start(
        &mut self,
        host: &str,
        use_tls: bool,
        mut rx: Receiver<WsMessage>,
        tx: Sender<WsUpdateMessage>,
    ) -> JoinHandle<()> {
        let host = host.to_owned();

        task::spawn(async move {
            if LgTvWebSocket::send_update(&tx, WsUpdateMessage::Status(WsStatus::Idle))
                .await
                .is_err()
            {
                return;
            }

            // Connect
            let ws_stream = match LgTvWebSocket::connect(&host, use_tls).await {
                Ok(ws_stream) => ws_stream,
                Err(e) => {
                    error!("{e}");
                    let _ = LgTvWebSocket::send_update(
                        &tx,
                        WsUpdateMessage::Status(WsStatus::ConnectError(e)),
                    )
                    .await;

                    return;
                }
            };

            if LgTvWebSocket::send_update(&tx, WsUpdateMessage::Status(WsStatus::Connected))
                .await
                .is_err()
            {
                return;
            }

            let (mut ws_write, mut ws_read) = ws_stream.split();

            // Configure an interval which will always be checked regardless of whether there's any
            // items waiting in a channel for processing.
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            let mut connection_test_start: Option<SystemTime> = None;

            // Loop forever, reading messages from the caller and the WebSocket.

            loop {
                select! {
                    // Incoming message from the caller (LgTvManager)
                    caller_optional = rx.recv() => {
                        match caller_optional {
                            Some(caller_message) => match caller_message {
                                WsMessage::Command(command) => {
                                    match command {
                                        WsCommand::ShutDown => {
                                            warn!("WebSocket shut down request received");

                                            match ws_write.send(Message::Close(None)).await {
                                                Ok(_) => {
                                                    info!("WebSocket connection closed");
                                                },
                                                Err(e) => {
                                                    error!("Failed to send WebSocket close message: {:?}", e);
                                                }
                                            }

                                            break;
                                        }
                                    }
                                }
                                WsMessage::Payload(payload) => {
                                    if let Err(e) = ws_write.send(payload.clone().into()).await {
                                        // A failed WebSocket send is considered fatal
                                        error!("Failed to send WebSocket payload: {:?}", e);

                                        break;
                                    }
                                }
                                WsMessage::TestConnection => {
                                    if connection_test_start.is_some() {
                                        warn!("Connection test already in progress");
                                    } else {
                                        info!("Initiating connection test");

                                        if let Err(e) = ws_write.send(Message::Ping(vec!())).await {
                                            error!("Failed to send WebSocket ping while testing connection: {:?}", e);
                                            break;
                                        }

                                        connection_test_start = Some(SystemTime::now());
                                    }
                                }
                            },
                            None => {
                                warn!("WebSocket caller receive channel closed");

                                match ws_write.send(Message::Close(None)).await {
                                    Ok(_) => {
                                        info!("WebSocket connection closed");
                                    },
                                    Err(e) => {
                                        error!("Failed to send WebSocket close message: {:?}", e);
                                    }
                                }

                                break;
                            },
                        }
                    },

                    // Incoming message from the WebSocket connection
                    Some(ws_message) = ws_read.next() => {
                        match ws_message {
                            Ok(message) => match message {
                                Message::Text(payload) => {
                                    if LgTvWebSocket::send_update(&tx, WsUpdateMessage::Payload(payload)).await.is_err() {
                                        break;
                                    }
                                }
                                Message::Ping(_) => {
                                    debug!("WebSocket Ping");
                                }
                                Message::Pong(_) => {
                                    debug!("WebSocket Pong");

                                    // A pong received while testing the connection means the
                                    // connection test passed
                                    if connection_test_start.is_some() {
                                        connection_test_start = None;

                                        let _ = LgTvWebSocket::send_update(
                                            &tx,
                                            WsUpdateMessage::IsConnectionOk(true)
                                        ).await;
                                    }
                                }
                                Message::Close(_) => {
                                    warn!("WebSocket received server close");
                                    let _ = LgTvWebSocket::send_update(
                                        &tx,
                                        WsUpdateMessage::Status(WsStatus::ServerClosedConnection)
                                    ).await;

                                    break;
                                }
                                unhandled => {
                                    warn!("WebSocket received unhandled message: {:?}", unhandled);
                                }
                            }
                            Err(e) => {
                                error!("WebSocket message read error: {:?}", &e);
                                let _ = LgTvWebSocket::send_update(
                                    &tx,
                                    WsUpdateMessage::Status(WsStatus::MessageReadError(format!("{e}")))
                                ).await;

                                break;
                            }
                        }
                    }

                    // Do some checks every interval, regardless of incoming messages.
                    _ = interval.tick() => {
                        match connection_test_start {
                            Some(connection_test_start) => {
                                // A connection test is in progress. If a pong has not been received
                                // in time then the test has failed.
                                let now = SystemTime::now();

                                if let Ok(duration_since_test_start) = now.duration_since(connection_test_start) {
                                    if duration_since_test_start > TEST_CONNECTION_DURATION {
                                        warn!("Connection test failed (pong timeout)");

                                        let _ = LgTvWebSocket::send_update(
                                            &tx,
                                            WsUpdateMessage::IsConnectionOk(false)
                                        ).await;
                                    }
                                }
                            },
                            None => {},
                        }
                    }
                }
            }

            // The loop has been broken out of, likely due to the WebSocket server connection being
            // lost, or the caller sending a `ShutDown` message.
            let _ =
                LgTvWebSocket::send_update(&tx, WsUpdateMessage::Status(WsStatus::Disconnected))
                    .await;

            info!("LgTvWebSocket has successfully shut down");
        })
    }

    /// Connect to TV WebSocket (with timeout).
    async fn connect(
        host: &str,
        use_tls: bool,
    ) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, String> {
        let url = match url::Url::parse(host) {
            Ok(url) => url,
            Err(e) => return Err(format!("Could not parse host '{}': {:?}", &host, e)),
        };

        info!("Attempting to connect to TV WebSocket at: {}", &url);

        let tls_connector = match native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .danger_accept_invalid_hostnames(true)
            .build()
        {
            Ok(tls_connector) => tls_connector,
            Err(e) => return Err(format!("Could not create TLS connector: {:?}", e)),
        };

        let connect_future = connect_async_tls_with_config(
            url,
            None,
            false,
            if use_tls {
                Some(Connector::NativeTls(tls_connector))
            } else {
                Some(Connector::Plain)
            },
        );

        let (ws_stream, _) = match timeout(CONNECTION_TIMEOUT, connect_future).await {
            Ok(Ok(conn_result)) => conn_result,
            Ok(Err(e)) => {
                return match e {
                    Error::Io(e) => {
                        let error_string = e.to_string();

                        return if error_string.contains("Host is down") {
                            Err("Failed to connect to TV: Host is down".into())
                        } else {
                            Err(format!("Failed to connect to TV: {error_string}"))
                        };
                    }
                    _ => Err(format!("Failed to connect to TV: {:?}", e)),
                }
            }
            Err(_) => return Err("Failed to connect to TV: Connection timeout".into()),
        };

        info!("WebSocket handshake has been successfully completed");

        Ok(ws_stream)
    }

    /// Send a `WsUpdateMessage` back to the caller.
    async fn send_update(
        sender: &Sender<WsUpdateMessage>,
        message: WsUpdateMessage,
    ) -> Result<(), ()> {
        sender.send(message).await.map_err(|_| {
            warn!("WebSocket handler update channel closed");
        })
    }
}
