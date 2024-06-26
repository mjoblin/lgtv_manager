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
    /// Attempting to pair with the TV (TV is prompting for pairing confirmation).
    Pairing,
    /// Initializing the paired connection to the TV (subscribing to TV updates).
    Initializing,
    /// Able to send [`TvCommand`] messages to the TV.
    Communicating,
    /// Disconnecting from the TV.
    Disconnecting,
    /// An unrecoverable problem has occurred. The Manager is unresponsive and will only respond
    /// (at best) to `ManagerMessage::ShutDown` requests.
    // Cannot be transitioned into or out of. This state exists only so the LgTvManager can inform
    // the caller that it is unresponsive and should be shut down.
    Zombie,
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
    HandleConnectionError,
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
//         Error => Disconnected [HandleConnectionError],
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
//         Error => Disconnected [HandleConnectionError],
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
            (State::Connected(_), Input::Pair) => Some(State::Pairing),
            (State::Connected(_), Input::Initialize) => Some(State::Initializing),
            (State::Connected(_), Input::Reset) => Some(State::Disconnected),
            (State::Connected(_), Input::Error) => Some(State::Disconnecting),

            // Pairing
            (State::Pairing, Input::Initialize) => Some(State::Initializing),
            (State::Pairing, Input::Reset) => Some(State::Disconnected),
            (State::Pairing, Input::Error) => Some(State::Disconnecting),

            // Initializing
            (State::Initializing, Input::StartCommunication) => Some(State::Communicating),
            (State::Initializing, Input::Reset) => Some(State::Disconnected),
            (State::Initializing, Input::Error) => Some(State::Disconnecting),

            // Communicating
            (State::Communicating, Input::SendCommand(_)) => Some(State::Communicating),
            (State::Communicating, Input::Disconnect) => Some(State::Disconnecting),
            (State::Communicating, Input::Reset) => Some(State::Disconnected),
            (State::Communicating, Input::Error) => Some(State::Disconnecting),

            // Disconnecting
            (State::Disconnecting, Input::DisconnectionComplete) => Some(State::Disconnected),
            (State::Disconnecting, Input::Reset) => Some(State::Disconnected),
            (State::Disconnecting, Input::Error) => Some(State::Disconnected),

            _ => None,
        }
    }

    fn output(state: &Self::State, input: &Self::Input) -> Option<Self::Output> {
        match (state, input) {
            // Disconnected
            (State::Disconnected, Input::Connect(host)) => Some(Output::ConnectToTv(host.clone())),

            // Connecting
            (State::Connecting(_), Input::AttemptRegister) => Some(Output::SendRegisterPayload),
            (State::Connecting(_), Input::Error) => Some(Output::HandleConnectionError),

            // Connected
            (State::Connected(_), Input::Pair) => Some(Output::PairWithTv),
            (State::Connected(_), Input::Initialize) => Some(Output::InitializeConnection),
            (State::Connected(_), Input::Error) => Some(Output::DisconnectFromTv),

            // Pairing
            (State::Pairing, Input::Initialize) => Some(Output::InitializeConnection),
            (State::Pairing, Input::Error) => Some(Output::DisconnectFromTv),

            // Initializing
            (State::Initializing, Input::Error) => Some(Output::DisconnectFromTv),

            // Communicating
            (State::Communicating, Input::SendCommand(command)) => {
                Some(Output::SendCommand(command.clone()))
            }
            (State::Communicating, Input::Disconnect) => Some(Output::DisconnectFromTv),
            (State::Communicating, Input::Error) => Some(Output::DisconnectFromTv),

            // Disconnecting
            (State::Disconnecting, Input::DisconnectionComplete) => {
                Some(Output::HandleSuccessfulDisconnect)
            }
            (State::Disconnecting, Input::Error) => Some(Output::HandleConnectionError),

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
