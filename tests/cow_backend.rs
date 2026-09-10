use std::fs;
use std::os::unix::net::UnixListener;

use cow::{CloneOptions, CloneStrategy, CowError, clone_dir};

#[test]
fn required_cow_never_silently_copies() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("data"), "original").unwrap();

    match clone_dir(&source, &destination, CloneOptions::require_cow()) {
        Ok(result) => {
            assert!(result.strategy.is_cow());
            assert_ne!(result.strategy, CloneStrategy::Copy);
            fs::write(destination.join("data"), "changed").unwrap();
            assert_eq!(fs::read_to_string(source.join("data")).unwrap(), "original");
        }
        Err(error) => {
            eprintln!("skipped native CoW verification: {error}");
            assert!(matches!(error, CowError::CowUnsupported { .. }));
        }
    }
}

#[test]
fn automatic_strategy_always_produces_an_independent_clone() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("nested/data"), "original").unwrap();

    let result = clone_dir(&source, &destination, CloneOptions::default()).unwrap();
    assert_eq!(
        fs::read_to_string(destination.join("nested/data")).unwrap(),
        "original"
    );
    fs::write(source.join("nested/data"), "changed").unwrap();
    assert_eq!(
        fs::read_to_string(destination.join("nested/data")).unwrap(),
        "original"
    );
    assert!(matches!(
        result.strategy,
        CloneStrategy::ApfsClone | CloneStrategy::Reflink | CloneStrategy::Copy
    ));
}

#[test]
fn native_clone_rejects_special_files() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();
    let _socket = UnixListener::bind(source.join("socket")).unwrap();

    match clone_dir(&source, &destination, CloneOptions::require_cow()) {
        Err(CowError::UnsupportedFileType { .. }) => {
            assert!(!destination.exists());
        }
        Err(CowError::CowUnsupported { .. }) => {
            // Linux probes CoW with a temporary file before walking the tree. On
            // filesystems without reflink that probe fails first; special-file
            // rejection is still covered by the portable copy tests.
            assert!(!destination.exists());
        }
        Ok(result) => panic!("native clone succeeded with a socket: {result:?}"),
        Err(error) => panic!("unexpected error rejecting a socket: {error}"),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn empty_tree_reports_reflink_only_after_a_real_probe() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();

    match clone_dir(&source, &destination, CloneOptions::require_cow()) {
        Ok(result) => assert_eq!(result.strategy, CloneStrategy::Reflink),
        Err(error) => assert!(matches!(error, CowError::CowUnsupported { .. })),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn linux_uses_reflink_when_the_workspace_filesystem_supports_it() {
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&target).unwrap();
    let root = tempfile::TempDir::new_in(&target).unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("data"), "workspace-fs").unwrap();

    match clone_dir(&source, &destination, CloneOptions::require_cow()) {
        Ok(result) => {
            assert_eq!(result.strategy, CloneStrategy::Reflink);
            assert_eq!(
                fs::read_to_string(destination.join("data")).unwrap(),
                "workspace-fs"
            );
            fs::write(destination.join("data"), "changed").unwrap();
            assert_eq!(
                fs::read_to_string(source.join("data")).unwrap(),
                "workspace-fs"
            );
        }
        Err(error) => {
            eprintln!("workspace filesystem has no reflink: {error}");
            assert!(matches!(error, CowError::CowUnsupported { .. }));
        }
    }
}
