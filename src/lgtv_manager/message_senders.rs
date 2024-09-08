use log::{error, warn};
use tokio::sync::mpsc::Sender;

use crate::state_machine::Input;
use crate::websocket_client::WsMessage;
use crate::ManagerOutputMessage;

use super::LgTvManager;

// ------------------------------------------------------------------------------------------------
// Send messages to the caller, State Machine, and WebSocket server.
// ------------------------------------------------------------------------------------------------

impl LgTvManager {
    /// Send a `ManagerOutputMessage` back to the caller.
    pub(crate) async fn send_out(&self, message: ManagerOutputMessage) -> Result<(), ()> {
        self.output_tx.send(message).await.map_err(|_| {
            warn!("Output channel unexpectedly closed");
        })
    }

    /// Send a `ManagerOutputMessage` back to the caller using the given `sender`.
    pub(crate) async fn send_out_with_sender(
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
    pub(crate) async fn send_to_fsm(&mut self, message: Input) -> Result<(), ()> {
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
    /// never occur, but if the manager can't send messages to the WebSocket manager then it
    /// attempts to reset itself.
    pub(crate) async fn send_to_ws(&mut self, message: WsMessage) -> Result<(), ()> {
        match &self.to_ws_tx {
            Some(to_ws_tx) => match to_ws_tx.send(message).await {
                Ok(_) => Ok(()),
                Err(_) => {
                    self.force_manager_reset(
                        "Cannot send message to WebSocket handler: channel unexpectedly closed",
                    )
                    .await;

                    Err(())
                }
            },
            // It's OK to not have a WebSocket sender available, especially between connects,
            // although in theory we shouldn't encounter this case in a normal/happy path
            None => Ok(()),
        }
    }
}
