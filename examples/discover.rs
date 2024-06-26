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
