use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::fd::AsRawFd,
    path::Path,
};

use crate::{
    CloneStrategy,
    tree::{BackendError, TreeStats, UnsupportedReason, clone_tree, io_error},
};

pub(super) fn clone_cow(
    source: &File,
    source_display: &Path,
    destination: &Path,
) -> Result<(CloneStrategy, TreeStats), BackendError> {
    ensure_same_filesystem(source, destination)?;
    probe_reflink(destination)?;
    let stats = clone_tree(source, source_display, destination, &mut reflink_file)?;
    Ok((CloneStrategy::Reflink, stats))
}

struct ProbeCleanup {
    directory: std::path::PathBuf,
}

impl Drop for ProbeCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
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
    let probe_directory = (0..16)
        .find_map(|_| {
            let candidate =
                parent.join(format!(".cow-reflink-probe-{:016x}", rand::random::<u64>()));
            match fs::create_dir(&candidate) {
                Ok(()) => Some(Ok(candidate)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(io_error(
                    "creating private reflink probe directory",
                    &candidate,
                    error,
                ))),
            }
        })
        .unwrap_or_else(|| {
            Err(io_error(
                "creating private reflink probe directory",
                parent,
                std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "could not allocate a unique probe directory",
                ),
            ))
        })?;
    let cleanup = ProbeCleanup {
        directory: probe_directory,
    };
    let source = cleanup.directory.join("source");
    let clone = cleanup.directory.join("destination");
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

fn ensure_same_filesystem(source: &File, destination: &Path) -> Result<(), BackendError> {
    use std::os::unix::fs::MetadataExt;
    let parent = destination.parent().ok_or_else(|| {
        io_error(
            "locating destination parent",
            destination,
            std::io::Error::other("destination has no parent"),
        )
    })?;
    let source_device = source
        .metadata()
        .map_err(|error| io_error("reading source filesystem", destination, error))?
        .dev();
    let destination_device = fs::metadata(parent)
        .map_err(|error| io_error("reading destination filesystem", parent, error))?
        .dev();
    if source_device != destination_device {
        return Err(BackendError::Unsupported(UnsupportedReason::CrossDevice));
    }
    Ok(())
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
