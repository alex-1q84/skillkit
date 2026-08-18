//! skill 实体的查询与移除：find（搜 skills.sh）/ list（列已装）/ remove（卸载，替换 uninstall）。
//! 复用 core 的 npx::find / Registry / uninstall，cli 只做薄壳与展示。
use clap::Args;
use skillkit_core::{npx, paths::Paths, uninstall, Registry, Scope, SkillMeta};

/// find：skillkit find <query> [--json]，搜 skills.sh registry，纯展示候选不安装。
#[derive(Args)]
pub struct FindCmd {
    /// skill 名（搜 skills.sh registry）
    pub query: String,
    /// JSON 输出：候选数组 [{spec,url}]
    #[arg(long)]
    pub json: bool,
}

/// 输出 find 候选：json=true 序列化数组，否则编号列表。install 的 registry 源 --json 分支也复用。
pub fn print_candidates(paths: &Paths, query: &str, json: bool) -> anyhow::Result<()> {
    let cs = npx::find(paths, query)?;
    if cs.is_empty() {
        anyhow::bail!("在 skills.sh 未找到 skill：{query}");
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&cs)?);
    } else {
        println!("在 skills.sh 找到 {} 个候选：", cs.len());
        for (i, c) in cs.iter().take(20).enumerate() {
            println!("  [{i}] {}  {}", c.spec, c.url.as_deref().unwrap_or(""));
        }
    }
    Ok(())
}

pub fn run_find(cmd: FindCmd) -> anyhow::Result<()> {
    print_candidates(&Paths::production(), &cmd.query, cmd.json)
}

/// list：skillkit list [--json] [--unassigned]，列 registry 全部已装 skill。
#[derive(Args)]
pub struct ListCmd {
    /// JSON 输出：SkillMeta[]
    #[arg(long)]
    pub json: bool,
    /// 只列未纳入任何 profile 的 skill（local 且无主；global 不算）
    #[arg(long)]
    pub unassigned: bool,
}

fn scope_str(s: Scope) -> &'static str {
    match s {
        Scope::Global => "global",
        Scope::Local => "local",
    }
}

/// 渲染 list 表格（人看）。unmanaged（computed_hash=None）行尾标 unmanaged。
fn render_list_table(skills: &[SkillMeta]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for s in skills {
        let hash = s.computed_hash.as_deref().unwrap_or("-");
        let unm = if s.computed_hash.is_none() {
            "  (unmanaged)"
        } else {
            ""
        };
        writeln!(
            out,
            "{id}  [{scope}]  {source}  {ver}  {hash}{unm}",
            id = s.id,
            scope = scope_str(s.scope),
            source = s.source,
            ver = s.version.as_deref().unwrap_or("-"),
            hash = hash,
            unm = unm,
        )
        .unwrap();
    }
    out
}

fn render_list_json(skills: &[SkillMeta]) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(skills)?)
}

pub fn run_list(cmd: ListCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    let reg = Registry::load(&paths)?;
    let mut skills: Vec<SkillMeta> = reg.skills.values().cloned().collect();
    skills.sort_by(|a, b| a.id.cmp(&b.id));
    if cmd.unassigned {
        // 反向索引与判定调 core（web Skills 过滤视图共用，语义单点）
        let profiles_of = skillkit_core::skills_profiles_map(&paths);
        skills.retain(|m| skillkit_core::is_unassigned(m, &profiles_of));
    }
    if cmd.json {
        println!("{}", render_list_json(&skills)?);
    } else if skills.is_empty() {
        let hint = if cmd.unassigned {
            "（没有未纳入 profile 的 skill）"
        } else {
            "（registry 为空，尚无已装 skill）"
        };
        println!("{hint}");
    } else {
        print!("{}", render_list_table(&skills));
    }
    Ok(())
}

/// remove：skillkit remove <id> [--yes] [--json]，卸载 skill（完全替换 uninstall）。
/// 默认交互确认；--yes 跳过；--json 隐含跳过并输出 {id, removed_canonical}。
#[derive(Args)]
pub struct RemoveCmd {
    /// skill id，格式 <source>/<skill>
    pub id: String,
    /// 跳过交互确认
    #[arg(long)]
    pub yes: bool,
    /// JSON 输出（隐含 --yes）：{id, removed_canonical}
    #[arg(long)]
    pub json: bool,
}

