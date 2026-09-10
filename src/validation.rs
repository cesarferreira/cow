use std::{
    fs::File,
    os::unix::fs::MetadataExt,
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
    validate_paths_with_hook(source, destination, || {})
}

fn validate_paths_with_hook<F>(
    source: &Path,
    destination: &Path,
    before_source_open: F,
) -> Result<ValidatedPaths, CowError>
where
    F: FnOnce(),
{
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
    before_source_open();
    let source_root =
        crate::tree::open_directory_path_nofollow(&source).map_err(|error| match error {
            crate::tree::BackendError::Fatal(error) => error,
            crate::tree::BackendError::Unsupported(_) => {
                CowError::Unavailable("opening validated source directory")
            }
        })?;
    let opened_metadata = source_root
        .metadata()
        .map_err(|error| CowError::from_io("reading validated source identity", &source, error))?;
    if source_metadata.dev() != opened_metadata.dev()
        || source_metadata.ino() != opened_metadata.ino()
        || !opened_metadata.file_type().is_dir()
    {
        return Err(CowError::from_io(
            "verifying validated source identity",
            &source,
            std::io::Error::other("source changed during validation"),
        ));
    }
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

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn rejects_an_ancestor_swap_between_canonicalization_and_open() {
        let root = tempfile::tempdir().unwrap();
        let ancestor = root.path().join("ancestor");
        let original = root.path().join("original");
        let replacement = root.path().join("replacement");
        let source = ancestor.join("source");
        let destination = root.path().join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(replacement.join("source")).unwrap();

        let result = super::validate_paths_with_hook(&source, &destination, || {
            fs::rename(&ancestor, &original).unwrap();
            fs::rename(&replacement, &ancestor).unwrap();
        });

        assert!(result.is_err());
    }
}
