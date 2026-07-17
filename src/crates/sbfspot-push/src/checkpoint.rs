use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkpoint {
    pub last_timestamp: i64,
}

#[derive(Clone, Debug)]
pub struct CheckpointStore {
    path: PathBuf,
}

impl CheckpointStore {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Loads the checkpoint, returning `None` when it has not been created yet.
    ///
    /// # Errors
    /// Returns an error when the file cannot be read or decoded.
    pub async fn load(&self) -> Result<Option<Checkpoint>, CheckpointError> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => {
                serde_json::from_slice(&bytes)
                    .map(Some)
                    .map_err(|source| CheckpointError::Decode {
                        path: self.path.clone(),
                        source,
                    })
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(CheckpointError::Read {
                path: self.path.clone(),
                source,
            }),
        }
    }

    /// Atomically replaces the checkpoint after a confirmed upload.
    ///
    /// # Errors
    /// Returns an error when its directory or file cannot be written.
    pub async fn save(&self, checkpoint: Checkpoint) -> Result<(), CheckpointError> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|source| CheckpointError::Write {
                    path: self.path.clone(),
                    source,
                })?;
        }
        let temporary = temporary_path(&self.path);
        let contents = serde_json::to_vec(&checkpoint).map_err(CheckpointError::Encode)?;
        tokio::fs::write(&temporary, contents)
            .await
            .map_err(|source| CheckpointError::Write {
                path: temporary.clone(),
                source,
            })?;
        tokio::fs::rename(&temporary, &self.path)
            .await
            .map_err(|source| CheckpointError::Write {
                path: self.path.clone(),
                source,
            })
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

#[derive(Debug, Error)]
pub enum CheckpointError {
    #[error("failed to read checkpoint {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to decode checkpoint {path}: {source}")]
    Decode {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to encode checkpoint: {0}")]
    Encode(serde_json::Error),
    #[error("failed to write checkpoint {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}
