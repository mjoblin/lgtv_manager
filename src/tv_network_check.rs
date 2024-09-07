//! Check whether the TV is visible on the network.
//!
//! This is effectively a network ping check for a TV, and does not use or rely on the SSAP
//! WebSocket layer.

use std::io;
use std::net::IpAddr;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::{debug, error, info, warn};
use rand::random;
use surge_ping::{Client, Config, PingIdentifier, PingSequence};
use tokio::{
    select,
    sync::{Mutex, Notify},
    time::Duration,
};

const ONE_MINUTE_SECS: u64 = 60;

pub(crate) struct TvNetworkChecker {
    pub is_tv_on_network: Arc<AtomicBool>,

    check: Arc<Notify>,
    is_tv_on_network_notify: Arc<Notify>,
    tv_ip: Arc<Mutex<Option<IpAddr>>>,
    tv_ip_notify: Arc<Notify>,
}

impl TvNetworkChecker {
    pub fn new() -> Self {
        TvNetworkChecker {
            is_tv_on_network: Arc::new(AtomicBool::new(false)),

            check: Arc::new(Notify::new()),
            is_tv_on_network_notify: Arc::new(Notify::new()),
            tv_ip: Arc::new(Mutex::new(None)),
            tv_ip_notify: Arc::new(Notify::new()),
        }
    }

    /// Start the network checker task. The task will run forever, checking the provided IP address
    /// at regular intervals and immediately on request. The IP address can be changed while the
    /// checker task is running.
    pub fn start(
        &mut self,
    ) -> Result<(Arc<AtomicBool>, Arc<Notify>, Arc<AtomicBool>, Arc<Notify>), io::Error> {
        let is_tv_on_network = Arc::clone(&self.is_tv_on_network);
        let is_tv_on_network_notify = Arc::clone(&self.is_tv_on_network_notify);
        let tv_ip = Arc::clone(&self.tv_ip);
        let tv_ip_notify = Arc::clone(&self.tv_ip_notify);
        let check = Arc::clone(&self.check);

        let is_pinging = Arc::new(AtomicBool::new(false));
        let is_pinging_clone = Arc::clone(&is_pinging);
        let is_pinging_notify = Arc::new(Notify::new());
        let is_pinging_notify_clone = Arc::clone(&is_pinging_notify);

        let ping_client = Client::new(&Config::default())?;

        tokio::spawn(async move {
            info!("Starting TV network checker");

            let mut ping_target_ip: Option<IpAddr> = None;
            let mut ping_interval = tokio::time::interval(Duration::from_secs(ONE_MINUTE_SECS));

            loop {
                select! {
                    // Ping the TV at regular intervals.
                    _ = ping_interval.tick() => {
                        if let Some(target_ip) = ping_target_ip {
                            TvNetworkChecker::check_tv(
                                &ping_client,
                                target_ip.clone(),
                                Arc::clone(&is_tv_on_network),
                                is_tv_on_network_notify.clone(),
                                is_pinging.clone(),
                                is_pinging_notify.clone(),
                            ).await;
                        }
                    }

                    // Ping the TV upon receipt of a TV IP address.
                    _ = tv_ip_notify.notified() => {
                        ping_target_ip = tv_ip.lock().await.clone();
                        check.notify_one();

                        debug!("TV IP address received: {:?}", &ping_target_ip);
                    }

                    // Ping the TV immediately on request.
                    _ = check.notified() => {
                        debug!("Immediate network ping check requested");

                        if let Some(target_ip) = ping_target_ip {
                            TvNetworkChecker::check_tv(
                                &ping_client,
                                target_ip.clone(),
                                Arc::clone(&is_tv_on_network),
                                is_tv_on_network_notify.clone(),
                                is_pinging.clone(),
                                is_pinging_notify.clone(),
                            ).await;
                        } else {
                            warn!("Ping check requested, but no TV IP address known");
                        }
                    }
                }
            }
        });

        Ok((
            self.is_tv_on_network.clone(),
            self.is_tv_on_network_notify.clone(),
            is_pinging_clone,
            is_pinging_notify_clone,
        ))
    }

    /// Set the IP address of the TV to check.
    pub async fn set_tv_ip(&mut self, tv_ip: IpAddr) {
        {
            let mut ip = self.tv_ip.lock().await;
            *ip = Some(tv_ip);
        }

        self.tv_ip_notify.notify_one();
        self.check();
    }

    /// Get the IP address currently used by the checker.
    pub async fn get_tv_ip(&self) -> Option<IpAddr> {
        let ip = self.tv_ip.lock().await;

        ip.clone()
    }

    /// Request an immediate ping check for the currently-tracked TV.
    pub fn check(&mut self) {
        self.check.notify_one();
    }

    // Private ------------------------------------------------------------------------------------

    /// Perform a ping of the currently-tracked TV. Only one ping is performed at a time.
    async fn check_tv(
        ping_client: &Client,
        tv_ip: IpAddr,
        is_tv_on_network: Arc<AtomicBool>,
        is_tv_on_network_notify: Arc<Notify>,
        is_pinging: Arc<AtomicBool>,
        is_pinging_notify: Arc<Notify>,
    ) {
        if is_pinging.load(Ordering::SeqCst) {
            warn!("Network ping check is already in progress; not starting new check");
            return;
        }

        is_pinging.store(true, Ordering::SeqCst);
        is_pinging_notify.notify_one();

        let ping_payload = [0; 8];
        let mut pinger = ping_client.pinger(tv_ip, PingIdentifier(random())).await;

        debug!("Initiating TV network ping test");

        match pinger.ping(PingSequence(0), &ping_payload).await {
            Ok(_) => {
                debug!("TV responded to network ping");
                is_tv_on_network.store(true, Ordering::SeqCst);
            }
            Err(e) => {
                info!("TV did not respond to network ping: {:?}", e);
                is_tv_on_network.store(false, Ordering::SeqCst);
            }
        }

        is_tv_on_network_notify.notify_one();

        is_pinging.store(false, Ordering::SeqCst);
        is_pinging_notify.notify_one();
    }
}
