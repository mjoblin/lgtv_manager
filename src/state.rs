use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

use directories::ProjectDirs;
use log::debug;
use serde::{Deserialize, Serialize};

use crate::messages::GetSystemInfoPayload;

const PERSISTED_STATE_FILE: &str = "lgtv_manager_data.json";

/// Current TV state for the managed LG TV.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TvState {
    pub power_state: Option<String>,
    pub volume: Option<u8>,
    pub is_muted: Option<bool>,
    pub is_screen_on: Option<bool>,
    pub is_volume_settable: Option<bool>,
}

/// TV information for the managed LG TV.
#[derive(Debug, Clone, PartialEq)]
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

/// LgTvManager state persisted to disk.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PersistedState {
    pub ws_url: Option<String>,
    pub client_key: Option<String>,
}

// ------------------------------------------------------------------------------------------------0

pub(crate) fn read_persisted_state(data_dir: Option<PathBuf>) -> Result<PersistedState, String> {
    let data_file = get_data_file(data_dir)?;

    match File::open(data_file) {
        Ok(mut file) => {
            let mut file_data = String::new();

            match file.read_to_string(&mut file_data) {
                Ok(_) => match serde_json::from_str::<PersistedState>(&file_data) {
                    Ok(persisted_state) => Ok(persisted_state),
                    Err(e) => Err(format!("Error reading persisted state: {:?}", e)),
                },
                Err(e) => Err(format!("Error reading persisted state: {:?}", e)),
            }
        }
        Err(e) => Err(format!("Could not read persisted state: {:?}", e)),
    }
}

pub(crate) fn write_persisted_state(
    state: PersistedState,
    data_dir: Option<PathBuf>,
) -> Result<(), String> {
    let data_file = get_data_file(data_dir)?;

    return match File::create(data_file) {
        Ok(mut file) => {
            let persisted_state = PersistedState {
                ws_url: state.ws_url.clone(),
                client_key: state.client_key.clone(),
            };

            match serde_json::to_string(&persisted_state) {
                Ok(json) => match file.write_all(json.as_bytes()) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(format!("Error writing persisted state: {:?}", e)),
                },
                Err(e) => Err(format!("Error writing persisted state: {:?}", e)),
            }
        }
        Err(e) => Err(format!("Error writing persisted state: {:?}", e)),
    };
}

/// Get the path to the data file as a PathBuf, creating the directory if necessary.
fn get_data_file(passed_path: Option<PathBuf>) -> Result<PathBuf, String> {
    let mut dir_path_buf = match passed_path {
        Some(path) => path,
        None => {
            if let Some(project_dirs) = ProjectDirs::from("com", "lgtv_manager", "lgtv_manager") {
                project_dirs.data_dir().to_path_buf()
            } else {
                return Err(
                    "Could not determine where to write data file: home directory unknown"
                        .to_string(),
                );
            }
        }
    };

    // Ensure the directory exists
    if let Err(e) = std::fs::create_dir_all(&dir_path_buf) {
        return Err(format!("Could not create data directory: {:?}", e));
    }

    dir_path_buf.push(PERSISTED_STATE_FILE);
    debug!("Data file: {:?}", dir_path_buf);

    Ok(dir_path_buf)
}

// ================================================================================================
// Tests

#[cfg(test)]
mod tests {
    use super::{TvInfo, TvState};
    use crate::messages::GetSystemInfoPayload;

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
