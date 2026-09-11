//! 同名副本处理：`~/.agents/skills/` 下与 registry 同名、但 registry 认领的正主
//! 另有所指的冗余真实目录（外部工具写回 / 手工拷贝的产物）。import 按名去重只会
//! 跳过它们——自动 adopt 意味着删副本，而副本与正主内容可能有实质差异，盲删丢数据。
//! 这里提供人工裁决三件套：列出（含与池子正主的一致性对比）/ 删至系统回收站 /
//! 以副本覆盖池子正主（正主先送回收站，可逆）。
use crate::error::{Result, SkillkitError};
use crate::paths::Paths;
use crate::registry::{Registry, Scope, SkillMeta};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// 一条待处理副本。`identical_with_canonical`：与池子正主的内容对比结果；
/// None = 无法对比（owner canonical 不一致 / 不在池子 / 目录不存在）。
#[derive(Debug, Clone, Serialize)]
pub struct DuplicateEntry {
    pub name: String,
    /// 副本目录绝对路径（~/.agents/skills/<name>）。
    pub path: String,
    /// registry 同名记录的 id 列表（副本不被任何记录认领——canonical 均另有所指）。
    pub owner_ids: Vec<String>,
    /// 同名记录中是否存在 managed（computed_hash=Some）——managed 正主来自源安装，
    /// 覆盖会破坏升级语义，adopt 对此类拒绝。
    pub owner_managed: bool,
    /// 全部 owner canonical 一致时给出；不一致为 None（保守，不自动裁决）。
    pub owner_canonical: Option<String>,
    pub identical_with_canonical: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DupesReport {
    pub entries: Vec<DuplicateEntry>,
}

/// 列出 ~/.agents/skills/ 下的同名副本：真实目录（非 symlink）、含 SKILL.md、
/// registry 有同名记录、且没有任何记录的 canonical 指向该目录本身
/// （canonical 指向它的那条是正主，不列——比如旧版本 import 只登记不迁移的产物）。
pub fn list_duplicates(paths: &Paths) -> Result<DupesReport> {
    let reg = Registry::load(paths)?;
    let mut report = DupesReport::default();
    let dir = paths.agents_skills_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(report); // 目录不存在 / 无权限：没有副本可列
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let is_real_dir = std::fs::symlink_metadata(&p)
            .is_ok_and(|m| m.file_type().is_dir() && !m.file_type().is_symlink());
        if !is_real_dir || !p.join("SKILL.md").exists() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let owners: Vec<&SkillMeta> = reg
            .skills
            .values()
            .filter(|m| m.name == name && Path::new(&m.canonical_path) != p.as_path())
            .collect();
        if owners.is_empty() {
            continue;
        }
        let owner_canonical = match owners.split_first() {
            Some((first, rest))
                if rest
                    .iter()
                    .all(|m| m.canonical_path == first.canonical_path) =>
            {
                Some(first.canonical_path.clone())
            }
            _ => None,
        };
        let identical_with_canonical = owner_canonical.as_deref().and_then(|canon| {
            let canon = Path::new(canon);
            let in_pool = canon.starts_with(paths.skillkit_skills_dir());
            (in_pool && canon.is_dir()).then(|| dirs_identical(&p, canon))
        });
        report.entries.push(DuplicateEntry {
            owner_ids: owners.iter().map(|m| m.id.clone()).collect(),
            owner_managed: owners.iter().any(|m| m.computed_hash.is_some()),
            owner_canonical,
            identical_with_canonical,
            name,
            path: p.to_string_lossy().into_owned(),
        });
    }
    report.entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(report)
}

/// 生产用回收站实现（macOS 走 NSWorkspace，删入 Finder 可见的废纸篓）。
/// 壳层把它作为 `discard` 传入 trash/adopt；测试传临时目录重定向的假实现。
pub fn system_trash(p: &Path) -> Result<()> {
    trash::delete(p).map_err(|e| SkillkitError::Tool {
        message: format!("移入回收站失败：{e}"),
    })
}

