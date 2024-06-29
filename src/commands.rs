//! LG TV control commands.
//!
//! LG commands related to the volume and mute state of the TV. Most commands set a new value for
//! volume or mute. `GetVolumeSubscription` is different in that it subscribes to announcements
//! from the TV regarding any changes in the volume or mute setting.

use std::fmt;

use serde_json::{Map, Value};

use crate::{
    generate_lgtv_message_id,
    messages::{LgTvRequest, LgTvRequestType},
};

const GET_VOLUME_SUBSCRIPTION_ID: &str = "get_volume_subscription";

/// LG control commands.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TvCommand {
    /// Get current software information for the TV.
    GetCurrentSWInfo,
    /// Get a list of external inputs on the TV.
    GetExternalInputList,
    /// Get the power status of the TV.
    GetPowerState,
    /// Get system information about the TV.
    GetSystemInfo,
    /// Get the current volume details, including mute.
    GetVolume,
    /// Media fast forward.
    MediaFastForward,
    /// Media pause.
    MediaPause,
    /// Media play.
    MediaPlay,
    /// Media rewind.
    MediaRewind,
    /// Media stop.
    MediaStop,
    /// Set the mute value (`true` is muted, `false` is not muted).
    SetMute(bool),
    /// Set the screen on/off state (`true` is on, `false` is off).
    SetScreenOn(bool),
    /// Set the volume level.
    SetVolume(u8),
    /// Subscribe to volume updates, including mute.
    #[doc(hidden)]
    SubscribeGetVolume,
    /// Switch to the given input id on the TV.
    SwitchInput(String),
    /// Turn off the TV.
    TurnOff,
    /// Decrease the volume level by one step.
    VolumeDown,
    /// Increase the volume level by one step.
    VolumeUp,
}

impl fmt::Display for TvCommand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TvCommand::SetMute(val) => write!(f, "SetMute({})", val),
            TvCommand::SetScreenOn(val) => write!(f, "SetScreenOn({})", val),
            TvCommand::SetVolume(val) => write!(f, "SetVolume({})", val),
            TvCommand::SwitchInput(val) => write!(f, "SwitchInput({})", val),
            variant => write!(f, "{:?}", variant),
        }
    }
}

impl From<TvCommand> for String {
    fn from(val: TvCommand) -> Self {
        match val {
            TvCommand::GetCurrentSWInfo => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some(
                        "ssap://com.webos.service.update/getCurrentSWInformation".to_string(),
                    ),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::GetExternalInputList => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some(
                        "ssap://tv/getExternalInputList".to_string(),
                    ),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::GetPowerState => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://com.webos.service.tvpower/power/getPowerState".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::GetSystemInfo => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://system/getSystemInfo".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::GetVolume => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://audio/getVolume".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::MediaFastForward => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://media.controls/fastForward".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::MediaPause => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://media.controls/pause".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::MediaPlay => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://media.controls/play".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::MediaRewind => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://media.controls/rewind".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::MediaStop => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://media.controls/stop".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::SetMute(value) => {
                let mut map = Map::new();
                map.insert("mute".to_string(), Value::Bool(value));

                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://audio/setMute".to_string()),
                    payload: Some(Value::Object(map)),
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::SetScreenOn(value) => {
                let on_off_state = if value { "On" } else { "Off" };

                let mut map = Map::new();
                map.insert("standbyMode".to_string(), Value::String("active".into()));

                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some(format!(
                        "ssap://com.webos.service.tvpower/power/turn{on_off_state}Screen"
                    )),
                    payload: Some(Value::Object(map)),
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::SetVolume(value) => {
                let mut map = Map::new();
                map.insert("volume".to_string(), Value::Number(value.into()));

                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://audio/setVolume".to_string()),
                    payload: Some(Value::Object(map)),
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::SubscribeGetVolume => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Subscribe,
                    id: GET_VOLUME_SUBSCRIPTION_ID.into(),
                    uri: Some("ssap://audio/getVolume".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::SwitchInput(input_id) => {
                let mut map = Map::new();
                map.insert("inputId".to_string(), Value::String(input_id));

                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://tv/switchInput".to_string()),
                    payload: Some(Value::Object(map)),
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::TurnOff => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://system/turnOff".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::VolumeDown => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://audio/volumeDown".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
            TvCommand::VolumeUp => {
                let data = LgTvRequest {
                    r#type: LgTvRequestType::Request,
                    id: generate_lgtv_message_id(),
                    uri: Some("ssap://audio/volumeUp".to_string()),
                    payload: None,
                };

                serde_json::to_string(&data).unwrap_or_else(|_| String::new())
            }
        }
    }
}

// ================================================================================================
// Tests

#[cfg(test)]
mod tests {
    use super::TvCommand;

    // TvCommand payload string creation. These assume that serde will always serialize keys in
    // the same order.

    #[test]
    fn payload_get_current_sw_information() {
        let payload: String = TvCommand::GetCurrentSWInfo.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://com.webos.service.update/getCurrentSWInformation","payload":null}"#
        );
    }

    #[test]
    fn payload_get_external_input_list() {
        let payload: String = TvCommand::GetExternalInputList.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://tv/getExternalInputList","payload":null}"#
        );
    }

    #[test]
    fn payload_get_power_status() {
        let payload: String = TvCommand::GetPowerState.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://com.webos.service.tvpower/power/getPowerState","payload":null}"#
        );
    }

    #[test]
    fn payload_get_system_info() {
        let payload: String = TvCommand::GetSystemInfo.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://system/getSystemInfo","payload":null}"#
        );
    }

