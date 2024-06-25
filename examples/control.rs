use env_logger;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use lgtv_manager::LgTvManagerMessage::{Connect, Disconnect, SendTvCommand, ShutDown};
use lgtv_manager::ManagerStatus::Disconnected;
use lgtv_manager::{
    Connection, ConnectionSettings, LgTvManager, LgTvManagerOutputMessage,
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

                if LgTvManagerOutputMessage::Status(Disconnected) == manager_output_msg {
                    println!("\n>>> Manager is disconnected and ready to receive messages.");
                    println!(
                        ">>> SEND CONNECT ('c') COMMAND FIRST; NETWORK IP NEEDS TO BE VALID <<<"
                    );
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

    // Run the manager until instructed to shut down (LgTvManagerMessage::ShutDown)
    manager.run().await;

    stdin_handle.await.map_err(|_| ())?
}
