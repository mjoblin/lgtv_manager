//! Defines the state machine for a connection to an LG TV.
//!
//! Broadly, the state machine flow is as follows:
//!
//! 1. Connect to the TV.
//! 2. Send a registration payload (including a client key if available).
//!      - If the client key is `null` or otherwise invalid, then the TV will prompt the user to
//!        pair.
//! 3. Initialize the connection (i.e. subscribe to volume/mute updates).
//! 4. Start adhoc client-driven communication.
//!      - We expect to stay here for the duration of the app, sending commands to the TV when
//!        requested.
//! 5. Disconnect from the TV.

use std::fmt;

use log::{debug, error, info, warn};
use rust_fsm::*;
use tokio::select;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::TvCommand;

// CHANNEL MESSAGES -------------------------------------------------------------------------------

/// Message types sent from the state machine task back to the caller.
#[derive(Debug)]
pub(crate) enum StateMachineUpdateMessage {
    State(State),
    Output(Output),
    TransitionError(String),
}

// ------------------------------------------------------------------------------------------------
// States, Inputs, Outputs

/// Manager status.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// Not currently connected to the TV.
    Disconnected,
    /// Attempting to connect to the TV over a WebSocket at the provided url.
    Connecting(String),
    /// WebSocket connection to the TV has been established at the provided url.
    Connected(String),
    /// Attempting to pair with the TV at the provided url (TV is prompting for pairing confirmation).
    Pairing(String),
    /// Initializing the paired connection to the TV at the provided url (subscribing to TV updates).
    Initializing(String),
    /// Able to send [`TvCommand`] messages to the TV at the provided url.
    Communicating(String),
    /// Disconnecting from the TV at the provided url.
    Disconnecting(String),
    /// Attempting a reconnect to the provided url in the given number of seconds.
    // This is a faked state of sorts. The state machine never technically enters this state, but
    // the LgTvManager manages the reconnecting state manually and wants to inform callers about
    // the reconnect status. This is hacky.
    // TODO: Can the reconnect state be formally handled by the state machine in a clean manner.
    Reconnecting(String, u64),
    /// An unrecoverable problem has occurred. The Manager is unresponsive and will only respond
    /// (at best) to `ManagerMessage::ShutDown` requests.
    // Cannot be transitioned into or out of. This state exists only so the LgTvManager can inform
    // the caller that it is unresponsive and should be shut down.
    Zombie,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            State::Connecting(val) => write!(f, "Connecting({})", val),
            State::Connected(val) => write!(f, "Connected({})", val),
            State::Pairing(val) => write!(f, "Pairing({})", val),
            State::Initializing(val) => write!(f, "Initializing({})", val),
            State::Communicating(val) => write!(f, "Communicating({})", val),
            State::Disconnecting(val) => write!(f, "Disconnecting({})", val),
            variant => write!(f, "{:?}", variant),
        }
    }
}

/// State machine transition inputs.
#[derive(Debug, Clone)]
pub(crate) enum Input {
    Connect(String),
    AttemptRegister,
    Pair,
    Initialize,
    StartCommunication,
    SendCommand(TvCommand),
    Reset,
    Error,
    Disconnect,
    DisconnectionComplete,
}

/// State machine transition outputs.
#[derive(Debug)]
pub(crate) enum Output {
    ConnectToTv(String),
    SendRegisterPayload,
    PairWithTv,
    InitializeConnection,
    SendCommand(TvCommand),
    DisconnectFromTv,
    HandleConnectError,
    HandleDisconnectError,
    HandleSuccessfulDisconnect,
}

// Following is the state machine definition using the DSL, which more succinctly shows how it's
// configured. Unfortunately the DSL approach doesn't support inputs which take data (looking at
// you, Input::Connect(host) and Input::SendCommand(command). Leaving this DSL definition here as
// documentation, but the actual state machine implementation is defined below (see
// StateMachineImpl for LgTvStateMachine).
//
// state_machine! {
//     #[derive(Debug)]
//     #[state_machine(input(crate::Input), state(crate::State), output(crate::Output))]
//     pub LgTvStateMachine(Disconnected)
//
//     Disconnected => {
//         Connect => Connecting [ConnectToTv],
//         BecomeZombie => Zombie [HandleBecomingAZombie],
//     }
//
//     Connecting => {
//         AttemptRegister => Connected [SendRegisterPayload],
//         Error => Disconnected [HandleConnectError],
//         BecomeZombie => Zombie [HandleBecomingAZombie],
//     }
//
//     Connected => {
//         Pair => Pairing [PairWithTv],
//         Initialize => Initializing [InitializeConnection],
//         Error => Disconnecting [DisconnectFromTv],
//         BecomeZombie => Zombie [HandleBecomingAZombie],
//     },
//
//     Pairing => {
//         Initialize => Initializing [InitializeConnection],
//         Error => Disconnecting [DisconnectFromTv],
//         BecomeZombie => Zombie [HandleBecomingAZombie],
//     },
//
//     Initializing => {
//         StartCommunication => Communicating,
//         Error => Disconnecting [DisconnectFromTv],
//         BecomeZombie => Zombie [HandleBecomingAZombie],
//     },
//
//     Communicating => {
//         SendCommand => Communicating [SendCommand],
//         Disconnect => Disconnecting [DisconnectFromTv],
//         Error => Disconnecting [DisconnectFromTv],
//         BecomeZombie => Zombie [HandleBecomingAZombie],
//     },
//
//     Disconnecting => {
//         DisconnectionComplete => Disconnected [HandleSuccessfulDisconnect],
//         Error => Disconnected [HandleDisconnectError],
//         BecomeZombie => Zombie [HandleBecomingAZombie],
//     },
// }

