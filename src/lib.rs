mod cancellation;
mod error;
mod inspect;
mod platform;
mod transaction;
mod tree;
mod types;
mod validation;

use std::{fs::File, path::Path, time::Instant};

pub use error::CowError;
pub use types::{
    CloneOptions, CloneResult, CloneStrategy, CowCapability, FilesystemInfo, StrategyPreference,
};

#[doc(hidden)]
pub fn install_interrupt_handler() -> Result<(), CowError> {
    cancellation::install().map_err(|source| {
        CowError::from_io(
            "installing interrupt handler",
            std::path::Path::new(""),
            source,
        )
    })
}

pub fn inspect(path: impl AsRef<Path>) -> Result<FilesystemInfo, CowError> {
    inspect::inspect(path.as_ref())
}

pub fn clone_dir(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    options: CloneOptions,
) -> Result<CloneResult, CowError> {
    let paths = validation::validate_paths(source.as_ref(), destination.as_ref())?;
    clone_validated_with(paths, options, platform::clone_cow)
}

fn clone_validated_with<F>(
    paths: validation::ValidatedPaths,
    options: CloneOptions,
    mut cow_backend: F,
) -> Result<CloneResult, CowError>
where
    F: FnMut(&File, &Path, &Path) -> Result<(CloneStrategy, tree::TreeStats), tree::BackendError>,
{
    let started = Instant::now();
    let mut guard = transaction::DestinationGuard::new(&paths.destination)?;
    let (strategy, stats) = match options.strategy {
        StrategyPreference::Copy => (
            CloneStrategy::Copy,
            tree::copy_tree(&paths.source_root, &paths.source, guard.path())?,
        ),
        StrategyPreference::Cow | StrategyPreference::Auto => {
            match cow_backend(&paths.source_root, &paths.source, guard.path()) {
                Ok(result) => result,
                Err(tree::BackendError::Unsupported(_))
                    if options.strategy == StrategyPreference::Auto =>
                {
                    guard.reset()?;
                    (
                        CloneStrategy::Copy,
                        tree::copy_tree(&paths.source_root, &paths.source, guard.path())?,
                    )
                }
                Err(tree::BackendError::Unsupported(reason)) => {
                    return Err(match reason {
                        tree::UnsupportedReason::Unavailable => CowError::CowUnsupported {
                            source_path: paths.source,
                            destination_path: paths.destination,
                        },
                        tree::UnsupportedReason::CrossDevice => CowError::CrossDevice {
                            source_path: paths.source,
                            destination_path: paths.destination,
                        },
                    });
                }
                Err(tree::BackendError::Fatal(error)) => return Err(error),
            }
        }
    };
    guard.commit()?;
    Ok(CloneResult {
        source: paths.source,
        destination: paths.destination,
        strategy,
        logical_bytes: stats.logical_bytes,
        files: stats.files,
        duration: started.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::{
        CloneOptions, CloneStrategy, StrategyPreference,
        tree::{BackendError, UnsupportedReason},
    };

    #[test]
    fn auto_restarts_cleanly_after_a_partial_unsupported_backend() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("data"), "complete").unwrap();
        let paths = crate::validation::validate_paths(&source, &destination).unwrap();

        let result =
            super::clone_validated_with(paths, CloneOptions::default(), |_, _, private| {
                fs::create_dir(private).unwrap();
                fs::write(private.join("partial"), "partial").unwrap();
                Err(BackendError::Unsupported(UnsupportedReason::Unavailable))
            })
            .unwrap();

        assert_eq!(result.strategy, CloneStrategy::Copy);
        assert_eq!(
            fs::read_to_string(destination.join("data")).unwrap(),
            "complete"
        );
        assert!(!destination.join("partial").exists());
        assert!(fs::read_dir(root.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".cow-tmp-")
        }));
    }

    #[test]
    fn required_cow_preserves_cross_device_as_a_typed_error() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        fs::create_dir(&source).unwrap();
        let paths = crate::validation::validate_paths(&source, &destination).unwrap();
        let error = super::clone_validated_with(
            paths,
            CloneOptions {
                strategy: StrategyPreference::Cow,
            },
            |_, _, _| Err(BackendError::Unsupported(UnsupportedReason::CrossDevice)),
        )
        .unwrap_err();
        assert!(matches!(error, crate::CowError::CrossDevice { .. }));
    }

    #[test]
    fn auto_does_not_fallback_after_a_fatal_backend_error() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        fs::create_dir(&source).unwrap();
        let paths = crate::validation::validate_paths(&source, &destination).unwrap();
        let error = super::clone_validated_with(paths, CloneOptions::default(), |_, _, _| {
            Err(BackendError::Fatal(crate::CowError::Cancelled))
        })
        .unwrap_err();
        assert!(matches!(error, crate::CowError::Cancelled));
        assert!(!destination.exists());
    }

    #[test]
    fn validated_source_survives_an_ancestor_symlink_swap() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let ancestor = root.path().join("ancestor");
        let moved = root.path().join("moved");
        let outside = root.path().join("outside");
        let source = ancestor.join("source");
        let destination = root.path().join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("data"), "original").unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("data"), "outside").unwrap();
        let paths = crate::validation::validate_paths(&source, &destination).unwrap();

        fs::rename(&ancestor, &moved).unwrap();
        symlink(&outside, &ancestor).unwrap();
        let result = super::clone_validated_with(
            paths,
            CloneOptions {
                strategy: StrategyPreference::Copy,
            },
            |_, _, _| unreachable!(),
        )
        .unwrap();

        assert_eq!(result.strategy, CloneStrategy::Copy);
        assert_eq!(
            fs::read_to_string(destination.join("data")).unwrap(),
            "original"
        );
    }
}
