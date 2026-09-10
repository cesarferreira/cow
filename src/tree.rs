use std::{
    ffi::{CStr, CString, OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::{
            ffi::{OsStrExt, OsStringExt},
            fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        },
    },
    path::Path,
};

use filetime::{FileTime, set_file_times, set_symlink_file_times};

use crate::CowError;

#[cfg(target_os = "macos")]
const FILE_TYPE_MASK: u32 = libc::S_IFMT as u32;
#[cfg(target_os = "linux")]
const FILE_TYPE_MASK: u32 = libc::S_IFMT;
#[cfg(target_os = "macos")]
const DIRECTORY_TYPE: u32 = libc::S_IFDIR as u32;
#[cfg(target_os = "linux")]
const DIRECTORY_TYPE: u32 = libc::S_IFDIR;
#[cfg(target_os = "macos")]
const REGULAR_TYPE: u32 = libc::S_IFREG as u32;
#[cfg(target_os = "linux")]
const REGULAR_TYPE: u32 = libc::S_IFREG;
#[cfg(target_os = "macos")]
const SYMLINK_TYPE: u32 = libc::S_IFLNK as u32;
#[cfg(target_os = "linux")]
const SYMLINK_TYPE: u32 = libc::S_IFLNK;

#[derive(Debug, Default)]
pub(crate) struct TreeStats {
    pub logical_bytes: u64,
    pub files: u64,
}

#[derive(Debug)]
pub(crate) enum BackendError {
    Unsupported(UnsupportedReason),
    Fatal(CowError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UnsupportedReason {
    Unavailable,
    CrossDevice,
}

impl From<CowError> for BackendError {
    fn from(value: CowError) -> Self {
        Self::Fatal(value)
    }
}

#[derive(Clone)]
struct EntryMetadata {
    device: u64,
    inode: u64,
    mode: u32,
    len: u64,
    accessed: FileTime,
    modified: FileTime,
}

impl EntryMetadata {
    fn from_std(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            len: metadata.len(),
            accessed: FileTime::from_last_access_time(metadata),
            modified: FileTime::from_last_modification_time(metadata),
        }
    }

    fn kind(&self) -> u32 {
        self.mode & FILE_TYPE_MASK
    }
}

pub(crate) fn copy_tree(source: &Path, destination: &Path) -> Result<TreeStats, CowError> {
    clone_tree(source, destination, &mut copy_file).map_err(|error| match error {
        BackendError::Fatal(error) => error,
        BackendError::Unsupported(_) => CowError::Unavailable("regular copy reported unsupported"),
    })
}

#[cfg(target_os = "macos")]
pub(crate) fn measure_tree(source: &Path) -> Result<TreeStats, BackendError> {
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| io_error("reading entry metadata", source, error))?;
    let mut stats = TreeStats::default();
    if metadata.file_type().is_file() {
        stats.files = 1;
        stats.logical_bytes = metadata.len();
    } else if metadata.file_type().is_dir() {
        for entry in
            fs::read_dir(source).map_err(|error| io_error("reading directory", source, error))?
        {
            let entry =
                entry.map_err(|error| io_error("reading directory entry", source, error))?;
            let child = measure_tree(&entry.path())?;
            stats.files += child.files;
            stats.logical_bytes += child.logical_bytes;
        }
    } else if !metadata.file_type().is_symlink() {
        return Err(CowError::UnsupportedFileType {
            path: source.to_path_buf(),
        }
        .into());
    }
    Ok(stats)
}

pub(crate) fn clone_tree<F>(
    source: &Path,
    destination: &Path,
    regular_file: &mut F,
) -> Result<TreeStats, BackendError>
where
    F: FnMut(&Path, &File, &Path) -> Result<(), BackendError>,
{
    if crate::cancellation::requested() {
        return Err(CowError::Cancelled.into());
    }
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| io_error("reading source metadata", source, error))?;
    let mut stats = TreeStats::default();
    if metadata.is_dir() {
        let (directory, opened) = open_directory_nofollow(source)?;
        ensure_same_entry(source, &metadata, &opened)?;
        fs::create_dir(destination)
            .map_err(|error| io_error("creating directory", destination, error))?;
        clone_open_directory(&directory, source, destination, regular_file, &mut stats)?;
        restore_metadata(destination, &EntryMetadata::from_std(&opened))?;
    } else if metadata.is_file() {
        let (input, opened) = open_regular_nofollow(source)?;
        ensure_same_entry(source, &metadata, &opened)?;
        regular_file(source, &input, destination)?;
        let opened = EntryMetadata::from_std(&opened);
        restore_metadata(destination, &opened)?;
        stats.files = 1;
        stats.logical_bytes = opened.len;
    } else {
        return Err(CowError::UnsupportedFileType {
            path: source.to_path_buf(),
        }
        .into());
    }
    Ok(stats)
}

