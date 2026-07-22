use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("VM '{0}' not found")]
    VmNotFound(String),

    #[error("VM '{0}' already exists")]
    VmExists(String),

    #[error("VM '{0}' is not running")]
    NotRunning(String),

    #[error("VM '{0}' is already running")]
    AlreadyRunning(String),

    #[error("invalid VM name '{0}': use [a-zA-Z0-9._-] only")]
    InvalidName(String),

    #[error("QMP protocol error: {0}")]
    Qmp(String),

    #[error("QMP command '{command}' failed: {class}: {desc}")]
    QmpCommand {
        command: String,
        class: String,
        desc: String,
    },

    #[error("qemu-img failed: {0}")]
    QemuImg(String),

    #[error("failed to launch QEMU ({binary}): {source}")]
    Launch {
        binary: String,
        #[source]
        source: std::io::Error,
    },

    #[error("QEMU exited during startup: {0}")]
    EarlyExit(String),

    #[error("timed out waiting for QMP socket at {0}")]
    QmpTimeout(PathBuf),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}
