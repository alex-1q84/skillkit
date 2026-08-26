//! 安装源注册表（sources.toml）。Source 极简成 {name, package}：
//! package 是 npx skills 的 source format 串（github shorthand / git url / local path）；
//! None 表示 registry 搜索入口（skills.sh 默认源）。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub name: String,
    /// npx skills source format（github shorthand / git url / local path）；None=registry 搜索入口。
    pub package: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourcesStore {
    #[serde(default)]
    pub sources: Vec<Source>,
}

impl SourcesStore {
    pub fn load(paths: &Paths) -> Result<Self> {
        let path = paths.sources_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)?;
        // 旧 schema（含 source_type/url 字段）被 deny_unknown_fields 挡下：解析失败即备份后视为空
        // （项目未发布，无线上数据，直接 reset 比迁移成本低）。
        match toml::from_str::<Self>(&content) {
            Ok(store) => Ok(store),
            Err(e) => {
                tracing::warn!(
                    error = ?e, path = %path.display(),
                    "sources.toml 解析失败，按旧 schema 处理：备份 .bak 后重置为默认源"
                );
                let _ = std::fs::rename(&path, path.with_extension("toml.bak"));
                Ok(Self::default())
            }
        }
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let _lock = crate::lock::FileLock::acquire(paths, "sources")?;
        let path = paths.sources_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        crate::error::atomic_write(&path, &toml::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn list(&self) -> &[Source] {
        &self.sources
    }

    pub fn add(&mut self, source: Source) -> Result<()> {
        if self.sources.iter().any(|s| s.name == source.name) {
            return Err(SkillkitError::SkillAlreadyInstalled {
                id: format!("source:{}", source.name),
            });
        }
        self.sources.push(source);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<&mut Self> {
        let before = self.sources.len();
        self.sources.retain(|s| s.name != name);
        if self.sources.len() == before {
            return Err(SkillkitError::SourceNotFound {
                name: name.to_string(),
            });
        }
        Ok(self)
    }

    pub fn get(&self, name: &str) -> Result<&Source> {
        self.sources
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| SkillkitError::SourceNotFound {
                name: name.to_string(),
            })
    }

    /// 注册源：CLI source add 与 server sources::add 的业务单点。
    /// name = 显式名（trim 非空）或从 package 推导；推导失败报 SourceNameUnderived，
    /// 撞名报 SourceNameTaken（调用方按错误类型决定呈现：CLI 打印文案 / server 返 400）。
    /// 返回最终 name。
    pub fn register(paths: &Paths, package: &str, name_override: Option<&str>) -> Result<String> {
        let name = name_override
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| derive_source_name(package))
            .ok_or_else(|| SkillkitError::SourceNameUnderived {
                package: package.to_string(),
            })?;
        let mut store = Self::load(paths)?;
        if store.get(&name).is_ok() {
            return Err(SkillkitError::SourceNameTaken { name });
        }
        store.add(Source {
            name: name.clone(),
            package: Some(package.to_string()),
        })?;
        store.save(paths)?;
        Ok(name)
    }

    /// 入口层（CLI main / server 启动）调：sources.toml 不存在或列表里没有 name="skills.sh"
    /// 时，补回默认 registry 搜索入口。覆盖旧语义「文件不存在才种入」：用户删了 skills.sh，
    /// 下次任意入口启动都会自动补回（保证 GUI/CLI 始终有默认源）。
    pub fn ensure_default(paths: &Paths) -> Result<()> {
        let mut store = Self::load(paths)?;
        if store.sources.iter().any(|s| s.name == "skills.sh") {
            return Ok(());
        }
        store.sources.push(Source {
            name: "skills.sh".into(),
            package: None,
        });
        store.save(paths)
    }
}

