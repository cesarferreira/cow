use std::fs;

use cow::{CloneStrategy, CowCapability, CowError, inspect};

#[test]
fn inspection_reports_consistent_capability() {
    let directory = tempfile::tempdir().unwrap();
    let info = inspect(directory.path()).unwrap();

    assert_eq!(info.path, fs::canonicalize(directory.path()).unwrap());
    assert!(matches!(info.platform.as_str(), "macos" | "linux"));
    assert!(info.filesystem.as_ref().is_none_or(|name| !name.is_empty()));
    assert_eq!(
        info.preferred_strategy.is_cow(),
        info.cow_supported == CowCapability::Supported
    );
    assert!(fs::read_dir(directory.path()).unwrap().next().is_none());
}

#[test]
fn inspection_rejects_a_missing_path() {
    let directory = tempfile::tempdir().unwrap();
    let error = inspect(directory.path().join("missing")).unwrap_err();
    assert!(matches!(error, CowError::InvalidSource { .. }));
}

#[test]
fn supported_capability_names_a_native_strategy() {
    let directory = tempfile::tempdir().unwrap();
    let info = inspect(directory.path()).unwrap();
    if info.cow_supported == CowCapability::Supported {
        assert!(matches!(info.preferred_strategy, CloneStrategy::ApfsClone | CloneStrategy::Reflink));
    }
}
