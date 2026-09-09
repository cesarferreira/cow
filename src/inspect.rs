use std::{
    ffi::CString,
    fs::{self, OpenOptions},
    io::Write,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use crate::{CloneStrategy, CowCapability, CowError, FilesystemInfo, platform, tree::BackendError};

pub(crate) fn inspect(path: &Path) -> Result<FilesystemInfo, CowError> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CowError::InvalidSource {
                path: path.to_path_buf(),
            }
        } else {
            CowError::Io {
                operation: "canonicalizing inspected path",
                path: path.to_path_buf(),
                source: error,
            }
        }
    })?;
    let probe_directory = if canonical.is_dir() {
        canonical.as_path()
    } else {
        canonical.parent().unwrap_or_else(|| Path::new("/"))
    };
    let cow_supported = probe_capability(probe_directory);
    let preferred_strategy = if cow_supported == CowCapability::Supported {
        native_strategy()
    } else {
        CloneStrategy::Copy
    };
    Ok(FilesystemInfo {
        path: canonical.clone(),
        platform: std::env::consts::OS.to_owned(),
        filesystem: filesystem_name(&canonical),
        cow_supported,
        preferred_strategy,
    })
}

fn native_strategy() -> CloneStrategy {
    #[cfg(target_os = "macos")]
    return CloneStrategy::ApfsClone;
    #[cfg(target_os = "linux")]
    return CloneStrategy::Reflink;
    #[allow(unreachable_code)]
    CloneStrategy::Copy
}

struct ProbeCleanup {
    source: PathBuf,
    destination: PathBuf,
}

impl Drop for ProbeCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.destination);
        let _ = fs::remove_file(&self.source);
    }
}

fn probe_capability(directory: &Path) -> CowCapability {
    for _ in 0..16 {
        let suffix = rand::random::<u64>();
        let source = directory.join(format!(".cow-probe-source-{suffix:016x}"));
        let destination = directory.join(format!(".cow-probe-destination-{suffix:016x}"));
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&source)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return CowCapability::Unknown,
        };
        let cleanup = ProbeCleanup {
            source,
            destination,
        };
        if file.write_all(b"cow capability probe").is_err() {
            return CowCapability::Unknown;
        }
        drop(file);
        let capability = match platform::clone_cow(&cleanup.source, &cleanup.destination) {
            Ok(_) => CowCapability::Supported,
            Err(BackendError::Unsupported) => CowCapability::Unavailable,
            Err(BackendError::Fatal(_)) => CowCapability::Unknown,
        };
        return capability;
    }
    CowCapability::Unknown
}

fn c_path(path: &Path) -> Option<CString> {
    CString::new(path.as_os_str().as_bytes()).ok()
}

#[cfg(target_os = "macos")]
fn filesystem_name(path: &Path) -> Option<String> {
    use std::ffi::CStr;

    let path = c_path(path)?;
    // SAFETY: `stat` is initialized by statfs on success, and `path` is NUL-terminated.
    unsafe {
        let mut stat: libc::statfs = std::mem::zeroed();
        if libc::statfs(path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        Some(
            CStr::from_ptr(stat.f_fstypename.as_ptr())
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(target_os = "linux")]
fn filesystem_name(path: &Path) -> Option<String> {
    let path = c_path(path)?;
    // SAFETY: `stat` is initialized by statfs on success, and `path` is NUL-terminated.
    unsafe {
        let mut stat: libc::statfs = std::mem::zeroed();
        if libc::statfs(path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        let name = match stat.f_type as libc::c_long {
            libc::BTRFS_SUPER_MAGIC => "btrfs".to_owned(),
            libc::XFS_SUPER_MAGIC => "xfs".to_owned(),
            libc::EXT4_SUPER_MAGIC => "ext4".to_owned(),
            libc::TMPFS_MAGIC => "tmpfs".to_owned(),
            other => format!("0x{other:x}"),
        };
        Some(name)
    }
}
