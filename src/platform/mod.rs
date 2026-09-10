#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use std::path::Path;

use crate::{
    CloneStrategy,
    tree::{BackendError, TreeStats, UnsupportedReason},
};

pub(crate) fn clone_cow(
    source: &Path,
    destination: &Path,
) -> Result<(CloneStrategy, TreeStats), BackendError> {
    #[cfg(target_os = "macos")]
    return macos::clone_cow(source, destination);

    #[cfg(target_os = "linux")]
    return linux::clone_cow(source, destination);

    #[allow(unreachable_code)]
    Err(BackendError::Unsupported(UnsupportedReason::Unavailable))
}
