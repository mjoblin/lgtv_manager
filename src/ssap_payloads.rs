//! Message payloads for the LG TV SSAP WebSocket protocol.
//!
//! These messages define the shape of the [`LgTvRequest`] and [`LgTvResponse`] payloads sent to and
//! received from an LG TV over its WebSocket connection.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ------------------------------------------------------------------------------------------------
// Requests

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LgTvRequestType {
    Register,
    Request,
    Subscribe,
}

// Top-level Request shape

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct LgTvRequest {
    pub r#type: LgTvRequestType,
    pub id: String,
    pub uri: Option<String>,
    pub payload: Option<Value>,
}

// ------------------------------------------------------------------------------------------------
// Responses

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub(crate) enum LgTvResponsePayload {
    CurrentSwInfo(CurrentSwInfoPayload),
    GetExternalInputList(GetExternalInputListPayload),
    GetPowerState(GetPowerStatePayload),
    GetSystemInfo(GetSystemInfoPayload),
    GetVolume(GetVolumePayload),
    Pair(PairPayload),
    PlainReturnValue(PlainReturnValuePayload),
    SetMute(SetMutePayload),
    SetScreenOn(SetScreenOnPayload),
    SetVolume(SetVolumePayload), // Received for setVolume, volumeDown, and volumeUp
}

// Top-level Response shape
//
// Responses are either: Error, Registered (pairing has occurred), or Command (LgTvCommand response)

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub(crate) enum LgTvResponse {
    #[serde(rename = "error")]
    Error(LgTvErrorResponse),
    #[serde(rename = "registered")]
    Registered(LgTvRegisteredResponse),
    #[serde(rename = "response")]
    Command(Box<LgTvCommandResponse>),
}

// Errors

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct LgTvErrorResponse {
    pub id: String,
    pub error: Option<String>,
    pub payload: ErrorPayload,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ErrorPayload {}

// Registration

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct LgTvRegisteredResponse {
    pub id: String,
    pub payload: RegisteredPayload,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct RegisteredPayload {
    #[serde(rename = "client-key")]
    pub client_key: String,
}

// Pairing

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PairPayload {
    #[serde(rename = "pairingType")]
    pub pairing_type: String,
    #[serde(rename = "returnValue")]
    pub return_value: bool,
}

// Plain return payload with no additional information

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlainReturnValuePayload {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
}

// LgTvCommand responses

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct LgTvCommandResponse {
    pub id: String,
    pub payload: LgTvResponsePayload,
}

// Responses to conventional TvCommands -----------------------------------------------------------

// GetCurrentSwInformation

/// TV software information for the managed LG TV.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CurrentSwInfoPayload {
    #[doc(hidden)]
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    pub product_name: String,
    pub model_name: String,
    pub sw_type: String,
    pub major_ver: String,
    pub minor_ver: String,
    pub country: String,
    pub country_group: String,
    pub device_id: String,
    pub auth_flag: String,
    pub ignore_disable: String,
    pub eco_info: String,
    pub config_key: String,
    pub language_code: String,
}

// GetExternalInputList

/// A TV input (e.g. HDMI).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExternalInput {
    pub id: String,
    pub label: String,
    pub port: i64,
    pub connected: bool,
    #[serde(rename = "appId")]
    pub app_id: String,
    pub icon: String,
    #[serde(rename = "forceIcon")]
    pub force_icon: bool,
    pub modified: bool,
    #[serde(rename = "lastUniqueId")]
    pub last_unique_id: i64,
    #[serde(rename = "hdmiPlugIn")]
    pub hdmi_plug_in: bool,
    #[serde(rename = "subCount")]
    pub sub_count: i64,
    pub favorite: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct GetExternalInputListPayload {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    pub devices: Vec<ExternalInput>,
}

// GetPowerState

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct GetPowerStatePayload {
    pub state: String,
    #[serde(rename = "returnValue")]
    pub return_value: bool,
}

// GetSystemInfo

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Features {
    pub dvr: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GetSystemInfoPayload {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    pub features: Features,
    #[serde(rename = "receiverType")]
    pub receiver_type: String,
    #[serde(rename = "modelName")]
    pub model_name: String,
    #[serde(rename = "serialNumber")]
    pub serial_number: String,
    #[serde(rename = "programMode")]
    pub program_mode: bool,
}

// VolumeStatus

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct VolumeStatus {
    #[serde(rename = "volumeLimitable")]
    pub volume_limitable: bool,
    #[serde(rename = "activeStatus")]
    pub active_status: bool,
    #[serde(rename = "maxVolume")]
    pub max_volume: u8,
    #[serde(rename = "volumeLimiter")]
    pub volume_limiter: String,
    #[serde(rename = "soundOutput")]
    pub sound_output: String,
    pub volume: u8,
    pub cause: Option<String>, // "cause" is only included for "subscribe" payloads (not "request")
    pub mode: String,
    #[serde(rename = "externalDeviceControl")]
    pub external_device_control: bool,
    #[serde(rename = "muteStatus")]
    pub mute_status: bool,
    #[serde(rename = "volumeSyncable")]
    pub volume_syncable: bool,
    #[serde(rename = "adjustVolume")]
    pub adjust_volume: bool,
}

// GetVolume

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct GetVolumePayload {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    #[serde(rename = "volumeStatus")]
    pub volume_status: VolumeStatus,
    #[serde(rename = "callerId")]
    pub caller_id: String,
}

// SetMute

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SetMutePayload {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    #[serde(rename = "muteStatus")]
    pub mute_status: bool,
    #[serde(rename = "soundOutput")]
    pub sound_output: String,
}

// SetScreenOn

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SetScreenOnPayload {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    #[serde(rename = "state")]
    pub state: String,
}

// SetVolume

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SetVolumePayload {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    pub volume: u8,
    #[serde(rename = "soundOutput")]
    pub sound_output: String,
}
