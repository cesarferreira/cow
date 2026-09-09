use cow::{CloneOptions, CloneStrategy, CowCapability, StrategyPreference};

#[test]
fn default_options_prefer_automatic_cow() {
    assert_eq!(CloneOptions::default().strategy, StrategyPreference::Auto);
    assert!(CloneStrategy::ApfsClone.is_cow());
    assert!(CloneStrategy::Reflink.is_cow());
    assert!(!CloneStrategy::Copy.is_cow());
}

#[test]
fn capabilities_are_explicitly_tristate() {
    assert_ne!(CowCapability::Unknown, CowCapability::Unavailable);
}
