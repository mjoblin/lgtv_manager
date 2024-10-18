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
    GetVolumeVariantA(GetVolumePayloadVariantA),
    GetVolumeVariantB(GetVolumePayloadVariantB),
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
    pub app_id: Option<String>,
    pub icon: Option<String>,
    #[serde(rename = "forceIcon")]
    pub force_icon: Option<bool>,
    pub modified: Option<bool>,
    #[serde(rename = "lastUniqueId")]
    pub last_unique_id: Option<i64>,
    #[serde(rename = "hdmiPlugIn")]
    pub hdmi_plug_in: Option<bool>,
    #[serde(rename = "subCount")]
    pub sub_count: Option<i64>,
    pub favorite: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct GetExternalInputListPayload {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    pub devices: Vec<ExternalInput>,
}

// GetPowerState

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GetPowerStatePayload {
    pub state: String,
    pub processing: Option<String>,
    #[serde(rename = "returnValue")]
    pub return_value: bool,
}

// GetSystemInfo

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Features {
    #[serde(rename = "3d")]
    pub three_d: Option<bool>,
    pub dvr: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct GetSystemInfoPayload {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    pub features: Option<Features>,
    #[serde(rename = "receiverType")]
    pub receiver_type: String,
    #[serde(rename = "modelName")]
    pub model_name: String,
    #[serde(rename = "serialNumber")]
    pub serial_number: Option<String>, // "serial_number" isn't always present
    #[serde(rename = "programMode")]
    pub program_mode: bool,
}

// VolumeStatus
//
// Two variants of VolumeStatus payloads have been seen across various TVs. There could be other
// variants not allowed for here.

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct VolumeStatusVariantA {
    #[serde(rename = "volumeLimitable")]
    pub volume_limitable: Option<bool>,
    #[serde(rename = "activeStatus")]
    pub active_status: Option<bool>,
    #[serde(rename = "maxVolume")]
    pub max_volume: u8,
    #[serde(rename = "volumeLimiter")]
    pub volume_limiter: Option<String>,
    #[serde(rename = "soundOutput")]
    pub sound_output: Option<String>,
    pub volume: u8,
    pub cause: Option<String>, // "cause" is only included for "subscribe" payloads (not "request")
    pub mode: Option<String>,
    #[serde(rename = "externalDeviceControl")]
    pub external_device_control: Option<bool>,
    #[serde(rename = "muteStatus")]
    pub mute_status: bool,
    #[serde(rename = "volumeSyncable")]
    pub volume_syncable: Option<bool>,
    #[serde(rename = "adjustVolume")]
    pub adjust_volume: bool,
}

// GetVolume

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct GetVolumePayloadVariantA {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    #[serde(rename = "volumeStatus")]
    pub volume_status: VolumeStatusVariantA,
    #[serde(rename = "callerId")]
    pub caller_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct GetVolumePayloadVariantB {
    #[serde(rename = "returnValue")]
    pub return_value: bool,
    #[serde(rename = "volumeMax")]
    pub volume_max: u8,
    pub ringtonewithvibration: Option<bool>,
    pub scenario: Option<String>,
    pub muted: bool,
    pub hac: Option<bool>,
    pub subscribed: Option<bool>,
    pub volume: u8,
    pub action: Option<String>,
    pub supportvolume: bool,
    #[serde(rename = "ringer switch")]
    pub ringer_switch: Option<bool>,
    pub slider: Option<bool>,
    pub active: Option<bool>,
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