/// 从 package 推导 source 名：git url 取仓库名、本地路径取目录名。
/// 判定顺序：先剥 `://` scheme；含 `:` 且 `:` 前无 `/` 视为 scp-style（取冒号后）；
/// 其余按 `/` 取末段（shorthand `A/B` / 本地路径 / 单段直接返回）。
/// 统一后处理：剥尾斜杠 + 剥一个 `.git` 后缀。空串 / 纯空白返回 None。
pub fn derive_source_name(package: &str) -> Option<String> {
    let trimmed = package.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    // 仓库段：url 剥 scheme；scp-style 剥 `user@host:`；shorthand/本地路径保留整串。
    let repo_seg = if let Some(rest) = trimmed.split_once("://") {
        rest.1
    } else if let Some(idx) = trimmed.find(':') {
        if trimmed[..idx].contains('/') {
            trimmed
        } else {
            &trimmed[idx + 1..]
        }
    } else {
        trimmed
    };
    let last = repo_seg.rsplit('/').next().unwrap_or(repo_seg);
    let name = last.strip_suffix(".git").unwrap_or(last);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use tempfile::tempdir;

    fn paths() -> Paths {
        Paths::new(tempdir().unwrap().path().to_path_buf())
    }

    #[test]
    fn add_then_list_then_remove() {
        let p = paths();
        let mut store = SourcesStore::load(&p).unwrap();
        assert!(store.list().is_empty());

        store
            .add(Source {
                name: "team-private".into(),
                package: Some("git@github.com:org/team.git".into()),
            })
            .unwrap();
        store.save(&p).unwrap();

        let mut reloaded = SourcesStore::load(&p).unwrap();
        assert_eq!(reloaded.list().len(), 1);
        assert_eq!(reloaded.list()[0].name, "team-private");

        reloaded.remove("team-private").unwrap().save(&p).unwrap();
        assert!(SourcesStore::load(&p).unwrap().list().is_empty());
    }

    #[test]
    fn add_duplicate_fails() {
        let mut store = SourcesStore::default();
        let s = Source {
            name: "x".into(),
            package: Some("~/x".into()),
        };
        store.add(s.clone()).unwrap();
        assert!(store.add(s).is_err());
    }

    #[test]
    fn remove_missing_fails() {
        let mut store = SourcesStore::default();
        assert!(matches!(
            store.remove("nope"),
            Err(SkillkitError::SourceNotFound { .. })
        ));
    }

    #[test]
    fn ensure_default_seeds_skills_sh_when_absent() {
        let p = paths();
        assert!(!p.sources_path().exists());
        SourcesStore::ensure_default(&p).unwrap();
        let store = SourcesStore::load(&p).unwrap();
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].name, "skills.sh");
        assert!(store.list()[0].package.is_none());
        // 已存在不再覆盖
        SourcesStore::ensure_default(&p).unwrap();
        assert_eq!(SourcesStore::load(&p).unwrap().list().len(), 1);
    }

    #[test]
    fn ensure_default_refills_after_empty_or_removal() {
        // 空文件（sources = []）：本 bug 的场景，启动后应补回 skills.sh。
        let p = paths();
        SourcesStore::default().save(&p).unwrap();
        SourcesStore::ensure_default(&p).unwrap();
        let store = SourcesStore::load(&p).unwrap();
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].name, "skills.sh");

        // 用户删掉 skills.sh（含其它源）后，下次启动自动补回，且不丢其它源、不重复。
        let p2 = paths();
        let mut store = SourcesStore::default();
        store
            .add(Source {
                name: "team-private".into(),
                package: Some("git@github.com:org/team.git".into()),
            })
            .unwrap();
        store.save(&p2).unwrap();
        SourcesStore::ensure_default(&p2).unwrap();
        let reloaded = SourcesStore::load(&p2).unwrap();
        assert_eq!(reloaded.list().len(), 2);
        assert!(reloaded.list().iter().any(|s| s.name == "skills.sh"));
        assert!(reloaded.list().iter().any(|s| s.name == "team-private"));
        // 再调不重复
        SourcesStore::ensure_default(&p2).unwrap();
        assert_eq!(SourcesStore::load(&p2).unwrap().list().len(), 2);
    }

    #[test]
    fn register_derives_name_and_persists() {
        let p = paths();
        let name = SourcesStore::register(&p, "https://github.com/org/team.git", None).unwrap();
        assert_eq!(name, "team");
        let store = SourcesStore::load(&p).unwrap();
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].name, "team");
        assert_eq!(
            store.list()[0].package.as_deref(),
            Some("https://github.com/org/team.git")
        );
    }

    #[test]
    fn register_explicit_name_trims_and_overrides() {
        let p = paths();
        // 显式名 trim 后非空即生效，不从 package 推导
        let name = SourcesStore::register(&p, "org/repo", Some("  alias  ")).unwrap();
        assert_eq!(name, "alias");
        // 显式名 trim 后为空 → 回退推导
        let name2 = SourcesStore::register(&p, "org/other", Some("   ")).unwrap();
        assert_eq!(name2, "other");
    }

    #[test]
    fn register_fails_when_name_underived() {
        let p = paths();
        let err = SourcesStore::register(&p, "", None).unwrap_err();
        assert!(
            matches!(err, SkillkitError::SourceNameUnderived { .. }),
            "推导失败应报 SourceNameUnderived：{err:?}"
        );
        // 不落任何源
        assert!(SourcesStore::load(&p).unwrap().list().is_empty());
    }

    #[test]
    fn register_fails_on_duplicate_name() {
        let p = paths();
        SourcesStore::register(&p, "org/team", None).unwrap();
        // 同推导名撞车
        let err =
            SourcesStore::register(&p, "https://github.com/other/team.git", None).unwrap_err();
        assert!(
            matches!(err, SkillkitError::SourceNameTaken { .. }),
            "撞名应报 SourceNameTaken：{err:?}"
        );
        assert_eq!(SourcesStore::load(&p).unwrap().list().len(), 1);
    }

    #[test]
    fn derive_source_name_rules() {
        let cases: &[(&str, Option<&str>)] = &[
            // github shorthand
            ("owner/repo", Some("repo")),
            ("org/skills", Some("skills")),
            // scp-style git url
            ("git@github.com:org/repo.git", Some("repo")),
            ("git@example/x.git", Some("x")),
            // scheme url
            ("https://github.com/org/repo.git", Some("repo")),
            ("ssh://git@host:7999/dw/repo.git", Some("repo")),
            // 本地路径
            ("~/my-skills", Some("my-skills")),
            ("/abs/path/to/foo", Some("foo")),
            ("./foo", Some("foo")),
            // 尾斜杠先剥
            ("repo.git/", Some("repo")),
            ("~/skills/", Some("skills")),
            // 空 / 纯空白
            ("", None),
            ("   ", None),
        ];
        for (input, expected) in cases {
            assert_eq!(
                derive_source_name(input).as_deref(),
                *expected,
                "derive_source_name({input:?})"
            );
        }
    }

    #[test]
    fn legacy_schema_backed_up_and_reset() {
        // 旧 schema（source_type/url 字段）被 deny_unknown_fields 挡下 → 备份 .bak + 重置为空。
        let p = paths();
        std::fs::create_dir_all(p.skillkit_dir()).unwrap();
        std::fs::write(
            p.sources_path(),
            "[[sources]]\nname=\"old\"\nsource_type=\"git\"\nurl=\"x\"\n",
        )
        .unwrap();
        let store = SourcesStore::load(&p).unwrap();
        assert!(store.list().is_empty());
        assert!(p.sources_path().with_extension("toml.bak").exists());
    }

    #[test]
    fn malformed_toml_backed_up_and_reset() {
        // 非旧 schema 的坏 TOML 同样解析失败 → 备份重置，不 panic、不吞错误。
        let p = paths();
        std::fs::create_dir_all(p.skillkit_dir()).unwrap();
        std::fs::write(p.sources_path(), "not-toml [[[ ").unwrap();
        let store = SourcesStore::load(&p).unwrap();
        assert!(store.list().is_empty());
        assert!(p.sources_path().with_extension("toml.bak").exists());
    }
}
