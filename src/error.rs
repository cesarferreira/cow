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
    #[error("source and destination are on different filesystems: {source_path} -> {destination_path}", source_path = .source_path.display(), destination_path = .destination_path.display())]
    CrossDevice {
        source_path: PathBuf,
        destination_path: PathBuf,
    },
    #[error("permission denied while {operation} at {path}: {source}", path = .path.display())]
    PermissionDenied {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("insufficient space while {operation} at {path}: {source}", path = .path.display())]
    InsufficientSpace {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
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
            Self::CrossDevice { .. } => "cross_device",
            Self::PermissionDenied { .. } => "permission_denied",
            Self::InsufficientSpace { .. } => "insufficient_space",
            Self::UnsupportedFileType { .. } => "unsupported_file_type",
            Self::Cancelled => "cancelled",
            Self::Unavailable(_) => "unavailable",
            Self::Io { .. } => "io",
        }
    }

    pub(crate) fn from_io(
        operation: &'static str,
        path: &std::path::Path,
        source: io::Error,
    ) -> Self {
        if source.raw_os_error() == Some(libc::ENOSPC) {
            Self::InsufficientSpace {
                operation,
                path: path.to_path_buf(),
                source,
            }
        } else if source.kind() == io::ErrorKind::PermissionDenied {
            Self::PermissionDenied {
                operation,
                path: path.to_path_buf(),
                source,
            }
        } else {
            Self::Io {
                operation,
                path: path.to_path_buf(),
                source,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{io, path::Path};

    use super::CowError;

    #[test]
    fn maps_permission_denied_to_a_typed_error() {
        let error = CowError::from_io(
            "opening source",
            Path::new("source"),
            io::Error::from(io::ErrorKind::PermissionDenied),
        );
        assert!(matches!(error, CowError::PermissionDenied { .. }));
    }

    #[test]
    fn maps_enospc_to_a_typed_error() {
        let error = CowError::from_io(
            "writing destination",
            Path::new("destination"),
            io::Error::from_raw_os_error(libc::ENOSPC),
        );
        assert!(matches!(error, CowError::InsufficientSpace { .. }));
    }
}
