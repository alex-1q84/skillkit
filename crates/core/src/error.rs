//! core 的错误类型。具体错误让调用方（CLI/server）决定呈现方式。
//!
//! 信息遵循「反馈引导行动」：不只报告失败，给出下一步（如「先 `skillkit install`」）。
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum SkillkitError {
    #[error("源不存在：{name}（先 `skillkit source add` 添加）")]
    SourceNotFound { name: String },

    #[error("无法从 package 推导源名称：{package}（可用 --name / name 字段指定）")]
    SourceNameUnderived { package: String },

    #[error("该名称已被源 {name} 占用（可用 --name / name 字段指定别名重新添加）")]
    SourceNameTaken { name: String },

    #[error("skill 未安装：{id}（先 `skillkit install {id}`）")]
    SkillNotInstalled { id: String },

    #[error("skill 已存在：{id}")]
    SkillAlreadyInstalled { id: String },

    #[error(
        "skill 是 global，不属 profile/project：{id}（先 `skillkit rescope {id} local` 再归入）"
    )]
    SkillIsGlobal { id: String },

    #[error("外部工具调用失败：{message}")]
    Tool { message: String },

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

    #[error("canonical 目录删除失败：{0}（权限不足？检查文件占用或手动删除）")]
    RemoveFailed(PathBuf),

    #[error("profile 不存在：{name}（先 `skillkit profile create {name}`）")]
    ProfileNotFound { name: String },

    #[error("project 不存在：{id}（先 `skillkit project add <path>` 注册）")]
    ProjectNotFound { id: String },

    #[error("文件锁超时：{key} 被其他进程持有（稍后重试，或关闭其他 skillkit 进程）")]
    LockTimeout { key: String },

    #[error("升级 {id} 将影响以下项目的版本基线：{affected:?}，需确认或 --yes")]
    UpgradeBlocked { id: String, affected: Vec<String> },

    #[error("本地 skill 无效：{path}（{reason}）")]
    InvalidLocalSkill { path: String, reason: String },

    #[error("skill 归档结构不明确：{reason}（请直接传 skill 目录路径）")]
    AmbiguousSkillArchive { reason: String },

    #[error(
        "目录 {name} 已被占用：{owner}（先 skillkit skill remove <owner> 再装，或手动删除该目录）",
        owner = owner_id.as_deref().unwrap_or("无 registry 记录的孤儿目录")
    )]
    SkillPoolOccupied {
        name: String,
        owner_id: Option<String>,
    },
}

/// 原子写：先写同目录临时文件，再 rename 覆盖，避免半写状态。
pub fn atomic_write(path: &std::path::Path, content: &str) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub type Result<T> = std::result::Result<T, SkillkitError>;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skill_is_global_message_guides_rescope() {
        let e = SkillkitError::SkillIsGlobal {
            id: "skills.sh/foo".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("global"), "文案点明 global：{msg}");
        assert!(
            msg.contains("rescope skills.sh/foo local"),
            "文案给出 rescope 引导：{msg}"
        );
    }

    #[test]
    fn local_skill_errors_guide_action() {
        let a = SkillkitError::InvalidLocalSkill {
            path: "/x".into(),
            reason: "未找到 SKILL.md".into(),
        };
        assert!(a.to_string().contains("SKILL.md"));
        let b = SkillkitError::SkillPoolOccupied {
            name: "foo".into(),
            owner_id: Some("skills.sh/foo".into()),
        };
        assert!(b.to_string().contains("skills.sh/foo"));
        let c = SkillkitError::SkillPoolOccupied {
            name: "foo".into(),
            owner_id: None,
        };
        assert!(c.to_string().contains("孤儿") || c.to_string().contains("foo"));
    }

    /// 反馈引导行动：撞名/推导失败都要写明用哪个参数指定别名（CLI --name / server name 字段）。
    #[test]
    fn source_name_errors_guide_how_to_rename() {
        let taken = SkillkitError::SourceNameTaken {
            name: "team".into(),
        }
        .to_string();
        assert!(taken.contains("--name"), "撞名文案给出改名参数：{taken}");
        let underived = SkillkitError::SourceNameUnderived {
            package: "x".into(),
        }
        .to_string();
        assert!(
            underived.contains("--name"),
            "推导失败文案给出改名参数：{underived}"
        );
    }
}
