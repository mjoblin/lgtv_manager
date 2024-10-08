//! Discover LG TV UPnP devices on the local network.

use std::collections::HashSet;
use std::fmt;
use std::net::IpAddr;
use std::process::Command;
use std::str::FromStr;
use std::time::Duration;

use futures::prelude::*;
use log::{info, warn};
use macaddr::MacAddr;
use rupnp::ssdp::{SearchTarget, URN};

use crate::helpers::url_ip_addr;

const DISCOVERY_DURATION_SECS: u64 = 3;
const MEDIA_RENDERER: URN = URN::device("schemas-upnp-org", "MediaRenderer", 1);

/// A UPnP Device representation of an LG TV.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
pub struct LgTvDevice {
    pub friendly_name: String,
    pub model: String,
    pub model_number: Option<String>,
    pub serial_number: Option<String>,
    pub url: String,
    pub udn: String,
    pub mac_addr: Option<String>,
}

impl fmt::Display for LgTvDevice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}) [{}]", self.friendly_name, self.model, self.udn)
    }
}

/// Get the MAC address for the given IP.
///
/// This uses the "arp" CLI tool assumed to be available from the OS. If that tool is not
/// available, or if the MAC address was otherwise not determined, then None is returned.
pub(crate) fn mac_address_for_ip(ip: IpAddr) -> Option<MacAddr> {
    match Command::new("arp").arg("-n").arg(ip.to_string()).output() {
        Ok(arp_cmd_output) => {
            let output_str = String::from_utf8_lossy(&arp_cmd_output.stdout);

            for line in output_str.lines() {
                if line.contains(&ip.to_string()) {
                    let output_chunks: Vec<&str> = line.trim().split_whitespace().collect();

                    for output_chunk in output_chunks {
                        if output_chunk.parse::<MacAddr>().is_ok() {
                            return MacAddr::from_str(output_chunk).ok();
                        }
                    }

                    warn!("Could not determine MAC address; MAC not found in 'arp' output");
                    return None;
                }
            }

            None
        }
        Err(e) => {
            warn!(
                "Could not invoke the 'arp' tool to determine MAC address: {:?}",
                e
            );
            None
        }
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

    match rupnp::discover(&search_target, Duration::from_secs(DISCOVERY_DURATION_SECS)).await {
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
                        mac_addr: url_ip_addr(&device.url().to_string())
                            .and_then(|ip_addr| mac_address_for_ip(ip_addr))
                            .map(|mac_addr| mac_addr.to_string()),
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
            warn!("LG TV UPnP discovery error: {:?}", e);
        }
    }

    info!(
        "LG TV UPnP discovery found {} TV{}",
        &found_count,
        if found_count == 1 { "" } else { "s" }
    );

    Ok(discovered_lgtv_devices)
}

// ================================================================================================
// Tests

#[cfg(test)]
mod tests {
    use super::LgTvDevice;

    #[test]
    fn lgtvdevice_display() {
        assert_eq!(
            LgTvDevice {
                friendly_name: "LG WebOS TV".to_string(),
                model: "model".to_string(),
                model_number: None,
                serial_number: None,
                url: "http://38.0.101.76:3000".to_string(),
                udn: "uuid:00000000-0000-0000-0000-000000000000".to_string(),
                mac_addr: None,
            }
            .to_string(),
            "LG WebOS TV (model) [uuid:00000000-0000-0000-0000-000000000000]"
        )
    }
}
