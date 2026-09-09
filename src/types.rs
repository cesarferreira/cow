use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyPreference {
    #[default]
    Auto,
    Cow,
    Copy,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloneStrategy {
    ApfsClone,
    Reflink,
    Copy,
}

impl CloneStrategy {
    pub const fn is_cow(self) -> bool {
        !matches!(self, Self::Copy)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CowCapability {
    Supported,
    Unavailable,
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CloneOptions {
    pub strategy: StrategyPreference,
}

impl CloneOptions {
    pub const fn copy() -> Self {
        Self {
            strategy: StrategyPreference::Copy,
        }
    }

    pub const fn require_cow() -> Self {
        Self {
            strategy: StrategyPreference::Cow,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloneResult {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub strategy: CloneStrategy,
    pub logical_bytes: u64,
    pub files: u64,
    pub duration: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemInfo {
    pub path: PathBuf,
    pub platform: String,
    pub filesystem: Option<String>,
    pub cow_supported: CowCapability,
    pub preferred_strategy: CloneStrategy,
}
