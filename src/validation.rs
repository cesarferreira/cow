use std::{
    fs::File,
    path::{Path, PathBuf},
};

use crate::CowError;

pub(crate) struct ValidatedPaths {
    pub source: PathBuf,
    pub source_root: File,
    pub destination: PathBuf,
}

pub(crate) fn validate_paths(
    source: &Path,
    destination: &Path,
) -> Result<ValidatedPaths, CowError> {
    let source_metadata = std::fs::symlink_metadata(source).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CowError::InvalidSource {
                path: source.to_path_buf(),
            }
        } else {
            CowError::from_io("reading source metadata", source, error)
        }
    })?;
    if !source_metadata.file_type().is_dir() {
        return Err(CowError::SourceNotDirectory {
            path: source.to_path_buf(),
        });
    }

    if std::fs::symlink_metadata(destination).is_ok() {
        return Err(CowError::DestinationExists {
            path: destination.to_path_buf(),
        });
    }

    let source = std::fs::canonicalize(source)
        .map_err(|source_error| CowError::from_io("canonicalizing source", source, source_error))?;
    let source_root =
        crate::tree::open_directory_path_nofollow(&source).map_err(|error| match error {
            crate::tree::BackendError::Fatal(error) => error,
            crate::tree::BackendError::Unsupported(_) => {
                CowError::Unavailable("opening validated source directory")
            }
        })?;
    let file_name = destination
        .file_name()
        .ok_or_else(|| CowError::DestinationExists {
            path: destination.to_path_buf(),
        })?;
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let parent = std::fs::canonicalize(parent).map_err(|source_error| {
        CowError::from_io("canonicalizing destination parent", parent, source_error)
    })?;
    let destination = parent.join(file_name);

    if destination.starts_with(&source) || source.starts_with(&destination) {
        return Err(CowError::OverlappingPaths {
            source_path: source,
            destination_path: destination,
        });
    }

    Ok(ValidatedPaths {
        source,
        source_root,
        destination,
    })
}
