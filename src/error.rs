use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CowError {
    #[error("source does not exist: {path}", path = .path.display())]
    InvalidSource { path: PathBuf },
    #[error("source is not a directory: {path}", path = .path.display())]
    SourceNotDirectory { path: PathBuf },
    #[error("destination already exists: {path}", path = .path.display())]
    DestinationExists { path: PathBuf },
    #[error("source and destination overlap: {source_path} -> {destination_path}", source_path = .source_path.display(), destination_path = .destination_path.display())]
    OverlappingPaths {
        source_path: PathBuf,
        destination_path: PathBuf,
    },
    #[error("copy-on-write cloning is unavailable for {source_path} -> {destination_path}", source_path = .source_path.display(), destination_path = .destination_path.display())]
    CowUnsupported {
        source_path: PathBuf,
        destination_path: PathBuf,
    },
    #[error("unsupported file type: {path}", path = .path.display())]
    UnsupportedFileType { path: PathBuf },
    #[error("clone cancelled")]
    Cancelled,
    #[error("{0}")]
    Unavailable(&'static str),
    #[error("I/O error while {operation} at {path}: {source}", path = .path.display())]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl CowError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidSource { .. } => "invalid_source",
            Self::SourceNotDirectory { .. } => "source_not_directory",
            Self::DestinationExists { .. } => "destination_exists",
            Self::OverlappingPaths { .. } => "overlapping_paths",
            Self::CowUnsupported { .. } => "cow_unsupported",
            Self::UnsupportedFileType { .. } => "unsupported_file_type",
            Self::Cancelled => "cancelled",
            Self::Unavailable(_) => "unavailable",
            Self::Io { source, .. } if source.kind() == io::ErrorKind::PermissionDenied => {
                "permission_denied"
            }
            Self::Io { .. } => "io",
        }
    }
}
