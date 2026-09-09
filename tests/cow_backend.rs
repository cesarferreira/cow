use std::fs;

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
        Err(error) => assert!(matches!(error, CowError::CowUnsupported { .. })),
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
    assert_eq!(fs::read_to_string(destination.join("nested/data")).unwrap(), "original");
    fs::write(&source.join("nested/data"), "changed").unwrap();
    assert_eq!(fs::read_to_string(destination.join("nested/data")).unwrap(), "original");
    assert!(matches!(result.strategy, CloneStrategy::ApfsClone | CloneStrategy::Reflink | CloneStrategy::Copy));
}
