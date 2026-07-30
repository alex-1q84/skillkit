//! core 的错误类型。具体错误让调用方（CLI/server）决定呈现方式。
//!
//! 信息遵循「反馈引导行动」：不只报告失败，给出下一步（如「先 `skillkit install`」）。
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SkillkitError {
    #[error("源不存在：{name}（先 `skillkit source add` 添加）")]
    SourceNotFound { name: String },

    #[error("skill 未安装：{id}（先 `skillkit install {id}`）")]
    SkillNotInstalled { id: String },

    #[error("skill 已存在：{id}")]
    SkillAlreadyInstalled { id: String },

    #[error("git 操作失败：{message}")]
    Git { message: String },

    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),

    #[error("序列化错误：{0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("配置解析错误：{0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("配置序列化错误：{0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("canonical 目录创建失败：{0}")]
    CanonicalCreate(PathBuf),

    #[error("profile 不存在：{name}（先 `skillkit profile create {name}`）")]
    ProfileNotFound { name: String },
}

/// 原子写：先写同目录临时文件，再 rename 覆盖，避免半写状态。
pub fn atomic_write(path: &std::path::Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub type Result<T> = std::result::Result<T, SkillkitError>;
