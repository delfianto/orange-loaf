use std::sync::Mutex;

use tokio::sync::oneshot;

use crate::config::Config;
use crate::history::History;
use crate::profiles::Profiles;
use crate::queue::Queue;

/// Shared, mutable application state, managed by Tauri and accessed from
/// commands via `tauri::State`.
pub struct AppState {
    pub config: Mutex<Config>,
    /// `Queue::paused` (not a separate flag here) is the single source of
    /// truth for whether dispatch should start new jobs — see
    /// `queue::pause_queue`'s doc comment for why pausing isn't "pause the
    /// current encode."
    pub queue: Mutex<Queue>,
    /// The currently-running job's id, paired with the sender that cancels
    /// it — `None` whenever nothing is running. Only ever one entry since
    /// the queue runs strictly sequentially.
    pub running_cancel: Mutex<Option<(String, oneshot::Sender<()>)>>,
    pub profiles: Mutex<Profiles>,
    pub history: Mutex<History>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(Config::load()),
            queue: Mutex::new(Queue::load()),
            running_cancel: Mutex::new(None),
            profiles: Mutex::new(Profiles::load()),
            history: Mutex::new(History::load()),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
