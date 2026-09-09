use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    path::Path,
};

use crate::{
    CloneStrategy,
    tree::{BackendError, TreeStats, clone_tree, io_error},
};

pub(super) fn clone_cow(
    source: &Path,
    destination: &Path,
) -> Result<(CloneStrategy, TreeStats), BackendError> {
    let stats = clone_tree(source, destination, &mut reflink_file)?;
    Ok((CloneStrategy::Reflink, stats))
}

fn reflink_file(source: &Path, destination: &Path) -> Result<(), BackendError> {
    let input =
        File::open(source).map_err(|error| io_error("opening source file", source, error))?;
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| io_error("creating destination file", destination, error))?;
    // SAFETY: Both file descriptors are valid for the duration of the ioctl.
    let result = unsafe { libc::ioctl(output.as_raw_fd(), libc::FICLONE, input.as_raw_fd()) };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::EOPNOTSUPP | libc::ENOTTY | libc::EXDEV | libc::EINVAL) => {
            Err(BackendError::Unsupported)
        }
        _ => Err(io_error("cloning file with FICLONE", destination, error)),
    }
}
