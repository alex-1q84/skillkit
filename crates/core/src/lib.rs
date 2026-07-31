//! skillkit-core：承载全部业务逻辑。CLI 和 server 都调这里，无重复逻辑。
pub mod apply;
pub mod config;
pub mod error;
pub mod git;
pub mod install;
pub mod lock;
pub mod paths;
pub mod profile;
pub mod project;
pub mod registry;
pub mod source;
pub mod symlink;

pub use apply::{
    build_status, compute_diff, run_apply, scan_shared, ApplyDiff, ApplyReport, LocalTarget,
    StatusView,
};
pub use error::{Result, SkillkitError};
pub use install::{install, uninstall};
pub use lock::FileLock;
pub use paths::Paths;
pub use profile::{list_names as list_profile_names, Profile};
pub use project::{list_ids as list_project_ids, Project};
pub use registry::{Registry, Scope, SkillMeta};
pub use source::SourcesStore;
pub use symlink::ensure_global_claude;