// Mermaid format:
//
// (Note: This omits the Zombie state for brevity).
//
// ---
// title: LgTvManager state machine
// ---
// stateDiagram-v2
// [*] --> Disconnected
// Disconnected --> Connecting: Connect
//
// Connecting --> Connected: AttemptRegister
// Connecting --> Disconnected: Error
//
// Connected --> Pairing: Pair
// Connected --> Initializing: Initialize
// Connected --> Disconnecting: Error
//
// Pairing --> Initializing: Initialize
// Pairing --> Disconnecting: Error
//
// Initializing --> Communicating: StartCommunication
// Initializing --> Disconnecting: Error
//
// Communicating --> Communicating: SendCommand
// Communicating --> Disconnecting: Disconnect
// Communicating --> Disconnecting: Error
//
// Disconnecting --> Disconnected: DisconnectionComplete
// Disconnecting --> Disconnected: Error

// ================================================================================================
// LgTvStateMachine

#[derive(Debug)]
pub(crate) struct LgTvStateMachine;

impl StateMachineImpl for LgTvStateMachine {
    type Input = Input;
    type State = State;
    type Output = Output;

    const INITIAL_STATE: Self::State = State::Disconnected;

    fn transition(state: &Self::State, input: &Self::Input) -> Option<Self::State> {
        match (state, input) {
            // Disconnected
            (State::Disconnected, Input::Connect(url)) => Some(State::Connecting(url.into())),
            (State::Disconnected, Input::Reset) => Some(State::Disconnected),

            // Connecting
            (State::Connecting(url), Input::AttemptRegister) => Some(State::Connected(url.into())),
            (State::Connecting(_), Input::Reset) => Some(State::Disconnected),
            (State::Connecting(_), Input::Error) => Some(State::Disconnected),

            // Connected
            (State::Connected(url), Input::Pair) => Some(State::Pairing(url.into())),
            (State::Connected(url), Input::Initialize) => Some(State::Initializing(url.into())),
            (State::Connected(_), Input::Reset) => Some(State::Disconnected),
            (State::Connected(url), Input::Error) => Some(State::Disconnecting(url.into())),

            // Pairing
            (State::Pairing(url), Input::Initialize) => Some(State::Initializing(url.into())),
            (State::Pairing(_), Input::Reset) => Some(State::Disconnected),
            (State::Pairing(url), Input::Error) => Some(State::Disconnecting(url.into())),

            // Initializing
            (State::Initializing(url), Input::StartCommunication) => {
                Some(State::Communicating(url.into()))
            }
            (State::Initializing(_), Input::Reset) => Some(State::Disconnected),
            (State::Initializing(url), Input::Error) => Some(State::Disconnecting(url.into())),

            // Communicating
            (State::Communicating(url), Input::SendCommand(_)) => {
                Some(State::Communicating(url.into()))
            }
            (State::Communicating(url), Input::Disconnect) => {
                Some(State::Disconnecting(url.into()))
            }
            (State::Communicating(_), Input::Reset) => Some(State::Disconnected),
            (State::Communicating(url), Input::Error) => Some(State::Disconnecting(url.into())),

            // Disconnecting
            (State::Disconnecting(_), Input::DisconnectionComplete) => Some(State::Disconnected),
            (State::Disconnecting(_), Input::Reset) => Some(State::Disconnected),
            (State::Disconnecting(_), Input::Error) => Some(State::Disconnected),

            _ => None,
        }
    }

