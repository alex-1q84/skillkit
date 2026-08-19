//! profile（粗分类候选集）—— ~/.skillkit/profiles/<name>.toml，只存 skill id 列表（DRY）。
//! source/scope/version 等信息只在 registry 存一份，profile 不重复。
use crate::error::{atomic_write, Result, SkillkitError};
use crate::paths::Paths;
use crate::registry::{Registry, Scope};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub skills: Vec<String>,
}

impl Profile {
    pub fn load(paths: &Paths, name: &str) -> Result<Self> {
        let path = paths.profiles_dir().join(format!("{name}.toml"));
        if !path.exists() {
            return Err(SkillkitError::ProfileNotFound {
                name: name.to_string(),
            });
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        let _lock = crate::lock::FileLock::acquire(paths, &format!("profile-{}", self.name))?;
        let dir = paths.profiles_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{}.toml", self.name));
        atomic_write(&path, &toml::to_string_pretty(self)?)?;
        Ok(())
    }

    /// 加 skill：先查 registry 拒绝 global（core 硬约束），再查重。非幂等（重复返 SkillAlreadyInstalled）。
    /// registry 无记录的 id 按 Local 兜底（不拒绝），仅拦截明确的 global。
    pub fn add_skill(&mut self, id: &str, registry: &Registry) -> Result<()> {
        if registry.get(id).map_or(Scope::Local, |m| m.scope) == Scope::Global {
            return Err(SkillkitError::SkillIsGlobal { id: id.to_string() });
        }
        if self.skills.iter().any(|s| s == id) {
            return Err(SkillkitError::SkillAlreadyInstalled { id: id.to_string() });
        }
        self.skills.push(id.to_string());
        Ok(())
    }

    pub fn remove_skill(&mut self, id: &str) -> Result<()> {
        let before = self.skills.len();
        self.skills.retain(|s| s != id);
        if self.skills.len() == before {
            return Err(SkillkitError::SkillNotInstalled { id: id.to_string() });
        }
        Ok(())
    }
}

/// profile 目录扫描（list 用）。
pub fn list_names(paths: &Paths) -> Result<Vec<String>> {
    let dir = paths.profiles_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) == Some("toml") {
            if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                names.push(stem.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

/// 反向索引：扫所有 profile，返回含 skill_id 的 profile name 列表。
/// global skill 永远空（不属任何 profile，语义保证）。现算不缓存（profile 数量小，YAGNI）。
pub fn skill_profiles(paths: &Paths, skill_id: &str) -> Vec<String> {
    let reg = Registry::load(paths).unwrap_or_default();
    // global 直接空（不依赖 profile 实存，registry 标 global 即不属任何 profile）
    if reg.get(skill_id).map_or(Scope::Local, |m| m.scope) == Scope::Global {
        return Vec::new();
    }
    let mut out = Vec::new();
    if let Ok(names) = list_names(paths) {
        for name in names {
            if let Ok(p) = Profile::load(paths, &name) {
                if p.skills.iter().any(|s| s == skill_id) {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// 全量反向索引：skill_id → 所属 profile 名列表，一次遍历所有 profile 文件。
/// 直接从 profile 文件构建（add_skill 已拒 global；legacy profile 含 global 由 is_unassigned 的 local 判定兜底）。
/// server Skills 过滤视图与 CLI list --unassigned 共用此索引（core 单点，两壳不重复）。
pub fn skills_profiles_map(paths: &Paths) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    if let Ok(names) = list_names(paths) {
        for name in names {
            if let Ok(p) = Profile::load(paths, &name) {
                for id in &p.skills {
                    map.entry(id.clone()).or_default().push(name.clone());
                }
            }
        }
    }
    map
}

/// 「未纳入 profile」判定：local 且不属于任何 profile。
/// global 永不属 profile（语义保证），不算未纳入——否则筛选混杂全部 global，与「找无主 skill 归类」场景不符。
#[allow(clippy::implicit_hasher)] // 泛型化 hasher 会传染 skills_profiles_map 及两壳调用方签名，内部工具不值
pub fn is_unassigned(
    meta: &crate::registry::SkillMeta,
    profiles_of: &HashMap<String, Vec<String>>,
) -> bool {
    meta.is_local() && !profiles_of.contains_key(&meta.id)
}

/// 删除 profile 报告（调用方反馈用）。
#[derive(Debug, Clone, Default)]
pub struct ProfileRemovalReport {
    /// 完整解绑（重算 installed_skills + 落地清理）成功的项目名。
    pub unbound: Vec<String>,
    /// 落地失败、仅清除了绑定记录的项目名（项目目录残留下次 apply 幂等清理）。
    pub fallback: Vec<String>,
}

/// 删除 profile：先解绑所有绑定它的项目，再删 profile 文件。不存在返回 ProfileNotFound。
///
/// 解绑 = set_profiles 替换语义（applied_profiles 去名 + installed_skills 重算为剩余 profiles 并集），
/// 与工作台「取消勾选再保存」同一条路径。save 先于 run_apply 落地：落地失败（如剩余 profile
/// 引用了 registry 已无记录的 skill）时绑定记录也已清除，即「解绑出错则清除对应绑定记录」；
/// 项目目录残留由 status 的 extra 视图暴露、下次 apply 幂等清理。
///
/// profile↔skill 绑定只存于 profile 文件（skills_profiles_map 现算不缓存），删文件即解除，
/// registry 无 profile 字段，无需改动。
pub fn remove_profile(paths: &Paths, name: &str) -> Result<ProfileRemovalReport> {
    Profile::load(paths, name)?; // 存在性校验，不存在报 ProfileNotFound
    let registry = Registry::load(paths)?;
    let mut report = ProfileRemovalReport::default();
    if let Ok(ids) = crate::project::list_ids(paths) {
        for id in ids {
            let Ok(mut proj) = crate::project::Project::load(paths, &id) else {
                // 项目元数据损坏，无从解绑；profile 照删（悬空引用随该 toml 修复消失）
                continue;
            };
            if !proj.applied_profiles.iter().any(|p| p == name) {
                continue;
            }
            let remaining: Vec<String> = proj
                .applied_profiles
                .iter()
                .filter(|p| *p != name)
                .cloned()
                .collect();
            let remaining_profiles: Vec<Profile> = remaining
                .iter()
                .filter_map(|n| Profile::load(paths, n).ok())
                .collect();
            proj.set_profiles(&remaining, &remaining_profiles, &registry);
            if let Err(e) = proj.save(paths) {
                // 连绑定记录都写不进（磁盘/权限级故障），只能记日志不阻塞其余项目
                tracing::error!(error = ?e, "删除 profile {} 时写项目 {} 失败，绑定记录未清除", name, proj.name);
                continue;
            }
            match crate::apply::run_apply(paths, &mut proj, false) {
                Ok(_) => {
                    // 落地可能更新 locked_shas；二次 save 失败不视为解绑失败（下次 apply 重算）
                    if let Err(e) = proj.save(paths) {
                        tracing::warn!(error = ?e, "删除 profile {} 后回写项目 {} 的 locked_shas 失败", name, proj.name);
                    }
                    report.unbound.push(proj.name);
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "删除 profile {} 时项目 {} 落地失败，已仅清除绑定记录", name, proj.name);
                    report.fallback.push(proj.name);
                }
            }
        }
    }
    let file = paths.profiles_dir().join(format!("{name}.toml"));
    std::fs::remove_file(&file)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use tempfile::tempdir;

    fn paths() -> Paths {
        Paths::new(tempdir().unwrap().path().to_path_buf())
    }

    /// 增量建 registry：load 现有 + upsert 一条 + save（多次调用累积，供 skill_profiles 的 load 短路生效）。
    fn reg_with(paths: &Paths, id: &str, scope: Scope) -> Registry {
        let mut reg = Registry::load(paths).unwrap();
        reg.upsert(crate::registry::SkillMeta {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(),
            scope,
            version: None,
            computed_hash: Some("abc".into()),
            installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: format!(
                "~/.skillkit/.agents/skills/{}",
                id.rsplit('/').next().unwrap()
            ),
        });
        reg.save(paths).unwrap();
        reg
    }

    #[test]
    fn add_skill_local_persists_and_dedups() {
        let p = paths();
        let reg = reg_with(&p, "skills.sh/fe", Scope::Local);
        let mut profile = Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec![],
        };
        profile.add_skill("skills.sh/fe", &reg).unwrap();
        // 重复 add 报 SkillAlreadyInstalled
        assert!(matches!(
            profile.add_skill("skills.sh/fe", &reg),
            Err(SkillkitError::SkillAlreadyInstalled { .. })
        ));
    }

    #[test]
    fn add_skill_global_rejected() {
        let p = paths();
        let reg = reg_with(&p, "skills.sh/g1", Scope::Global);
        let mut profile = Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec![],
        };
        assert!(matches!(
            profile.add_skill("skills.sh/g1", &reg),
            Err(SkillkitError::SkillIsGlobal { .. })
        ));
        assert!(profile.skills.is_empty(), "拒绝时 skills 不变");
    }

    #[test]
    fn remove_skill_persists() {
        let p = paths();
        Profile {
            name: "frontend".into(),
            description: String::new(),
            skills: vec![
                "skills.sh/frontend-design".into(),
                "skills.sh/dataviz".into(),
            ],
        }
        .save(&p)
        .unwrap();
        let mut reloaded = Profile::load(&p, "frontend").unwrap();
        reloaded.remove_skill("skills.sh/dataviz").unwrap();
        reloaded.save(&p).unwrap();
        assert_eq!(
            Profile::load(&p, "frontend").unwrap().skills,
            vec!["skills.sh/frontend-design"]
        );
    }

    #[test]
    fn skill_profiles_reverses_and_global_empty() {
        let p = paths();
        reg_with(&p, "skills.sh/fe", Scope::Local);
        reg_with(&p, "skills.sh/g1", Scope::Global);
        // fe 含 fe（走 add_skill），base 手工含 fe
        let reg = Registry::load(&p).unwrap();
        let mut fe = Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec![],
        };
        fe.add_skill("skills.sh/fe", &reg).unwrap();
        fe.save(&p).unwrap();
        Profile {
            name: "base".into(),
            description: String::new(),
            skills: vec!["skills.sh/fe".into()],
        }
        .save(&p)
        .unwrap();
        // legacy：手工塞 global 进 profile（绕过校验模拟存量）
        Profile {
            name: "legacy".into(),
            description: String::new(),
            skills: vec!["skills.sh/g1".into()],
        }
        .save(&p)
        .unwrap();

        let mut got = skill_profiles(&p, "skills.sh/fe");
        got.sort();
        assert_eq!(got, vec!["base".to_string(), "fe".to_string()]);
        // global 永远空（即使 legacy profile 含它）
        assert!(skill_profiles(&p, "skills.sh/g1").is_empty());
    }

    /// 全量反向索引：一 skill 属多 profile 都登记；「未纳入」= local 且无主（global 不算）。
    #[test]
    fn skills_profiles_map_and_unassigned_semantics() {
        let p = paths();
        reg_with(&p, "skills.sh/fe", Scope::Local);
        reg_with(&p, "skills.sh/be", Scope::Local);
        reg_with(&p, "skills.sh/g1", Scope::Global);
        Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec!["skills.sh/fe".into()],
        }
        .save(&p)
        .unwrap();
        Profile {
            name: "base".into(),
            description: String::new(),
            skills: vec!["skills.sh/fe".into()],
        }
        .save(&p)
        .unwrap();

        let map = skills_profiles_map(&p);
        let mut fe_profiles = map.get("skills.sh/fe").cloned().unwrap_or_default();
        fe_profiles.sort();
        assert_eq!(fe_profiles, vec!["base".to_string(), "fe".to_string()]);
        assert!(!map.contains_key("skills.sh/be"), "无主 local 不在 map");

        let reg = Registry::load(&p).unwrap();
        assert!(
            !is_unassigned(reg.get("skills.sh/fe").unwrap(), &map),
            "有主 local"
        );
        assert!(
            is_unassigned(reg.get("skills.sh/be").unwrap(), &map),
            "无主 local"
        );
        // global 永不属 profile（语义保证），不算「未纳入」
        assert!(
            !is_unassigned(reg.get("skills.sh/g1").unwrap(), &map),
            "global"
        );
    }

    #[test]
    fn load_missing_fails() {
        let p = paths();
        assert!(matches!(
            Profile::load(&p, "nope"),
            Err(SkillkitError::ProfileNotFound { .. })
        ));
    }

    #[test]
    fn list_names_sorted() {
        let p = paths();
        Profile {
            name: "base".into(),
            description: String::new(),
            skills: vec![],
        }
        .save(&p)
        .unwrap();
        Profile {
            name: "frontend".into(),
            description: String::new(),
            skills: vec![],
        }
        .save(&p)
        .unwrap();
        assert_eq!(list_names(&p).unwrap(), vec!["base", "frontend"]);
    }

    /// 建 canonical 目录 + registry 记录（local），供落地类测试。
    fn install_bare(paths: &Paths, id: &str) {
        let skill = id.rsplit('/').next().unwrap();
        let canon = paths.skillkit_skills_dir().join(skill);
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        let mut reg = Registry::load(paths).unwrap();
        reg.upsert(crate::registry::SkillMeta {
            id: id.into(),
            name: skill.into(),
            source: id.split('/').next().unwrap().into(),
            scope: Scope::Local,
            version: None,
            computed_hash: Some("sha1".into()),
            installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        });
        reg.save(paths).unwrap();
    }

    /// 删除 profile：解绑所有绑定项目（重算 + 落地清理）、删文件、反向索引同步消失。
    #[test]
    fn remove_profile_unbinds_projects_recomputes_and_deletes_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Paths::new(tmp.path().to_path_buf());
        install_bare(&p, "dc/fe1");
        install_bare(&p, "dc/fe2");
        Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec!["dc/fe1".into(), "dc/fe2".into()],
        }
        .save(&p)
        .unwrap();
        Profile {
            name: "base".into(),
            description: String::new(),
            skills: vec!["dc/fe2".into()],
        }
        .save(&p)
        .unwrap();
        let proj_root = tmp.path().join("proj");
        std::fs::create_dir_all(proj_root.join(".git/info")).unwrap();
        let reg = Registry::load(&p).unwrap();
        let mut proj = crate::project::Project {
            id: "P1".into(),
            name: "proj".into(),
            path: proj_root.to_string_lossy().into_owned(),
            agents: vec![],
            applied_profiles: vec![],
            installed_skills: vec![],
            locked_shas: std::collections::BTreeMap::new(),
        };
        // 绑 fe+base 并落地（fe1、fe2 均落 .agents/skills）
        let fe = Profile::load(&p, "fe").unwrap();
        let base = Profile::load(&p, "base").unwrap();
        proj.set_profiles(&["fe".into(), "base".into()], &[fe, base], &reg);
        proj.save(&p).unwrap();
        crate::apply::run_apply(&p, &mut proj, false).unwrap();
        assert!(proj_root.join(".agents/skills/fe1/SKILL.md").exists());

        let report = remove_profile(&p, "fe").unwrap();
        assert_eq!(report.unbound, vec!["proj".to_string()]);
        assert!(report.fallback.is_empty());
        let after = crate::project::Project::load(&p, "P1").unwrap();
        assert_eq!(after.applied_profiles, vec!["base".to_string()]);
        assert_eq!(
            after.installed_skills,
            vec!["dc/fe2".to_string()],
            "installed_skills 重算为剩余 base 的并集"
        );
        assert!(
            !proj_root.join(".agents/skills/fe1").exists(),
            "fe 独有 skill 的落地被清理"
        );
        assert!(
            proj_root.join(".agents/skills/fe2/SKILL.md").exists(),
            "base 仍绑定的 skill 保留"
        );
        assert!(
            Profile::load(&p, "fe").is_err(),
            "profile 文件已删（profile↔skill 绑定随文件解除）"
        );
        let map = skills_profiles_map(&p);
        assert!(!map.contains_key("dc/fe1"), "fe1 失去归属");
        assert_eq!(
            map.get("dc/fe2").map(Vec::as_slice),
            Some(&["base".to_string()][..])
        );
    }