fn clone_open_directory<F>(
    directory: &File,
    source_display: &Path,
    destination: &Path,
    regular_file: &mut F,
    stats: &mut TreeStats,
) -> Result<(), BackendError>
where
    F: FnMut(&Path, &File, &Path) -> Result<(), BackendError>,
{
    for name in directory_names(directory, source_display)? {
        if crate::cancellation::requested() {
            return Err(CowError::Cancelled.into());
        }
        let source_entry = source_display.join(&name);
        let destination_entry = destination.join(&name);
        let before = metadata_at(directory.as_raw_fd(), &name, &source_entry)?;
        match before.kind() {
            DIRECTORY_TYPE => {
                let child = open_directory_at(directory.as_raw_fd(), &name, &source_entry)?;
                let opened = EntryMetadata::from_std(&child.metadata().map_err(|error| {
                    io_error("reading opened directory metadata", &source_entry, error)
                })?);
                ensure_same_raw_entry(&source_entry, &before, &opened)?;
                fs::create_dir(&destination_entry)
                    .map_err(|error| io_error("creating directory", &destination_entry, error))?;
                clone_open_directory(
                    &child,
                    &source_entry,
                    &destination_entry,
                    regular_file,
                    stats,
                )?;
                restore_metadata(&destination_entry, &opened)?;
            }
            REGULAR_TYPE => {
                let input = open_regular_at(directory.as_raw_fd(), &name, &source_entry)?;
                let opened = EntryMetadata::from_std(&input.metadata().map_err(|error| {
                    io_error("reading opened file metadata", &source_entry, error)
                })?);
                ensure_same_raw_entry(&source_entry, &before, &opened)?;
                regular_file(&source_entry, &input, &destination_entry)?;
                restore_metadata(&destination_entry, &opened)?;
                stats.files += 1;
                stats.logical_bytes += opened.len;
            }
            SYMLINK_TYPE => {
                let target = read_link_at(directory.as_raw_fd(), &name, &source_entry)?;
                let after = metadata_at(directory.as_raw_fd(), &name, &source_entry)?;
                ensure_same_raw_entry(&source_entry, &before, &after)?;
                std::os::unix::fs::symlink(target, &destination_entry)
                    .map_err(|error| io_error("creating symlink", &destination_entry, error))?;
                set_symlink_file_times(&destination_entry, before.accessed, before.modified)
                    .map_err(|error| {
                        io_error("restoring symlink timestamps", &destination_entry, error)
                    })?;
            }
            _ => {
                return Err(CowError::UnsupportedFileType { path: source_entry }.into());
            }
        }
    }
    Ok(())
}

fn copy_file(source: &Path, input: &File, destination: &Path) -> Result<(), BackendError> {
    let mut input = input;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .map_err(|error| io_error("creating destination file", destination, error))?;
    let mut buffer = vec![0_u8; 128 * 1024];
    loop {
        if crate::cancellation::requested() {
            return Err(CowError::Cancelled.into());
        }
        let read = input
            .read(&mut buffer)
            .map_err(|error| io_error("reading source file", source, error))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| io_error("writing destination file", destination, error))?;
    }
    Ok(())
}

pub(crate) fn open_regular_nofollow(path: &Path) -> Result<(File, fs::Metadata), BackendError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| {
            io_error(
                "opening source file without following symlinks",
                path,
                error,
            )
        })?;
    let metadata = file
        .metadata()
        .map_err(|error| io_error("reading opened file metadata", path, error))?;
    if !metadata.is_file() {
        return Err(changed_entry(path));
    }
    Ok((file, metadata))
}