    #[test]
    fn payload_get_volume() {
        let payload: String = TvCommand::GetVolume.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://audio/getVolume","payload":null}"#
        );
    }

    #[test]
    fn payload_media_fastforward() {
        let payload: String = TvCommand::MediaFastForward.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://media.controls/fastForward","payload":null}"#
        );
    }

    #[test]
    fn payload_media_pause() {
        let payload: String = TvCommand::MediaPause.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://media.controls/pause","payload":null}"#
        );
    }

    #[test]
    fn payload_media_play() {
        let payload: String = TvCommand::MediaPlay.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://media.controls/play","payload":null}"#
        );
    }

    #[test]
    fn payload_media_rewind() {
        let payload: String = TvCommand::MediaRewind.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://media.controls/rewind","payload":null}"#
        );
    }

    #[test]
    fn payload_media_stop() {
        let payload: String = TvCommand::MediaStop.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://media.controls/stop","payload":null}"#
        );
    }

    #[test]
    fn payload_set_mute() {
        let payload_true: String = TvCommand::SetMute(true).into();

        assert_eq!(
            payload_true,
            r#"{"type":"request","id":"test-id","uri":"ssap://audio/setMute","payload":{"mute":true}}"#
        );

        let payload_false: String = TvCommand::SetMute(false).into();

        assert_eq!(
            payload_false,
            r#"{"type":"request","id":"test-id","uri":"ssap://audio/setMute","payload":{"mute":false}}"#
        );
    }

    #[test]
    fn payload_set_screen_on() {
        let payload_true: String = TvCommand::SetScreenOn(true).into();

        assert_eq!(
            payload_true,
            r#"{"type":"request","id":"test-id","uri":"ssap://com.webos.service.tvpower/power/turnOnScreen","payload":{"standbyMode":"active"}}"#
        );

        let payload_false: String = TvCommand::SetScreenOn(false).into();

        assert_eq!(
            payload_false,
            r#"{"type":"request","id":"test-id","uri":"ssap://com.webos.service.tvpower/power/turnOffScreen","payload":{"standbyMode":"active"}}"#
        );
    }

    #[test]
    fn payload_set_volume() {
        let payload: String = TvCommand::SetVolume(10).into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://audio/setVolume","payload":{"volume":10}}"#
        );
    }

    #[test]
    fn payload_subscribe_get_volume() {
        let payload: String = TvCommand::SubscribeGetVolume.into();

        assert_eq!(
            payload,
            r#"{"type":"subscribe","id":"get_volume_subscription","uri":"ssap://audio/getVolume","payload":null}"#
        );
    }

    #[test]
    fn payload_switch_input() {
        let payload: String = TvCommand::SwitchInput("test_input".into()).into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://tv/switchInput","payload":{"inputId":"test_input"}}"#
        );
    }

    #[test]
    fn payload_turn_off() {
        let payload: String = TvCommand::TurnOff.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://system/turnOff","payload":null}"#
        );
    }

    #[test]
    fn payload_volume_down() {
        let payload: String = TvCommand::VolumeDown.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://audio/volumeDown","payload":null}"#
        );
    }

    #[test]
    fn payload_volume_up() {
        let payload: String = TvCommand::VolumeUp.into();

        assert_eq!(
            payload,
            r#"{"type":"request","id":"test-id","uri":"ssap://audio/volumeUp","payload":null}"#
        );
    }

    // TvCommand display

    #[test]
    fn tvcommand_display() {
        assert_eq!(TvCommand::GetCurrentSWInfo.to_string(), "GetCurrentSWInfo");
        assert_eq!(TvCommand::GetExternalInputList.to_string(), "GetExternalInputList");
        assert_eq!(TvCommand::GetPowerState.to_string(), "GetPowerState");
        assert_eq!(TvCommand::GetSystemInfo.to_string(), "GetSystemInfo");
        assert_eq!(TvCommand::GetVolume.to_string(), "GetVolume");
        assert_eq!(TvCommand::MediaFastForward.to_string(), "MediaFastForward");
        assert_eq!(TvCommand::MediaPause.to_string(), "MediaPause");
        assert_eq!(TvCommand::MediaPlay.to_string(), "MediaPlay");
        assert_eq!(TvCommand::MediaRewind.to_string(), "MediaRewind");
        assert_eq!(TvCommand::MediaStop.to_string(), "MediaStop");
        assert_eq!(TvCommand::SetMute(true).to_string(), "SetMute(true)");
        assert_eq!(TvCommand::SetMute(false).to_string(), "SetMute(false)");
        assert_eq!(TvCommand::SetScreenOn(true).to_string(), "SetScreenOn(true)");
        assert_eq!(TvCommand::SetScreenOn(false).to_string(), "SetScreenOn(false)");
        assert_eq!(TvCommand::SetVolume(10).to_string(), "SetVolume(10)");
        assert_eq!(TvCommand::SubscribeGetVolume.to_string(), "SubscribeGetVolume");
        assert_eq!(TvCommand::SwitchInput("test_input".into()).to_string(), "SwitchInput(test_input)");
        assert_eq!(TvCommand::TurnOff.to_string(), "TurnOff");
        assert_eq!(TvCommand::VolumeDown.to_string(), "VolumeDown");
        assert_eq!(TvCommand::VolumeUp.to_string(), "VolumeUp");
    }
}