/// 删副本到系统回收站（可逆）。删除后对同名 global 记录幂等补建桥接——
/// 典型场景 grill-with-docs 型占位目录：清掉后桥接立即收敛，无需重跑 import。
/// local 记录无桥接语义（如 dingtalk-*：ZCode 软链将悬空，提示在返回文案里）。
/// `discard` 由壳注入（生产 trash::delete；测试重定向到临时目录），core 不直接
/// 依赖回收站实现。
pub fn trash_duplicate(
    paths: &Paths,
    name: &str,
    discard: &dyn Fn(&Path) -> Result<()>,
) -> Result<String> {
    let entry = find_entry(paths, name)?;
    discard(Path::new(&entry.path))?;
    let mut notes = vec![format!("已移入回收站：{}", entry.path)];
    let reg = Registry::load(paths)?;
    for meta in reg
        .skills
        .values()
        .filter(|m| m.name == name && m.scope == Scope::Global && m.canonical_path != entry.path)
    {
        match crate::symlink::ensure_global_claude(paths, meta) {
            Ok(()) => notes.push(format!("已补建 {} 的 global 桥接", meta.id)),
            Err(e) => notes.push(format!(
                "{} 桥接补建失败（{}），重跑 import-existing 可收敛",
                meta.id, e
            )),
        }
    }
    let local: Vec<String> = reg
        .skills
        .values()
        .filter(|m| m.name == name && m.scope == Scope::Local && m.canonical_path != entry.path)
        .map(|m| m.id.clone())
        .collect();
    if !local.is_empty() {
        notes.push(format!(
            "注意：{} 为 local scope 无 ~/.agents/skills 桥接，指向该副本的软链（如 ~/.zcode/skills）将悬空",
            local.join("、")
        ));
    }
    Ok(notes.join("；"))
}

/// 以副本覆盖池子正主（正主先送回收站，可逆），registry 不动（canonical 不变）。
/// 保守边界与 import dedupe 一致：仅当同名记录全为 unmanaged 且 canonical 一致在池；
/// 存在 managed 拒绝（正主来自源安装，覆盖破坏升级语义，引导改用 trash）。
pub fn adopt_duplicate(
    paths: &Paths,
    name: &str,
    discard: &dyn Fn(&Path) -> Result<()>,
) -> Result<String> {
    let entry = find_entry(paths, name)?;
    if entry.owner_managed {
        return Err(SkillkitError::Tool {
            message: format!(
                "{name} 的同名登记含 managed 记录（{}），池子正主来自源安装，覆盖会破坏升级语义；要换用副本请先 remove 该记录，或改用 trash 只删副本",
                entry.owner_ids.join("、")
            ),
        });
    }
    let Some(canon) = entry.owner_canonical.clone() else {
        return Err(SkillkitError::Tool {
            message: format!("{name} 的同名登记 canonical 不一致，不自动裁决，先手工归一 registry"),
        });
    };
    let canon = PathBuf::from(&canon);
    if !canon.starts_with(paths.skillkit_skills_dir()) {
        return Err(SkillkitError::Tool {
            message: format!("{name} 的正主不在池子（{}），不自动覆盖", canon.display()),
        });
    }
    if canon.is_dir() {
        discard(&canon)?;
    }
    std::fs::rename(&entry.path, &canon)?;
    let mut notes = vec![format!(
        "已用副本覆盖池子正主：{} → {}（原正主已移入回收站）",
        entry.path,
        canon.display()
    )];
    let reg = Registry::load(paths)?;
    for meta in reg
        .skills
        .values()
        .filter(|m| m.name == name && m.scope == Scope::Global)
    {
        match crate::symlink::ensure_global_claude(paths, meta) {
            Ok(()) => notes.push(format!("已补建 {} 的 global 桥接", meta.id)),
            Err(e) => notes.push(format!(
                "{} 桥接补建失败（{}），重跑 import-existing 可收敛",
                meta.id, e
            )),
        }
    }
    Ok(notes.join("；"))
}

/// 复查名字仍是待处理副本（防传错名误删正主）：非真实目录 / 无 SKILL.md /
/// 无同名登记 / canonical 指向自身（是正主不是副本）都拒绝。
fn find_entry(paths: &Paths, name: &str) -> Result<DuplicateEntry> {
    let report = list_duplicates(paths)?;
    report.entries.iter().find(|e| e.name == name).cloned().ok_or_else(|| {
        SkillkitError::Tool {
            message: format!(
                "{name} 不是待处理同名副本（不存在 / 是 symlink / registry 无同名登记 / canonical 指向它本身）；先 dupes list 确认"
            ),
        }
    })
}