    /// 落地失败（剩余 profile 引用 registry 已无记录的 skill）时兜底：绑定记录仍被清除。
    #[test]
    fn remove_profile_apply_fails_still_clears_record() {
        let tmp = tempfile::tempdir().unwrap();
        let p = Paths::new(tmp.path().to_path_buf());
        install_bare(&p, "dc/fe1");
        Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec!["dc/fe1".into()],
        }
        .save(&p)
        .unwrap();
        // base 引用 dc/ghost：registry 无记录 → run_apply 报 SkillNotInstalled
        Profile {
            name: "base".into(),
            description: String::new(),
            skills: vec!["dc/ghost".into()],
        }
        .save(&p)
        .unwrap();
        crate::project::Project {
            id: "P2".into(),
            name: "proj2".into(),
            path: tmp.path().join("no-dir").to_string_lossy().into_owned(),
            agents: vec![],
            applied_profiles: vec!["fe".into(), "base".into()],
            installed_skills: vec!["dc/fe1".into(), "dc/ghost".into()],
            locked_shas: std::collections::BTreeMap::new(),
        }
        .save(&p)
        .unwrap();

        let report = remove_profile(&p, "fe").unwrap();
        assert_eq!(report.fallback, vec!["proj2".to_string()]);
        assert!(report.unbound.is_empty());
        let after = crate::project::Project::load(&p, "P2").unwrap();
        assert_eq!(
            after.applied_profiles,
            vec!["base".to_string()],
            "落地失败但绑定记录已清除（save 先于落地）"
        );
        assert!(
            Profile::load(&p, "fe").is_err(),
            "解绑兜底不阻塞 profile 删除"
        );
    }

    #[test]
    fn remove_profile_missing_errors() {
        let p = paths();
        assert!(matches!(
            remove_profile(&p, "nope"),
            Err(SkillkitError::ProfileNotFound { .. })
        ));
    }
}