pub fn run_remove(cmd: RemoveCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    // 先读 registry 判断 managed（决定 removed_canonical + 提示文案），与 uninstall 内部行为一致
    let managed = {
        let reg = Registry::load(&paths)?;
        reg.get(&cmd.id)?.computed_hash.is_some()
    };

    let skip_confirm = cmd.yes || cmd.json;
    if !skip_confirm {
        let note = if managed {
            ""
        } else {
            "（unmanaged：仅删登记，保留目录）"
        };
        println!("将删除 {id}{note}，确认？(y/n)", id = cmd.id, note = note);
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if line.trim() != "y" {
            println!("已取消");
            return Ok(());
        }
    }

    uninstall(&paths, &cmd.id)?;

    if cmd.json {
        println!(
            "{}",
            serde_json::json!({ "id": cmd.id, "removed_canonical": managed })
        );
    } else {
        let note = if managed { "" } else { "（仅删登记）" };
        println!("✓ 已卸载 {id}{note}", id = cmd.id, note = note);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};
    use skillkit_core::{Candidate, Scope, SkillMeta};

    /// 测试入口：自建同形 Parser 解析顶层命令（main.rs 的 Cli 私有，这里复刻命令变体）。
    /// 后续 task 给 TestCmd 累积追加 List/Remove 变体。
    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCmd,
    }

    #[derive(Subcommand)]
    enum TestCmd {
        Find(FindCmd),
        List(ListCmd),
        Remove(RemoveCmd),
    }

    fn meta(id: &str, scope: Scope, hash: Option<&str>) -> SkillMeta {
        SkillMeta {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(),
            scope,
            version: Some("1.0.0".into()),
            computed_hash: hash.map(str::to_string),
            installed_at: "2026-08-01T00:00:00Z".into(),
            canonical_path: format!(
                "~/.skillkit/.agents/skills/{}",
                id.rsplit('/').next().unwrap()
            ),
        }
    }

    #[test]
    fn find_parses_query_and_json() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "find", "pdf", "--json"]);
        let TestCmd::Find(FindCmd { query, json }) = cmd else {
            panic!("expected Find")
        };
        assert_eq!(query, "pdf");
        assert!(json);
    }

    #[test]
    fn find_defaults_json_false() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "find", "pdf"]);
        let TestCmd::Find(FindCmd { json, .. }) = cmd else {
            panic!("expected Find")
        };
        assert!(!json);
    }

    /// --json schema 锁定：Candidate 序列化为 {"spec","url"}（纯序列化契约，不依赖 npx）。
    #[test]
    fn find_json_schema_locks_candidate_fields() {
        let cs = vec![
            Candidate {
                spec: "anthropics/skills@pdf".into(),
                url: Some("https://skills.sh/a".into()),
            },
            Candidate {
                spec: "openai/skills@pdf".into(),
                url: None,
            },
        ];
        let json = serde_json::to_string(&cs).unwrap();
        assert_eq!(
            json,
            r#"[{"spec":"anthropics/skills@pdf","url":"https://skills.sh/a"},{"spec":"openai/skills@pdf","url":null}]"#
        );
    }

    #[test]
    fn list_parses_json_flag() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "list", "--json"]);
        let TestCmd::List(ListCmd { json, .. }) = cmd else {
            panic!("expected List")
        };
        assert!(json);
    }

    /// `list --unassigned`：flag 解析 + 默认 false。
    #[test]
    fn list_parses_unassigned_flag() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "list", "--unassigned"]);
        let TestCmd::List(ListCmd { unassigned, .. }) = cmd else {
            panic!("expected List")
        };
        assert!(unassigned);

        let TestCli { cmd } = TestCli::parse_from(["skillkit", "list"]);
        let TestCmd::List(ListCmd { unassigned, .. }) = cmd else {
            panic!("expected List")
        };
        assert!(!unassigned);
    }

    #[test]
    fn list_table_marks_unmanaged() {
        let skills = vec![
            meta("skills.sh/pdf", Scope::Global, Some("abc123")),
            meta("unmanaged/legacy", Scope::Global, None),
        ];
        let table = render_list_table(&skills);
        assert!(table.contains("skills.sh/pdf"));
        assert!(table.contains("[global]"));
        assert!(table.contains("unmanaged/legacy"));
        assert!(table.contains("unmanaged"));
    }

    /// --json schema 锁定：SkillMeta[] 字段名稳定。
    #[test]
    fn list_json_schema_locks_skillmeta_fields() {
        let skills = vec![meta("skills.sh/pdf", Scope::Local, Some("abc123"))];
        let json = render_list_json(&skills).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let obj = &v[0];
        assert_eq!(obj["id"], "skills.sh/pdf");
        assert_eq!(obj["scope"], "local");
        assert_eq!(obj["computed_hash"], "abc123");
        assert_eq!(obj["source"], "skills.sh");
        assert!(obj["installed_at"].is_string());
        assert!(obj["canonical_path"].is_string());
    }

    #[test]
    fn remove_parses_id_yes_json() {
        let TestCli { cmd } =
            TestCli::parse_from(["skillkit", "remove", "skills.sh/pdf", "--yes", "--json"]);
        let TestCmd::Remove(RemoveCmd { id, yes, json }) = cmd else {
            panic!("expected Remove")
        };
        assert_eq!(id, "skills.sh/pdf");
        assert!(yes);
        assert!(json);
    }
}