    fn output(state: &Self::State, input: &Self::Input) -> Option<Self::Output> {
        match (state, input) {
            // Disconnected
            (State::Disconnected, Input::Connect(host)) => Some(Output::ConnectToTv(host.clone())),

            // Connecting
            (State::Connecting(_), Input::AttemptRegister) => Some(Output::SendRegisterPayload),
            (State::Connecting(_), Input::Error) => Some(Output::HandleConnectError),

            // Connected
            (State::Connected(_), Input::Pair) => Some(Output::PairWithTv),
            (State::Connected(_), Input::Initialize) => Some(Output::InitializeConnection),
            (State::Connected(_), Input::Error) => Some(Output::DisconnectFromTv),

            // Pairing
            (State::Pairing(_), Input::Initialize) => Some(Output::InitializeConnection),
            (State::Pairing(_), Input::Error) => Some(Output::DisconnectFromTv),

            // Initializing
            (State::Initializing(_), Input::Error) => Some(Output::DisconnectFromTv),

            // Communicating
            (State::Communicating(_), Input::SendCommand(command)) => {
                Some(Output::SendCommand(command.clone()))
            }
            (State::Communicating(_), Input::Disconnect) => Some(Output::DisconnectFromTv),
            (State::Communicating(_), Input::Error) => Some(Output::DisconnectFromTv),

            // Disconnecting
            (State::Disconnecting(_), Input::DisconnectionComplete) => {
                Some(Output::HandleSuccessfulDisconnect)
            }
            (State::Disconnecting(_), Input::Error) => Some(Output::HandleDisconnectError),

            _ => None,
        }
    }
}

pub(crate) fn start_state_machine(
    task_tracker: &TaskTracker,
    mut msg_rx: Receiver<Input>,
    msg_tx: Sender<StateMachineUpdateMessage>,
    cancel_token: CancellationToken,
) {
    debug!("Starting state machine");

    task_tracker.spawn(async move {
        let mut fsm: StateMachine<LgTvStateMachine> = StateMachine::new();

        // Loop forever, reading Input messages from the caller. Each Input is consumed,
        // potentially resulting in a transition to a new State -- and sometimes an Output. The
        // State and Output details are sent back to the caller to act on.

        loop {
            select! {
                Some(input) = msg_rx.recv() => {
                    let entry_state = fsm.state().clone();

                    match fsm.consume(&input) {
                        Ok(fsm_output) => {
                            let exit_state = fsm.state().clone();

                            debug!(
                                "FSM acting on input [{:?}]: {:?} -> {:?}, with output [{}]",
                                &input, entry_state, exit_state,
                                match &fsm_output {
                                    Some(o) => format!("{:?}", o),
                                    None => "none".into()
                                }
                            );

                            // Inform the LgTvManager of the new state
                            if send_update(
                                &msg_tx,
                                StateMachineUpdateMessage::State(fsm.state().clone()),
                            ).await.is_err() {
                                break;
                            }

                            if let Some(output) = fsm_output {
                                // Let the LgTvManager handle the output action
                                if send_update(
                                    &msg_tx,
                                    StateMachineUpdateMessage::Output(output),
                                ).await.is_err() {
                                    break;
                                }
                            }
                        },
                        Err(e) => {
                            let msg = format!(
                                "FSM transition error with Input [{:?}] while in State '{:?}': {:?}",
                                &input, &fsm.state(), e
                            );
                            error!("{}", &msg);

                            if send_update(
                                &msg_tx,
                                StateMachineUpdateMessage::TransitionError(msg),
                            ).await.is_err() {
                                break;
                            }
                        }
                    }
                }

                _ = cancel_token.cancelled() => {
                    info!("State machine shut down request received");

                    break;
                }
            }
        }

        info!("State machine successfully shut down");
    });
}

/// Send a `StateMachineUpdateMessage` back to the caller.
async fn send_update(
    sender: &Sender<StateMachineUpdateMessage>,
    message: StateMachineUpdateMessage,
) -> Result<(), ()> {
    sender.send(message).await.map_err(|_| {
        warn!("State machine update channel closed");
    })
}

// ================================================================================================
// Tests

#[cfg(test)]
mod tests {
    use crate::TvCommand;

    use super::State;

    #[test]
    fn state_display() {
        assert_eq!(TvCommand::GetCurrentSWInfo.to_string(), "GetCurrentSWInfo");

        assert_eq!(State::Disconnected.to_string(), "Disconnected");
        assert_eq!(
            State::Connecting("wss://127.0.0.1:3001/".into()).to_string(),
            "Connecting(wss://127.0.0.1:3001/)"
        );
        assert_eq!(
            State::Connected("wss://127.0.0.1:3001/".into()).to_string(),
            "Connected(wss://127.0.0.1:3001/)"
        );
        assert_eq!(
            State::Pairing("wss://127.0.0.1:3001/".into()).to_string(),
            "Pairing(wss://127.0.0.1:3001/)"
        );
        assert_eq!(
            State::Initializing("wss://127.0.0.1:3001/".into()).to_string(),
            "Initializing(wss://127.0.0.1:3001/)"
        );
        assert_eq!(
            State::Communicating("wss://127.0.0.1:3001/".into()).to_string(),
            "Communicating(wss://127.0.0.1:3001/)"
        );
        assert_eq!(
            State::Disconnecting("wss://127.0.0.1:3001/".into()).to_string(),
            "Disconnecting(wss://127.0.0.1:3001/)"
        );
        assert_eq!(State::Zombie.to_string(), "Zombie");
    }
}