pub(crate) fn open_directory_nofollow(path: &Path) -> Result<(File, fs::Metadata), BackendError> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|error| {
            io_error(
                "opening source directory without following symlinks",
                path,
                error,
            )
        })?;
    let metadata = file
        .metadata()
        .map_err(|error| io_error("reading opened directory metadata", path, error))?;
    if !metadata.is_dir() {
        return Err(changed_entry(path));
    }
    Ok((file, metadata))
}

fn open_directory_at(parent: RawFd, name: &OsStr, display: &Path) -> Result<File, BackendError> {
    open_at(
        parent,
        name,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        display,
    )
}

fn open_regular_at(parent: RawFd, name: &OsStr, display: &Path) -> Result<File, BackendError> {
    open_at(
        parent,
        name,
        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        display,
    )
}

fn open_at(parent: RawFd, name: &OsStr, flags: i32, display: &Path) -> Result<File, BackendError> {
    let name = c_name(name, display)?;
    // SAFETY: `parent` is open, `name` is NUL-terminated, and ownership of a successful fd moves to File.
    let descriptor = unsafe { libc::openat(parent, name.as_ptr(), flags) };
    if descriptor < 0 {
        return Err(io_error(
            "opening source entry",
            display,
            io::Error::last_os_error(),
        ));
    }
    // SAFETY: `openat` returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn metadata_at(parent: RawFd, name: &OsStr, display: &Path) -> Result<EntryMetadata, BackendError> {
    let name = c_name(name, display)?;
    // SAFETY: `stat` is initialized by fstatat on success, and inputs are valid for this call.
    unsafe {
        let mut stat: libc::stat = std::mem::zeroed();
        if libc::fstatat(parent, name.as_ptr(), &mut stat, libc::AT_SYMLINK_NOFOLLOW) != 0 {
            return Err(io_error(
                "reading source entry metadata",
                display,
                io::Error::last_os_error(),
            ));
        }
        Ok(metadata_from_stat(&stat))
    }
}

