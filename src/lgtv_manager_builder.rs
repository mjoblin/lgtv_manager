use std::path::PathBuf;

use log::debug;
use tokio::sync::mpsc::Receiver;

use crate::{LgTvManager, ManagerMessage, ManagerOutputMessage};

/// Build an [`LgTvManager`] instance.
///
/// ```
/// use std::path::Path;
///
/// use lgtv_manager::LgTvManagerBuilder;
/// use tokio::sync::mpsc;
///
/// let (to_manager_tx, to_manager_rx) = mpsc::channel(32);
///
/// let (mut manager, mut from_manager_rx) = LgTvManagerBuilder::new(to_manager_rx)
///     .with_data_dir(Path::new("/data/file/path/test").to_path_buf())
///     .build();
/// ```
pub struct LgTvManagerBuilder {
    manager: LgTvManager,
    out_channel: Receiver<ManagerOutputMessage>,
}

impl LgTvManagerBuilder {
    pub fn new(command_receiver: Receiver<ManagerMessage>) -> Self {
        debug!("Builder is instantiating an LgTvManager instance");
        let (manager, out_channel) = LgTvManager::new(command_receiver);

        LgTvManagerBuilder {
            manager,
            out_channel,
        }
    }

    /// Override the default persisted data directory (where manager data, such as the client key,
    /// are stored).
    pub fn with_data_dir(mut self, data_dir: PathBuf) -> Self {
        debug!("Builder is overriding data_dir: {:?}", data_dir);

        self.manager.data_dir = Some(data_dir);
        self.manager.clear_persisted_state_on_manager();
        self.manager.import_persisted_state();

        self
    }

    pub fn build(self) -> (LgTvManager, Receiver<ManagerOutputMessage>) {
        (self.manager, self.out_channel)
    }
}