/// 递归对比两个目录内容：条目集合一致且每个文件逐字节一致。
fn dirs_identical(a: &Path, b: &Path) -> bool {
    let (Ok(ea), Ok(eb)) = (std::fs::read_dir(a), std::fs::read_dir(b)) else {
        return false;
    };
    let mut na: Vec<_> = ea.flatten().map(|e| e.file_name()).collect();
    let mut nb: Vec<_> = eb.flatten().map(|e| e.file_name()).collect();
    na.sort();
    nb.sort();
    if na != nb {
        return false;
    }
    na.iter().all(|n| {
        let (pa, pb) = (a.join(n), b.join(n));
        match (
            std::fs::symlink_metadata(&pa),
            std::fs::symlink_metadata(&pb),
        ) {
            (Ok(ma), Ok(mb)) if ma.is_dir() && mb.is_dir() => dirs_identical(&pa, &pb),
            (Ok(ma), Ok(mb)) if ma.is_file() && mb.is_file() => {
                std::fs::read(&pa).is_ok_and(|ca| std::fs::read(&pb).is_ok_and(|cb| ca == cb))
            }
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_skill(dir: &Path, name: &str, body: &str) {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("SKILL.md"), body).unwrap();
    }

    fn seed(paths: &Paths, id: &str, name: &str, scope: Scope, managed: bool, canonical: &Path) {
        let mut reg = Registry::load(paths).unwrap();
        reg.upsert(SkillMeta {
            id: id.into(),
            name: name.into(),
            source: id.split('/').next().unwrap_or("").into(),
            scope,
            version: None,
            computed_hash: managed.then(|| "hash1".to_string()),
            spec: None,
            installed_at: "t".into(),
            canonical_path: canonical.to_string_lossy().into_owned(),
        });
        reg.save(paths).unwrap();
    }

    /// 测试用假回收站：rename 到临时目录下的 .Trash 子目录（验证「从原位消失 + 可捞回」）。
    fn fake_trash(root: &Path) -> (PathBuf, impl Fn(&Path) -> Result<()>) {
        let trash_dir = root.join(".Trash");
        std::fs::create_dir_all(&trash_dir).unwrap();
        let dir = trash_dir.clone();
        (trash_dir, move |p: &Path| {
            let dest = dir.join(p.file_name().unwrap());
            std::fs::rename(p, &dest).map_err(|e| SkillkitError::Tool {
                message: e.to_string(),
            })
        })
    }

    #[test]
    fn lists_real_dir_dupes_only() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let pool = paths.skillkit_skills_dir().join("foo");
        make_skill(&paths.skillkit_skills_dir(), "foo", "pool");
        make_skill(&paths.agents_skills_dir(), "foo", "dupe");
        seed(&paths, "unmanaged/foo", "foo", Scope::Global, false, &pool);
        // symlink 不算副本
        let real = tmp.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::write(real.join("SKILL.md"), "x").unwrap();
        std::os::unix::fs::symlink(&real, paths.agents_skills_dir().join("lnk")).unwrap();
        seed(&paths, "unmanaged/lnk", "lnk", Scope::Global, false, &pool);
        // 正主（canonical 指向 agents 目录本身）不列
        make_skill(&paths.agents_skills_dir(), "owner", "owner");
        seed(
            &paths,
            "unmanaged/owner",
            "owner",
            Scope::Global,
            false,
            &paths.agents_skills_dir().join("owner"),
        );
        // 无同名登记的孤儿目录不列（那是 import-existing 的辖区）
        make_skill(&paths.agents_skills_dir(), "orphan", "orphan");

        let report = list_duplicates(&paths).unwrap();
        assert_eq!(
            report
                .entries
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>(),
            vec!["foo"],
            "只列真实目录副本，symlink/正主/孤儿不列"
        );
        let e = &report.entries[0];
        assert_eq!(e.owner_ids, vec!["unmanaged/foo"]);
        assert!(!e.owner_managed);
        assert_eq!(e.identical_with_canonical, Some(false), "内容有差异");
    }

    #[test]
    fn identical_flag_true_when_content_matches() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let body = "---\nname: foo\n---\nsame";
        let pool = paths.skillkit_skills_dir().join("foo");
        make_skill(&paths.skillkit_skills_dir(), "foo", body);
        make_skill(&paths.agents_skills_dir(), "foo", body);
        std::fs::write(
            paths.skillkit_skills_dir().join("foo/ref.md"),
            "ref content",
        )
        .unwrap();
        std::fs::write(paths.agents_skills_dir().join("foo/ref.md"), "ref content").unwrap();
        seed(&paths, "unmanaged/foo", "foo", Scope::Local, false, &pool);
        let report = list_duplicates(&paths).unwrap();
        assert_eq!(report.entries[0].identical_with_canonical, Some(true));
    }

    #[test]
    fn trash_removes_dupe_and_rebuilds_global_bridge() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let pool = paths.skillkit_skills_dir().join("foo");
        make_skill(&paths.skillkit_skills_dir(), "foo", "pool");
        make_skill(&paths.agents_skills_dir(), "foo", "stale dupe");
        seed(&paths, "skills.sh/foo", "foo", Scope::Global, true, &pool);
        let (trash_dir, discard) = fake_trash(tmp.path());

        let note = trash_duplicate(&paths, "foo", &discard).unwrap();
        let agents_link = paths.agents_skills_dir().join("foo");
        assert!(
            agents_link.is_symlink(),
            "占位清掉后 global 桥接幂等补建：{note}"
        );
        assert_eq!(
            std::fs::read_link(agents_link).unwrap(),
            pool,
            "桥接指向池子正主"
        );
        assert!(
            trash_dir.join("foo").join("SKILL.md").exists(),
            "副本进回收站可捞回"
        );
    }

    #[test]
    fn trash_rejects_non_dupe_names() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        // canonical 指向 agents 目录本身：是正主不是副本，必须拒绝
        make_skill(&paths.agents_skills_dir(), "foo", "owner");
        seed(
            &paths,
            "unmanaged/foo",
            "foo",
            Scope::Global,
            false,
            &paths.agents_skills_dir().join("foo"),
        );
        let (_, discard) = fake_trash(tmp.path());
        assert!(
            trash_duplicate(&paths, "foo", &discard).is_err(),
            "正主不可删"
        );
        assert!(trash_duplicate(&paths, "nope", &discard).is_err());
    }

    #[test]
    fn adopt_replaces_pool_canonical_and_rebuilds_bridge() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let pool = paths.skillkit_skills_dir().join("foo");
        make_skill(&paths.skillkit_skills_dir(), "foo", "old pool version");
        make_skill(&paths.agents_skills_dir(), "foo", "new dupe version");
        seed(&paths, "unmanaged/foo", "foo", Scope::Global, false, &pool);
        let (trash_dir, discard) = fake_trash(tmp.path());

        let note = adopt_duplicate(&paths, "foo", &discard).unwrap();
        let canon = PathBuf::from(
            Registry::load(&paths)
                .unwrap()
                .get("unmanaged/foo")
                .unwrap()
                .canonical_path
                .clone(),
        );
        assert_eq!(
            std::fs::read_to_string(canon.join("SKILL.md")).unwrap(),
            "new dupe version",
            "池子被副本覆盖，registry canonical 不变"
        );
        assert!(
            trash_dir.join("foo").join("SKILL.md").exists(),
            "原正主进回收站可捞回"
        );
        assert!(
            paths.agents_skills_dir().join("foo").is_symlink(),
            "rename 腾出的桥接位幂等补建：{note}"
        );
        // 覆盖后不再有副本可列
        assert!(list_duplicates(&paths).unwrap().entries.is_empty());
    }

    #[test]
    fn adopt_rejects_managed_owner_and_divergent_canonicals() {
        let tmp = tempdir().unwrap();
        let paths = Paths::new(tmp.path().to_path_buf());
        let pool = paths.skillkit_skills_dir().join("foo");
        make_skill(&paths.skillkit_skills_dir(), "foo", "pool");
        make_skill(&paths.agents_skills_dir(), "foo", "dupe");
        seed(&paths, "skills.sh/foo", "foo", Scope::Global, true, &pool);
        let (_, discard) = fake_trash(tmp.path());

        let err = adopt_duplicate(&paths, "foo", &discard).unwrap_err();
        assert!(
            err.to_string().contains("managed"),
            "managed 正主拒绝覆盖：{err}"
        );

        // canonical 不一致（两条 owner 各指一处）：保守拒绝
        //（先摘 managed 记录，否则走的是 managed 拒绝分支）
        let mut reg = Registry::load(&paths).unwrap();
        reg.remove("skills.sh/foo").unwrap();
        reg.upsert(SkillMeta {
            id: "unmanaged/foo".into(),
            name: "foo".into(),
            source: "unmanaged".into(),
            scope: Scope::Global,
            version: None,
            computed_hash: None,
            spec: None,
            installed_at: "t".into(),
            canonical_path: pool.to_string_lossy().into_owned(),
        });
        reg.upsert(SkillMeta {
            id: "dc/foo".into(),
            name: "foo".into(),
            source: "dc".into(),
            scope: Scope::Local,
            version: None,
            computed_hash: None,
            spec: None,
            installed_at: "t".into(),
            canonical_path: tmp
                .path()
                .join("elsewhere/foo")
                .to_string_lossy()
                .into_owned(),
        });
        reg.save(&paths).unwrap();
        let err = adopt_duplicate(&paths, "foo", &discard).unwrap_err();
        assert!(err.to_string().contains("不一致"), "{err}");

        // owner 只剩池外 canonical 单条：同样保守拒绝
        let mut reg = Registry::load(&paths).unwrap();
        reg.remove("unmanaged/foo").unwrap();
        reg.save(&paths).unwrap();
        let err = adopt_duplicate(&paths, "foo", &discard).unwrap_err();
        assert!(err.to_string().contains("不在池子"), "{err}");
    }
}
