use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::str::FromStr;

use directories::ProjectDirs;
use log::{debug, warn};
use macaddr::MacAddr;
use serde::{Deserialize, Serialize};

use super::LgTvManager;

// ------------------------------------------------------------------------------------------------
// State persisted to disk for access between sessions.
// ------------------------------------------------------------------------------------------------

const PERSISTED_STATE_FILE: &str = "lgtv_manager_data.json";

/// LgTvManager state persisted to disk.
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PersistedState {
    pub ws_url: Option<String>,
    pub client_key: Option<String>,
    pub mac_addr: Option<String>,
}

impl LgTvManager {
    /// Import any previously-persisted `PersistedState` details from disk and store on the manager.
    pub(crate) fn import_persisted_state(&mut self) {
        match read_persisted_state(self.data_dir.clone()) {
            Ok(persisted_state) => {
                self.ws_url = persisted_state.ws_url;
                self.client_key = persisted_state.client_key;
                self.mac_addr = persisted_state
                    .mac_addr
                    .and_then(|mac_addr_string| MacAddr::from_str(&mac_addr_string).ok());
            }
            Err(e) => {
                warn!("Could not import persisted state: {}", e);
            }
        }
    }

    /// Persist any `PersistedState` details associated with the manager to disk.
    pub(crate) async fn write_persisted_state(&mut self) {
        match write_persisted_state(
            PersistedState {
                ws_url: self.ws_url.clone(),
                client_key: self.client_key.clone(),
                mac_addr: self.mac_addr.map(|mac_addr| mac_addr.to_string()),
            },
            self.data_dir.clone(),
        ) {
            Ok(_) => self.emit_is_wake_tv_available().await,
            Err(e) => warn!("{}", e),
        }
    }

    pub(crate) fn clear_persisted_state_on_manager(&mut self) {
        self.ws_url = None;
        self.client_key = None;
        self.mac_addr = None;
    }
}

// ------------------------------------------------------------------------------------------------0
// Persisted state helpers

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

    match File::create(data_file) {
        Ok(mut file) => {
            let persisted_state = PersistedState {
                ws_url: state.ws_url.clone(),
                client_key: state.client_key.clone(),
                mac_addr: state.mac_addr.clone(),
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
    }
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
