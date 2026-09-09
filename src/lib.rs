mod error;
mod types;

pub use error::CowError;
pub use types::{
    CloneOptions, CloneResult, CloneStrategy, CowCapability, FilesystemInfo, StrategyPreference,
};
