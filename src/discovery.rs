//! Discover LG TV UPnP devices on the local network.

use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

use futures::prelude::*;
use log::{error, info};
use rupnp::ssdp::{SearchTarget, URN};

const MEDIA_RENDERER: URN = URN::device("schemas-upnp-org", "MediaRenderer", 1);

/// A UPnP Device representation of an LG TV.
#[derive(Clone, Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct LgTvDevice {
    pub friendly_name: String,
    pub model: String,
    pub model_number: Option<String>,
    pub serial_number: Option<String>,
    pub url: String,
    pub udn: String,
}

impl fmt::Display for LgTvDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "'{}' ({}) [{}]",
            self.friendly_name, self.model, self.udn
        )
    }
}

/// Scans the local network for LG TVs and returns their UPnP Device representations.
///
/// Returns a vector of [`LgTvDevice`] instances. No discovered TVs is indicated by an empty return
/// vector.
pub(crate) async fn discover_lgtv_devices() -> Result<Vec<LgTvDevice>, String> {
    let search_target = SearchTarget::URN(MEDIA_RENDERER);
    let mut seen_udns: HashSet<String> = HashSet::new();
    let mut found_count = 0;

    info!("Performing UPnP discovery for LG TV devices...");

    let mut discovered_lgtv_devices: Vec<LgTvDevice> = Vec::new();

    match rupnp::discover(&search_target, Duration::from_secs(3)).await {
        Ok(discovered_devices) => {
            pin_utils::pin_mut!(discovered_devices);

            while let Some(device) = discovered_devices
                .try_next()
                .await
                .map_err(|e| format!("UPnP discovery error: {:?}", e.to_string()))?
            {
                let device_udn = device.udn().to_string();

                if seen_udns.contains(&device_udn) {
                    continue;
                }

                seen_udns.insert(device_udn);

                if device.manufacturer().starts_with("LG Elec")
                    && device.model_name().contains("TV")
                {
                    found_count += 1;

                    let lgtv_device = LgTvDevice {
                        friendly_name: device.friendly_name().to_string(),
                        model: device.model_name().to_string(),
                        model_number: device.model_number().map(|s| s.to_owned()),
                        serial_number: device.serial_number().map(|s| s.to_owned()),
                        url: device.url().to_string(),
                        udn: device.udn().to_string(),
                    };

                    info!(
                        "LG TV device discovered: {} @ {}",
                        &lgtv_device, &lgtv_device.url
                    );

                    discovered_lgtv_devices.push(lgtv_device);
                } else {
                    info!(
                        "LG TV UPnP discovery is ignoring non-LG MediaRenderer device '{}' ({}) from {}",
                        device.friendly_name(),
                        device.model_name(),
                        device.manufacturer(),
                    );
                }
            }
        }
        Err(e) => {
            error!("LG TV UPnP discovery error: {:?}", e);
        }
    }

    info!(
        "LG TV UPnP discovery found {} TV{}",
        &found_count,
        if found_count == 1 { "" } else { "s" }
    );

    Ok(discovered_lgtv_devices)
}
