//! Helper functions.

use core::net::IpAddr;

use serde_json::Value;
use url::{Host, Url};
#[cfg(not(test))]
use uuid::Uuid;

use crate::ssap_payloads::{LgTvRequest, LgTvRequestType};
use crate::{Connection, LgTvDevice};

// This LG TV register payload seems to be used by various projects which talk to LG TVs over the
// SSAP WebSocket protocol. Since SSAP appears to be largely undocumented, this payload is being
// left as-is.
const REGISTER_PAYLOAD: &str = include_str!("../resources/register_payload.json");

/// Generate a unique LG message Id.
pub(crate) fn generate_lgtv_message_id() -> String {
    #[cfg(test)]
    let id = "test-id".to_string();
    #[cfg(not(test))]
    let id = Uuid::new_v4().hyphenated().to_string();

    id
}

/// Extract the IP address from the given `url`.
pub(crate) fn url_ip_addr(url: &str) -> Option<IpAddr> {
    match Url::parse(url) {
        Ok(parsed_url) => match parsed_url.host() {
            Some(host) => match host {
                Host::Ipv4(ipv4) => Some(IpAddr::V4(ipv4)),
                Host::Ipv6(ipv6) => Some(IpAddr::V6(ipv6)),
                // TODO: Consider converting a hostname into an IP address
                _ => None,
            },
            None => None,
        },
        Err(_) => None,
    }
}

/// Generate an LG registration payload.
///
/// A client key is injected (if provided). If forced pairing is requested, the TV will trigger a
/// fresh pair flow regardless of whether a valid client key is provided or not.
pub(crate) fn generate_register_request(
    client_key: Option<String>,
    force_pairing: bool,
) -> Result<LgTvRequest, String> {
    let json_data: Value = serde_json::from_str(REGISTER_PAYLOAD)
        .map_err(|e| format!("Could not create register payload: {:?}", e))?;

    if let Value::Object(mut payload) = json_data {
        payload.insert(
            "client-key".to_string(),
            match client_key {
                Some(client_key) => Value::String(client_key),
                None => Value::Null,
            },
        );

        payload.insert("forcePairing".to_string(), Value::Bool(force_pairing));

        Ok(LgTvRequest {
            r#type: LgTvRequestType::Register,
            id: generate_lgtv_message_id(),
            uri: None,
            payload: Some(payload.into()),
        })
    } else {
        Err(String::from("Could not create register payload"))
    }
}

/// Generate a WebSocket URL for connecting to an LG TV.
///
/// `in_str` should be at least a host name or IP address (IPv4 or IPv6). If no scheme or port is
/// specified then wss and 3001 are assumed if `is_tls` is `true` (otherwise ws and 3000 are used).
/// If a scheme is provided then it must be wss or ws.
///
/// Examples:
///
/// 10.0.0.101 -> wss://10.0.0.101:3001/
/// tv.local/some/path -> wss://tv.local:3001/some/path
/// ws://10.0.0.101 -> ws://10.0.0.101:3000/
/// 10.0.0.101:3333 -> wss://10.0.0.101:3333/
fn generate_possible_websocket_url(in_str: &str, is_tls: bool) -> Result<String, String> {
    if in_str.is_empty() {
        return Err(String::from("No host specified"));
    }

    let mut in_with_scheme = String::from(in_str);

    if !in_with_scheme.contains("://") {
        in_with_scheme = format!("{}://{}", if is_tls { "wss" } else { "ws" }, in_with_scheme);
    }

    return match Url::parse(&in_with_scheme) {
        Ok(mut parsed_url) => match parsed_url.scheme() {
            "ws" => match is_tls {
                true => Err(String::from(
                    "Cannot use 'ws' scheme with TLS (must be wss)",
                )),
                false => Ok(match &parsed_url.port() {
                    Some(_) => parsed_url.to_string(),
                    None => {
                        parsed_url
                            .set_port(Some(3000))
                            .map_err(|_| "Could not set URL to port 3000")?;
                        parsed_url.to_string()
                    }
                }),
            },
            "wss" => match is_tls {
                true => Ok(match &parsed_url.port() {
                    Some(_) => parsed_url.to_string(),
                    None => {
                        parsed_url
                            .set_port(Some(3001))
                            .map_err(|_| "Could not set URL to port 3001")?;
                        parsed_url.to_string()
                    }
                }),
                false => Err(String::from(
                    "Cannot use 'wss' scheme without TLS (must be ws)",
                )),
            },
            _ => Err(format!(
                "Invalid scheme: {} (must be 'wss' or 'ws')",
                parsed_url.scheme()
            )),
        },
        Err(e) => Err(format!("Could not parse host: {:?}", e)),
    };
}

