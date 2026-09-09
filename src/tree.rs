use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::PermissionsExt,
    path::Path,
};

use filetime::{FileTime, set_file_times, set_symlink_file_times};

use crate::CowError;

#[derive(Debug, Default)]
pub(crate) struct TreeStats {
    pub logical_bytes: u64,
    pub files: u64,
}

#[derive(Debug)]
pub(crate) enum BackendError {
    Unsupported,
    Fatal(CowError),
}

impl From<CowError> for BackendError {
    fn from(value: CowError) -> Self {
        Self::Fatal(value)
    }
}

pub(crate) fn copy_tree(source: &Path, destination: &Path) -> Result<TreeStats, CowError> {
    clone_tree(source, destination, &mut copy_file).map_err(|error| match error {
        BackendError::Fatal(error) => error,
        BackendError::Unsupported => CowError::Unavailable("regular copy reported unsupported"),
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
    F: FnMut(&Path, &Path) -> Result<(), BackendError>,
{
    let mut stats = TreeStats::default();
    clone_entry(source, destination, regular_file, &mut stats)?;
    Ok(stats)
}

fn clone_entry<F>(
    source: &Path,
    destination: &Path,
    regular_file: &mut F,
    stats: &mut TreeStats,
) -> Result<(), BackendError>
where
    F: FnMut(&Path, &Path) -> Result<(), BackendError>,
{
    if crate::cancellation::requested() {
        return Err(CowError::Cancelled.into());
    }
    let metadata = fs::symlink_metadata(source)
        .map_err(|error| io_error("reading entry metadata", source, error))?;
    let file_type = metadata.file_type();

    if file_type.is_dir() {
        fs::create_dir(destination)
            .map_err(|error| io_error("creating directory", destination, error))?;
        for entry in
            fs::read_dir(source).map_err(|error| io_error("reading directory", source, error))?
        {
            let entry =
                entry.map_err(|error| io_error("reading directory entry", source, error))?;
            clone_entry(
                &entry.path(),
                &destination.join(entry.file_name()),
                regular_file,
                stats,
            )?;
        }
        restore_metadata(destination, &metadata)?;
    } else if file_type.is_file() {
        regular_file(source, destination)?;
        restore_metadata(destination, &metadata)?;
        stats.files += 1;
        stats.logical_bytes += metadata.len();
    } else if file_type.is_symlink() {
        let target =
            fs::read_link(source).map_err(|error| io_error("reading symlink", source, error))?;
        std::os::unix::fs::symlink(&target, destination)
            .map_err(|error| io_error("creating symlink", destination, error))?;
        let accessed = FileTime::from_last_access_time(&metadata);
        let modified = FileTime::from_last_modification_time(&metadata);
        set_symlink_file_times(destination, accessed, modified)
            .map_err(|error| io_error("restoring symlink timestamps", destination, error))?;
    } else {
        return Err(CowError::UnsupportedFileType {
            path: source.to_path_buf(),
        }
        .into());
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), BackendError> {
    let mut input =
        File::open(source).map_err(|error| io_error("opening source file", source, error))?;
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

fn restore_metadata(path: &Path, metadata: &fs::Metadata) -> Result<(), BackendError> {
    fs::set_permissions(
        path,
        fs::Permissions::from_mode(metadata.permissions().mode()),
    )
    .map_err(|error| io_error("restoring permissions", path, error))?;
    let accessed = FileTime::from_last_access_time(metadata);
    let modified = FileTime::from_last_modification_time(metadata);
    set_file_times(path, accessed, modified)
        .map_err(|error| io_error("restoring timestamps", path, error))?;
    Ok(())
}

pub(crate) fn io_error(operation: &'static str, path: &Path, source: io::Error) -> BackendError {
    CowError::Io {
        operation,
        path: path.to_path_buf(),
        source,
    }
    .into()
}

#[cfg(test)]
mod tests {
    use std::fs;

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
}
