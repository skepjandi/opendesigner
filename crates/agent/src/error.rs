use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("random number generator failed: {0}")]
    Random(String),
    #[error("{0}")]
    Config(String),
    #[error("cannot start the server: {0}")]
    Server(String),
}

impl AgentError {
    pub fn io(path: &Path, source: std::io::Error) -> AgentError {
        AgentError::Io {
            path: path.display().to_string(),
            source,
        }
    }
}
