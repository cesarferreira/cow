use std::{
    ffi::CString,
    fs, io,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use crate::CowError;

pub(crate) struct DestinationGuard {
    destination: PathBuf,
    private_root: PathBuf,
    private: PathBuf,
    armed: bool,
}

impl DestinationGuard {
    pub(crate) fn new(destination: &Path) -> Result<Self, CowError> {
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        for _ in 0..32 {
            let private_root = parent.join(format!(".cow-tmp-{:016x}", rand::random::<u64>()));
            match fs::create_dir(&private_root) {
                Ok(()) => {
                    let guard = Self {
                        destination: destination.to_path_buf(),
                        private: private_root.join("tree"),
                        private_root,
                        armed: true,
                    };
                    fs::set_permissions(
                        &guard.private_root,
                        <fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
                    )
                    .map_err(|source| CowError::Io {
                        operation: "securing private destination",
                        path: guard.private_root.clone(),
                        source,
                    })?;
                    return Ok(guard);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(CowError::Io {
                        operation: "creating private destination",
                        path: private_root,
                        source,
                    });
                }
            }
        }
        Err(CowError::Io {
            operation: "choosing a private destination",
            path: parent.to_path_buf(),
            source: io::Error::new(io::ErrorKind::AlreadyExists, "temporary name collisions"),
        })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.private
    }

    pub(crate) fn reset(&mut self) -> Result<(), CowError> {
        remove_private(&self.private)?;
        Ok(())
    }

    pub(crate) fn commit(mut self) -> Result<(), CowError> {
        rename_noreplace(&self.private, &self.destination).map_err(|source| {
            if source.kind() == io::ErrorKind::AlreadyExists {
                CowError::DestinationExists {
                    path: self.destination.clone(),
                }
            } else {
                CowError::Io {
                    operation: "exposing completed destination",
                    path: self.destination.clone(),
                    source,
                }
            }
        })?;
        let _ = fs::remove_dir(&self.private_root);
        self.armed = false;
        Ok(())
    }
}

impl Drop for DestinationGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = remove_private(&self.private_root);
        }
    }
}

fn remove_private(path: &Path) -> Result<(), CowError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(CowError::Io {
                operation: "reading private destination",
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let result = if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    result.map_err(|source| CowError::Io {
        operation: "cleaning private destination",
        path: path.to_path_buf(),
        source,
    })
}

fn c_path(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains a NUL byte"))
}

#[cfg(target_os = "macos")]
fn rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    let source = c_path(source)?;
    let destination = c_path(destination)?;
    // SAFETY: Both pointers reference valid NUL-terminated path bytes for this call.
    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(target_os = "linux")]
fn rename_noreplace(source: &Path, destination: &Path) -> io::Result<()> {
    let source = c_path(source)?;
    let destination = c_path(destination)?;
    // SAFETY: Both pointers reference valid NUL-terminated path bytes for this call.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::DestinationGuard;

    #[test]
    fn drop_removes_a_partial_directory() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("clone");
        let private;
        {
            let guard = DestinationGuard::new(&destination).unwrap();
            private = guard.path().to_path_buf();
            fs::create_dir(&private).unwrap();
        }
        assert!(!private.exists());
        assert!(!destination.exists());
    }

    #[test]
    fn guard_owns_a_private_parent_before_cloning() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("clone");
        let guard = DestinationGuard::new(&destination).unwrap();
        let private_parent = guard.path().parent().unwrap();
        assert!(private_parent.is_dir());
        assert!(
            private_parent
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".cow-tmp-")
        );
        assert!(!guard.path().exists());
    }

    #[test]
    fn commit_never_replaces_a_racing_destination() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("clone");
        let guard = DestinationGuard::new(&destination).unwrap();
        fs::create_dir(guard.path()).unwrap();
        fs::create_dir(&destination).unwrap();
        assert!(guard.commit().is_err());
        assert!(destination.exists());
    }
}
