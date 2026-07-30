//! skillkit-core：承载全部业务逻辑。CLI 和 server 都调这里，无重复逻辑。
pub mod config;
pub mod error;
pub mod git;
pub mod install;
pub mod paths;
pub mod registry;
pub mod source;
pub mod symlink;

pub use error::{Result, SkillkitError};
pub use install::{install, uninstall};
pub use paths::Paths;
pub use registry::{Registry, Scope, SkillMeta};
pub use symlink::ensure_global_claude;
