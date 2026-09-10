use std::{
    ffi::CString,
    fs::File,
    io,
    os::{fd::AsRawFd, unix::ffi::OsStrExt},
    path::Path,
};

use crate::{
    CloneStrategy,
    tree::{BackendError, TreeStats, UnsupportedReason, io_error, measure_tree},
};

const CLONE_NOFOLLOW: u32 = 0x0001;

pub(super) fn clone_cow(
    source: &File,
    _source_display: &Path,
    destination: &Path,
) -> Result<(CloneStrategy, TreeStats), BackendError> {
    let destination_c = c_path(destination)
        .map_err(|error| io_error("encoding destination path", destination, error))?;
    // SAFETY: The source descriptor is retained from validation and the destination path is NUL-terminated.
    let result = unsafe {
        libc::fclonefileat(
            source.as_raw_fd(),
            libc::AT_FDCWD,
            destination_c.as_ptr(),
            CLONE_NOFOLLOW,
        )
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted && crate::cancellation::requested() {
            return Err(crate::CowError::Cancelled.into());
        }
        return match error.raw_os_error() {
            Some(libc::ENOTSUP) => Err(BackendError::Unsupported(UnsupportedReason::Unavailable)),
            Some(libc::EXDEV) => Err(BackendError::Unsupported(UnsupportedReason::CrossDevice)),
            _ => Err(io_error(
                "cloning directory with clonefile",
                destination,
                error,
            )),
        };
    }
    if crate::cancellation::requested() {
        return Err(crate::CowError::Cancelled.into());
    }
    Ok((CloneStrategy::ApfsClone, measure_tree(destination)?))
}

fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a NUL byte"))
}