/// Generate an LG WebSocket URL for the given Connection.
pub(crate) fn websocket_url_for_connection(connection: &Connection) -> Result<String, String> {
    // Determine the TV URL (most likely an IP or host name)
    let tv_url = match connection {
        Connection::Host(host, _) => host.clone(),
        Connection::Device(device, _) => match device_host(device) {
            Ok(host) => host.clone(),
            Err(e) => {
                return Err(e);
            }
        },
    };

    // Determine whether TLS is requested.
    let is_tls = match &connection {
        Connection::Host(_, settings) => settings.is_tls,
        Connection::Device(_, settings) => settings.is_tls,
    };

    // Generate a WebSocket URL from the TV host and TLS requirement.
    generate_possible_websocket_url(&tv_url, is_tls)
}

/// Extract the hostname from the given UPnP `device`.
pub(crate) fn device_host(device: &LgTvDevice) -> Result<String, String> {
    match Url::parse(&device.url) {
        Ok(url) => match url.host_str() {
            Some(host) => Ok(String::from(host)),
            None => Err(format!(
                "Could not determine hostname for UPnP device '{}'",
                &device.friendly_name,
            )),
        },
        Err(e) => Err(format!(
            "Could not determine hostname for UPnP device '{}': {}",
            &device.friendly_name, &e
        )),
    }
}

/// Calculate exponential reconnect falloff in milliseconds.
pub(crate) fn reconnect_falloff(attempt: u64, min_delay: u64, max_delay: u64) -> u128 {
    let base_delay: f64 = 0.0;
    let growth_rate: f64 = 2.0;
    let skip_count: u64 = 6; // Skip initial very-small values

    let delay = base_delay + growth_rate.powf((attempt + skip_count) as f64);

    delay.min(max_delay as f64).max(min_delay as f64) as u128
}

// ================================================================================================
// Tests

#[cfg(test)]
mod tests {
    use super::{generate_possible_websocket_url, reconnect_falloff};

    #[test]
    fn valid_websocket_url_generation() {
        // Does not alter fully-qualified WebSocket url
        assert_eq!(
            generate_possible_websocket_url("ws://127.0.0.1:8080", false).unwrap(),
            "ws://127.0.0.1:8080/"
        );

        assert_eq!(
            generate_possible_websocket_url("wss://127.0.0.1:8080", true).unwrap(),
            "wss://127.0.0.1:8080/"
        );

        // Adds scheme and port if missing
        assert_eq!(
            generate_possible_websocket_url("127.0.0.1", true).unwrap(),
            "wss://127.0.0.1:3001/"
        );

        assert_eq!(
            generate_possible_websocket_url("something", true).unwrap(),
            "wss://something:3001/"
        );

        assert_eq!(
            generate_possible_websocket_url("127.0.0.1", false).unwrap(),
            "ws://127.0.0.1:3000/"
        );

        assert_eq!(
            generate_possible_websocket_url("something", false).unwrap(),
            "ws://something:3000/"
        );

        assert_eq!(
            generate_possible_websocket_url("[fd12:3456:789a:1::1]", true).unwrap(),
            "wss://[fd12:3456:789a:1::1]:3001/"
        );

        assert_eq!(
            generate_possible_websocket_url("[fd12:3456:789a:1::1]", false).unwrap(),
            "ws://[fd12:3456:789a:1::1]:3000/"
        );

        // Allows for path retention
        assert_eq!(
            generate_possible_websocket_url("wss://127.0.0.1:8080/foo", true).unwrap(),
            "wss://127.0.0.1:8080/foo"
        );

        assert_eq!(
            generate_possible_websocket_url("127.0.0.1/foo", true).unwrap(),
            "wss://127.0.0.1:3001/foo"
        );

        assert_eq!(
            generate_possible_websocket_url("[fd12:3456:789a:1::1]/foo", false).unwrap(),
            "ws://[fd12:3456:789a:1::1]:3000/foo"
        );
    }

    #[test]
    fn invalid_websocket_url_generation() {
        // Invalid scheme
        assert!(generate_possible_websocket_url("http://127.0.0.1", true).is_err(),);
        assert!(generate_possible_websocket_url("http://127.0.0.1", false).is_err(),);

        // Empty
        assert!(generate_possible_websocket_url("", true).is_err(),);
        assert!(generate_possible_websocket_url("", false).is_err(),);

        // Invalid WebSocket scheme for given is_tls
        assert!(generate_possible_websocket_url("ws://127.0.0.1:8080", true).is_err(),);

        assert!(generate_possible_websocket_url("wss://127.0.0.1:8080", false).is_err(),);
    }

    #[test]
    fn reconnect_falloff_results() {
        assert_eq!(reconnect_falloff(0, 250, 10_000), 250);
        assert_eq!(reconnect_falloff(1, 250, 10_000), 250);
        assert_eq!(reconnect_falloff(2, 250, 10_000), 256);
        assert_eq!(reconnect_falloff(3, 250, 10_000), 512);
        assert_eq!(reconnect_falloff(10_000, 250, 10_000), 10_000);
    }
}
