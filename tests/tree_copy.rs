use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixListener},
};

use cow::{CloneOptions, CloneStrategy, CowError, clone_dir};
use filetime::{FileTime, set_file_mtime};

#[test]
fn physical_copy_preserves_the_complete_tree() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(source.join("nested/.git")).unwrap();
    fs::write(source.join("nested/data.txt"), "hello").unwrap();
    fs::write(source.join(".hidden"), "secret").unwrap();
    fs::write(source.join("nested/.git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(source.join("unicode-🐄 and space"), "moo").unwrap();
    fs::write(source.join("empty"), []).unwrap();
    let executable = source.join("run.sh");
    fs::write(&executable, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o751)).unwrap();
    let timestamp = FileTime::from_unix_time(1_700_000_000, 0);
    set_file_mtime(source.join("nested/data.txt"), timestamp).unwrap();
    std::os::unix::fs::symlink("nested/data.txt", source.join("current")).unwrap();
    std::os::unix::fs::symlink("missing", source.join("broken")).unwrap();

    let result = clone_dir(&source, &destination, CloneOptions::copy()).unwrap();

    assert_eq!(result.strategy, CloneStrategy::Copy);
    assert_eq!(result.logical_bytes, 5 + 6 + 21 + 3 + 0 + 10);
    assert_eq!(result.files, 6);
    assert_eq!(fs::read_to_string(destination.join("nested/data.txt")).unwrap(), "hello");
    assert_eq!(fs::read_link(destination.join("current")).unwrap(), std::path::Path::new("nested/data.txt"));
    assert_eq!(fs::read_link(destination.join("broken")).unwrap(), std::path::Path::new("missing"));
    assert_eq!(fs::metadata(destination.join("run.sh")).unwrap().permissions().mode() & 0o777, 0o751);
    assert_eq!(FileTime::from_last_modification_time(&fs::metadata(destination.join("nested/data.txt")).unwrap()), timestamp);

    fs::write(destination.join("nested/data.txt"), "destination").unwrap();
    assert_eq!(fs::read_to_string(source.join("nested/data.txt")).unwrap(), "hello");
    fs::write(source.join(".hidden"), "source").unwrap();
    assert_eq!(fs::read_to_string(destination.join(".hidden")).unwrap(), "secret");
}

#[test]
fn failure_removes_the_private_destination() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("before"), "content").unwrap();
    let _socket = UnixListener::bind(source.join("socket")).unwrap();

    let error = clone_dir(&source, &destination, CloneOptions::copy()).unwrap_err();
    assert!(matches!(error, CowError::UnsupportedFileType { .. }));
    assert!(!destination.exists());
    assert!(fs::read_dir(root.path()).unwrap().all(|entry| {
        !entry.unwrap().file_name().to_string_lossy().starts_with(".cow-tmp-")
    }));
    assert!(source.exists());
}
