use macaddr::MacAddr;

use crate::ssap_payloads::GetSystemInfoPayload;

/// Information on the TV last connected to by the Manager.
#[derive(Debug, Default, Clone, PartialEq, Hash)]
pub struct LastSeenTv {
    pub websocket_url: Option<String>,
    pub client_key: Option<String>,
    pub mac_addr: Option<MacAddr>,
}

/// Current TV state for the managed LG TV.
#[derive(Debug, Default, Clone, PartialEq, Hash)]
pub struct TvState {
    pub power_state: Option<String>,
    pub volume: Option<u8>,
    pub is_muted: Option<bool>,
    pub is_screen_on: Option<bool>,
    pub is_volume_settable: Option<bool>,
}

/// TV information for the managed LG TV.
#[derive(Debug, Clone, PartialEq, Hash)]
pub struct TvInfo {
    pub receiver_type: String,
    pub model_name: String,
    pub serial_number: String,
    pub program_mode: bool,
}

impl From<GetSystemInfoPayload> for TvInfo {
    fn from(system_info: GetSystemInfoPayload) -> Self {
        TvInfo {
            receiver_type: system_info.receiver_type,
            model_name: system_info.model_name,
            serial_number: system_info.serial_number,
            program_mode: system_info.program_mode,
        }
    }
}

// ================================================================================================
// Tests

#[cfg(test)]
mod tests {
    use super::{TvInfo, TvState};
    use crate::ssap_payloads::GetSystemInfoPayload;

    #[test]
    fn tv_state_default() {
        assert_eq!(
            TvState::default(),
            TvState {
                power_state: None,
                volume: None,
                is_muted: None,
                is_screen_on: None,
                is_volume_settable: None,
            }
        );
    }

    #[test]
    fn tv_info_from_get_system_info_payload() {
        let system_info_json = r#"
        {
            "returnValue": true,
            "features": {
                "dvr": false
            },
            "receiverType": "TEST_RECEIVER_TYPE",
            "modelName": "TEST_MODEL_NAME",
            "serialNumber": "TEST_SERIAL_NUMBER",
            "programMode": true
        }
        "#;

        let system_info_payload: GetSystemInfoPayload =
            serde_json::from_str(system_info_json).unwrap();

        assert_eq!(
            TvInfo::from(system_info_payload),
            TvInfo {
                receiver_type: "TEST_RECEIVER_TYPE".to_string(),
                model_name: "TEST_MODEL_NAME".to_string(),
                serial_number: "TEST_SERIAL_NUMBER".to_string(),
                program_mode: true,
            }
        );
    }
}
