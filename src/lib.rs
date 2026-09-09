mod error;
mod inspect;
mod platform;
mod transaction;
mod tree;
mod types;
mod validation;

use std::{path::Path, time::Instant};

pub use error::CowError;
pub use types::{
    CloneOptions, CloneResult, CloneStrategy, CowCapability, FilesystemInfo, StrategyPreference,
};

pub fn inspect(path: impl AsRef<Path>) -> Result<FilesystemInfo, CowError> {
    inspect::inspect(path.as_ref())
}

pub fn clone_dir(
    source: impl AsRef<Path>,
    destination: impl AsRef<Path>,
    options: CloneOptions,
) -> Result<CloneResult, CowError> {
    let paths = validation::validate_paths(source.as_ref(), destination.as_ref())?;
    let started = Instant::now();
    let mut guard = transaction::DestinationGuard::new(&paths.destination)?;
    let (strategy, stats) = match options.strategy {
        StrategyPreference::Copy => (
            CloneStrategy::Copy,
            tree::copy_tree(&paths.source, guard.path())?,
        ),
        StrategyPreference::Cow | StrategyPreference::Auto => {
            match platform::clone_cow(&paths.source, guard.path()) {
                Ok(result) => result,
                Err(tree::BackendError::Unsupported)
                    if options.strategy == StrategyPreference::Auto =>
                {
                    guard.reset()?;
                    (
                        CloneStrategy::Copy,
                        tree::copy_tree(&paths.source, guard.path())?,
                    )
                }
                Err(tree::BackendError::Unsupported) => {
                    return Err(CowError::CowUnsupported {
                        source_path: paths.source,
                        destination_path: paths.destination,
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
