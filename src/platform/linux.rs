use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::fd::AsRawFd,
    path::Path,
};

use crate::{
    CloneStrategy,
    tree::{BackendError, TreeStats, UnsupportedReason, clone_tree, io_error},
};

pub(super) fn clone_cow(
    source: &Path,
    destination: &Path,
) -> Result<(CloneStrategy, TreeStats), BackendError> {
    probe_reflink(destination)?;
    let stats = clone_tree(source, destination, &mut reflink_file)?;
    Ok((CloneStrategy::Reflink, stats))
}

struct ProbeCleanup<'a> {
    source: &'a Path,
    destination: &'a Path,
}

impl Drop for ProbeCleanup<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.destination);
        let _ = std::fs::remove_file(self.source);
    }
}

fn probe_reflink(destination: &Path) -> Result<(), BackendError> {
    let parent = destination.parent().ok_or_else(|| {
        io_error(
            "locating reflink probe directory",
            destination,
            std::io::Error::other("destination has no parent"),
        )
    })?;
    let source = parent.join("reflink-probe-source");
    let clone = parent.join("reflink-probe-destination");
    let cleanup = ProbeCleanup {
        source: &source,
        destination: &clone,
    };
    let mut source_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&source)
        .map_err(|error| io_error("creating reflink probe source", &source, error))?;
    source_file
        .write_all(b"cow reflink probe")
        .map_err(|error| io_error("writing reflink probe source", &source, error))?;
    drop(source_file);
    let input = File::open(&source)
        .map_err(|error| io_error("opening reflink probe source", &source, error))?;
    let result = reflink_file(&source, &input, &clone);
    drop(cleanup);
    result
}

fn reflink_file(_source: &Path, input: &File, destination: &Path) -> Result<(), BackendError> {
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
        Some(libc::EXDEV) => Err(BackendError::Unsupported(UnsupportedReason::CrossDevice)),
        Some(libc::EOPNOTSUPP | libc::ENOTTY | libc::EINVAL) => {
            Err(BackendError::Unsupported(UnsupportedReason::Unavailable))
        }
        _ => Err(io_error("cloning file with FICLONE", destination, error)),
    }
}
