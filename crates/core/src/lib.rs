//! skillkit-core：承载全部业务逻辑。CLI 和 server 都调这里，无重复逻辑。
pub mod config;
pub mod error;
pub mod paths;

pub use error::{Result, SkillkitError};
pub use paths::Paths;
