use log::{error, info, warn};
use std::net::IpAddr;
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use surge_ping::{Client, Config, PingIdentifier};
use tokio::{
    select,
    sync::{Mutex, Notify},
    time::Duration,
};

pub(crate) struct TvNetworkChecker {
    pub is_tv_alive: Arc<AtomicBool>,

    check: Arc<Notify>,
    is_tv_alive_notify: Arc<Notify>,
    tv_ip: Arc<Mutex<Option<IpAddr>>>,
    tv_ip_notify: Arc<Notify>,
}

impl TvNetworkChecker {
    pub fn new() -> Self {
        TvNetworkChecker {
            is_tv_alive: Arc::new(AtomicBool::new(false)),

            check: Arc::new(Notify::new()),
            is_tv_alive_notify: Arc::new(Notify::new()),
            tv_ip: Arc::new(Mutex::new(None)),
            tv_ip_notify: Arc::new(Notify::new()),
        }
    }

    /// TODO: Document
    pub fn start(&mut self) -> (Arc<AtomicBool>, Arc<Notify>, Arc<AtomicBool>, Arc<Notify>) {
        let is_tv_alive = Arc::clone(&self.is_tv_alive);
        let is_tv_alive_notify = Arc::clone(&self.is_tv_alive_notify);
        let tv_ip = Arc::clone(&self.tv_ip);
        let tv_ip_notify = Arc::clone(&self.tv_ip_notify);
        let check = Arc::clone(&self.check);

        let is_pinging = Arc::new(AtomicBool::new(false));
        let is_pinging_clone = Arc::clone(&is_pinging);
        let is_pinging_notify = Arc::new(Notify::new());
        let is_pinging_notify_clone = Arc::clone(&is_pinging_notify);

        tokio::spawn(async move {
            info!("STARTING ALIVE CHECK");

            let mut ping_target_ip: Option<IpAddr> = None;

            // TODO: Check unwrap()
            let ping_client = Client::new(&Config::default()).unwrap();
            let mut ping_interval = tokio::time::interval(Duration::from_secs(60));

            loop {
                select! {
                    _ = ping_interval.tick() => {
                        info!("PING INTERVAL");

                        // Only ping the TV if we have an IP address and the TV is still thought to
                        // be offline.
                        if let Some(target_ip) = ping_target_ip {
                            // if !is_tv_alive.load(Ordering::SeqCst) {
                                TvNetworkChecker::check_tv(
                                    &ping_client,
                                    target_ip.clone(),
                                    Arc::clone(&is_tv_alive),
                                    is_tv_alive_notify.clone(),
                                    is_pinging.clone(),
                                    is_pinging_notify.clone(),
                                ).await;
                            // } else {
                            //     info!("TV THOUGHT TO BE ALIVE, NOT PINGING");
                            // }
                        }
                    }

                    _ = tv_ip_notify.notified() => {
                        ping_target_ip = tv_ip.lock().await.clone();
                        check.notify_one();

                        info!("PING IP ADDRESS RECEIVED: {:?}", &ping_target_ip);
                    }

                    _ = check.notified() => {
                        info!("PING CHECK START REQUESTED");

                        if let Some(target_ip) = ping_target_ip {
                            TvNetworkChecker::check_tv(
                                &ping_client,
                                target_ip.clone(),
                                Arc::clone(&is_tv_alive),
                                is_tv_alive_notify.clone(),
                                is_pinging.clone(),
                                is_pinging_notify.clone(),
                            ).await;
                        } else {
                            warn!("Alive check requested, but no TV IP address known");
                        }
                    }
                }
            }
        });

        (
            self.is_tv_alive.clone(),
            self.is_tv_alive_notify.clone(),
            is_pinging_clone,
            is_pinging_notify_clone,
        )
    }

    /// TODO: Document
    pub async fn set_tv_ip(&mut self, tv_ip: IpAddr) {
        {
            info!("SETTING IP HERE");
            let mut ip = self.tv_ip.lock().await;
            *ip = Some(tv_ip);
        }

        self.tv_ip_notify.notify_one();
        self.check();
    }

    /// TODO: Document
    pub fn check(&mut self) {
        self.check.notify_one();
    }

    // Private ------------------------------------------------------------------------------------

    /// TODO: Document
    async fn check_tv(
        ping_client: &Client,
        tv_ip: IpAddr,
        is_tv_alive: Arc<AtomicBool>,
        is_tv_alive_notify: Arc<Notify>,
        is_pinging: Arc<AtomicBool>,
        is_pinging_notify: Arc<Notify>,
    ) {
        if is_pinging.load(Ordering::SeqCst) {
            warn!("Network ping is already in progress; not starting new network ping");
            return;
        }

        is_pinging.store(true, Ordering::SeqCst);
        is_pinging_notify.notify_one();

        info!("PINGING TV FROM STRUCT IMPL: {:?}", tv_ip);

        let ping_payload = [0; 8];
        let mut pinger = ping_client.pinger(tv_ip, PingIdentifier::from(1)).await;

        // TODO: Look into what the first parameter should be
        match pinger.ping(1.into(), &ping_payload).await {
            Ok(_) => {
                info!("TV IS ALIVE");
                is_tv_alive.store(true, Ordering::SeqCst);
            }
            Err(e) => {
                info!("TV NOT ALIVE");
                is_tv_alive.store(false, Ordering::SeqCst);
            }
        }

        is_tv_alive_notify.notify_one();

        is_pinging.store(false, Ordering::SeqCst);
        is_pinging_notify.notify_one();
    }
}
