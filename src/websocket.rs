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
use tungstenite::Message;

// CHANNEL MESSAGES -------------------------------------------------------------------------------

/// Channel message types sent from the caller to the WebSocket manager.
///
/// The [`WsMessage::Command`] variant is used to send a [`WsCommand`] to the WebSocket manager.
/// The [`WsMessage::Payload`] variant is used to send a `String` payload to the TV's WebSocket
/// server.
#[derive(Debug)]
pub(crate) enum WsMessage {
    Command(WsCommand),
    Payload(String),
}

/// Channel message types sent from the WebSocket manager back to the caller.
///
/// The [`WsUpdateMessage::Status`] variant is used to send WebSocket status information
/// ([`WsStatus`]) back to the caller. The [`WsUpdateMessage::Payload`] variant is used to send
/// `String` payloads received from the WebSocket server back to the caller.
#[derive(Debug)]
pub(crate) enum WsUpdateMessage {
    Status(WsStatus),
    Payload(String),
}

// ------------------------------------------------------------------------------------------------

const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) enum WsCommand {
    ShutDown,
}

#[derive(Debug)]
pub(crate) enum WsStatus {
    Idle,
    Connected,
    Disconnected,
    Error(String),
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
        &self,
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
                        WsUpdateMessage::Status(WsStatus::Error(e)),
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
                                }
                                Message::Close(_) => {
                                    warn!("WebSocket received server close");
                                    let _ = LgTvWebSocket::send_update(
                                        &tx,
                                        WsUpdateMessage::Status(WsStatus::Error("Server closed connection".into()))
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
                                    WsUpdateMessage::Status(WsStatus::Error(format!("{e}")))
                                ).await;

                                break;
                            }
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
            Ok(Err(e)) => return Err(format!("Failed to connect to TV: {:?}", e)),
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
