use std::fs;

use cow::{CloneOptions, CowError, clone_dir};

#[test]
fn rejects_a_missing_source() {
    let root = tempfile::tempdir().unwrap();
    let error = clone_dir(
        root.path().join("missing"),
        root.path().join("destination"),
        CloneOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(error, CowError::InvalidSource { .. }));
}

#[test]
fn rejects_a_regular_file_source() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.txt");
    fs::write(&source, "hello").unwrap();
    let error = clone_dir(
        &source,
        root.path().join("destination"),
        CloneOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(error, CowError::SourceNotDirectory { .. }));
}

#[test]
fn rejects_an_existing_destination() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&destination).unwrap();
    let error = clone_dir(&source, &destination, CloneOptions::default()).unwrap_err();
    assert!(matches!(error, CowError::DestinationExists { .. }));
}

#[test]
fn rejects_a_destination_inside_the_source() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    let error = clone_dir(
        &source,
        source.join("nested-clone"),
        CloneOptions::default(),
    )
    .unwrap_err();
    assert!(matches!(error, CowError::OverlappingPaths { .. }));
}
