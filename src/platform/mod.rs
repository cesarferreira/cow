#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

use std::{fs::File, path::Path};

use crate::{
    CloneStrategy,
    tree::{BackendError, TreeStats, UnsupportedReason},
};

pub(crate) fn clone_cow(
    source: &File,
    source_display: &Path,
    destination: &Path,
) -> Result<(CloneStrategy, TreeStats), BackendError> {
    #[cfg(target_os = "macos")]
    return macos::clone_cow(source, source_display, destination);

    #[cfg(target_os = "linux")]
    return linux::clone_cow(source, source_display, destination);

    #[allow(unreachable_code)]
    Err(BackendError::Unsupported(UnsupportedReason::Unavailable))
}
