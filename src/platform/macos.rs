use std::{ffi::CString, io, os::unix::ffi::OsStrExt, path::Path};

use crate::{
    CloneStrategy,
    tree::{BackendError, TreeStats, io_error, measure_tree},
};

const CLONE_NOFOLLOW: u32 = 0x0001;

pub(super) fn clone_cow(
    source: &Path,
    destination: &Path,
) -> Result<(CloneStrategy, TreeStats), BackendError> {
    let source_c = c_path(source).map_err(|error| io_error("encoding source path", source, error))?;
    let destination_c = c_path(destination)
        .map_err(|error| io_error("encoding destination path", destination, error))?;
    // SAFETY: Both pointers contain valid, NUL-terminated path bytes for the duration of the call.
    let result = unsafe { libc::clonefile(source_c.as_ptr(), destination_c.as_ptr(), CLONE_NOFOLLOW) };
    if result != 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::Interrupted && crate::cancellation::requested() {
            return Err(crate::CowError::Cancelled.into());
        }
        return match error.raw_os_error() {
            Some(libc::ENOTSUP | libc::EXDEV) => Err(BackendError::Unsupported),
            _ => Err(io_error("cloning directory with clonefile", destination, error)),
        };
    }
    Ok((CloneStrategy::ApfsClone, measure_tree(source)?))
}

fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a NUL byte"))
}