fn read_link_at(parent: RawFd, name: &OsStr, display: &Path) -> Result<OsString, BackendError> {
    let name = c_name(name, display)?;
    let mut bytes = vec![0_u8; 256];
    loop {
        // SAFETY: The output buffer is valid and `name` is NUL-terminated.
        let length = unsafe {
            libc::readlinkat(
                parent,
                name.as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if length < 0 {
            return Err(io_error(
                "reading symlink",
                display,
                io::Error::last_os_error(),
            ));
        }
        let length = length as usize;
        if length < bytes.len() {
            bytes.truncate(length);
            return Ok(OsString::from_vec(bytes));
        }
        bytes.resize(bytes.len() * 2, 0);
    }
}

struct DirectoryStream(*mut libc::DIR);

impl Drop for DirectoryStream {
    fn drop(&mut self) {
        // SAFETY: The pointer was returned by fdopendir and is owned by this guard.
        unsafe { libc::closedir(self.0) };
    }
}

fn directory_names(directory: &File, display: &Path) -> Result<Vec<OsString>, BackendError> {
    // SAFETY: Duplicating a valid descriptor returns a new owned descriptor.
    let duplicate = unsafe { libc::dup(directory.as_raw_fd()) };
    if duplicate < 0 {
        return Err(io_error(
            "duplicating source directory",
            display,
            io::Error::last_os_error(),
        ));
    }
    // SAFETY: `duplicate` is an owned directory descriptor transferred to fdopendir.
    let pointer = unsafe { libc::fdopendir(duplicate) };
    if pointer.is_null() {
        // SAFETY: fdopendir did not take ownership on failure.
        unsafe { libc::close(duplicate) };
        return Err(io_error(
            "opening source directory stream",
            display,
            io::Error::last_os_error(),
        ));
    }
    let stream = DirectoryStream(pointer);
    let mut names = Vec::new();
    loop {
        set_errno(0);
        // SAFETY: `stream` owns a live DIR pointer.
        let entry = unsafe { libc::readdir(stream.0) };
        if entry.is_null() {
            let error = current_errno();
            if error != 0 {
                return Err(io_error(
                    "reading source directory stream",
                    display,
                    io::Error::from_raw_os_error(error),
                ));
            }
            break;
        }
        // SAFETY: d_name is NUL-terminated for a valid dirent returned by readdir.
        let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
        if bytes != b"." && bytes != b".." {
            names.push(OsString::from_vec(bytes.to_vec()));
        }
    }
    Ok(names)
}

fn c_name(name: &OsStr, display: &Path) -> Result<CString, BackendError> {
    CString::new(name.as_bytes()).map_err(|_| {
        io_error(
            "encoding source entry name",
            display,
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "entry name contains a NUL byte",
            ),
        )
    })
}

fn ensure_same_entry(
    path: &Path,
    before: &fs::Metadata,
    opened: &fs::Metadata,
) -> Result<(), BackendError> {
    ensure_same_raw_entry(
        path,
        &EntryMetadata::from_std(before),
        &EntryMetadata::from_std(opened),
    )
}

fn ensure_same_raw_entry(
    path: &Path,
    before: &EntryMetadata,
    opened: &EntryMetadata,
) -> Result<(), BackendError> {
    if before.device == opened.device
        && before.inode == opened.inode
        && before.kind() == opened.kind()
    {
        Ok(())
    } else {
        Err(changed_entry(path))
    }
}

fn changed_entry(path: &Path) -> BackendError {
    io_error(
        "verifying source entry identity",
        path,
        io::Error::other("source entry changed while cloning"),
    )
}

fn restore_metadata(path: &Path, metadata: &EntryMetadata) -> Result<(), BackendError> {
    fs::set_permissions(path, fs::Permissions::from_mode(metadata.mode))
        .map_err(|error| io_error("restoring permissions", path, error))?;
    set_file_times(path, metadata.accessed, metadata.modified)
        .map_err(|error| io_error("restoring timestamps", path, error))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn metadata_from_stat(stat: &libc::stat) -> EntryMetadata {
    EntryMetadata {
        device: stat.st_dev as u64,
        inode: stat.st_ino,
        mode: u32::from(stat.st_mode),
        len: stat.st_size.max(0) as u64,
        accessed: FileTime::from_unix_time(stat.st_atime, stat.st_atime_nsec as u32),
        modified: FileTime::from_unix_time(stat.st_mtime, stat.st_mtime_nsec as u32),
    }
}

#[cfg(target_os = "linux")]
fn metadata_from_stat(stat: &libc::stat) -> EntryMetadata {
    EntryMetadata {
        device: stat.st_dev,
        inode: stat.st_ino,
        mode: stat.st_mode,
        len: stat.st_size.max(0) as u64,
        accessed: FileTime::from_unix_time(stat.st_atime, stat.st_atime_nsec as u32),
        modified: FileTime::from_unix_time(stat.st_mtime, stat.st_mtime_nsec as u32),
    }
}

#[cfg(target_os = "macos")]
fn errno_pointer() -> *mut i32 {
    // SAFETY: __error returns the calling thread's errno pointer.
    unsafe { libc::__error() }
}

#[cfg(target_os = "linux")]
fn errno_pointer() -> *mut i32 {
    // SAFETY: __errno_location returns the calling thread's errno pointer.
    unsafe { libc::__errno_location() }
}

fn set_errno(value: i32) {
    // SAFETY: errno_pointer returns writable thread-local errno storage.
    unsafe { *errno_pointer() = value };
}

fn current_errno() -> i32 {
    // SAFETY: errno_pointer returns readable thread-local errno storage.
    unsafe { *errno_pointer() }
}

pub(crate) fn io_error(operation: &'static str, path: &Path, source: io::Error) -> BackendError {
    CowError::from_io(operation, path, source).into()
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink};

    use crate::{CowError, cancellation};

    use super::copy_tree;

    #[test]
    fn copy_stops_when_cancellation_is_requested() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let destination = root.path().join("destination");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("data"), "content").unwrap();
        cancellation::request_for_test();
        let result = copy_tree(&source, &destination);
        cancellation::reset_for_test();
        assert!(matches!(result, Err(CowError::Cancelled)));
    }

    #[test]
    fn regular_file_open_never_follows_a_symlink() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        let link = root.path().join("link");
        fs::write(&target, "outside").unwrap();
        symlink(&target, &link).unwrap();
        assert!(super::open_regular_nofollow(&link).is_err());
    }

    #[test]
    fn directory_open_never_follows_a_symlink() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join("target");
        let link = root.path().join("link");
        fs::create_dir(&target).unwrap();
        symlink(&target, &link).unwrap();
        assert!(super::open_directory_nofollow(&link).is_err());
    }
}
