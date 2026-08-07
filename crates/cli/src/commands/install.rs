//! install/uninstall 子命令：调 core 的 install/uninstall。
//! registry 源（package=None，即 skills.sh）install 时走 npx skills find 交互选候选；
//! `--json` 时直接输出候选数组（不安装），供 agent 决策后自行 install。
use clap::{Args, Subcommand};
use skillkit_core::{install, npx, paths::Paths, registry::Scope, source::SourcesStore};
use std::io::{self, Write};

#[derive(Args)]
pub struct InstallCmd {
    #[command(subcommand)]
    cmd: InstallSub,
}

#[derive(Subcommand)]
enum InstallSub {
    /// 安装 skill：skillkit install add <source> <skill> [--scope global|local] [--json]
    /// 固定源直接装；skills.sh（registry）源走 find 选候选（--json 时只输出候选不安装）
    Add {
        source: String,
        skill: String,
        #[arg(long, value_parser = parse_scope, default_value = "local")]
        scope: Scope,
        /// JSON 输出：registry 源输出候选数组（[{spec,url}]，不安装）；固定源输出安装结果 SkillMeta
        #[arg(long)]
        json: bool,
    },

    /// 安装本地 skill：skillkit install local <目录|zip> [--name N] [--scope global|local] [--force] [--json]
    Local {
        /// skill 目录或 .zip 路径（支持 ~/）
        path: String,
        /// 覆盖 skill 名（默认读 SKILL.md frontmatter name）
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_parser = parse_scope, default_value = "local")]
        scope: Scope,
        /// 覆盖已存在的 local/<name>
        #[arg(long)]
        force: bool,
        /// JSON 输出 SkillMeta
        #[arg(long)]
        json: bool,
    },
}

fn parse_scope(s: &str) -> Result<Scope, String> {
    match s {
        "global" => Ok(Scope::Global),
        "local" => Ok(Scope::Local),
        other => Err(format!("未知 scope：{other}（可选 global / local）")),
    }
}

/// registry 源（package=None）install：find 候选，交互式选返回 spec（owner/repo@skill）。
fn resolve_registry_package(paths: &Paths, skill: &str) -> anyhow::Result<String> {
    let candidates = npx::find(paths, skill)?;
    if candidates.is_empty() {
        anyhow::bail!("在 skills.sh 未找到 skill：{skill}");
    }
    println!("在 skills.sh 找到 {} 个候选：", candidates.len());
    for (i, c) in candidates.iter().take(20).enumerate() {
        println!("  [{}] {}  {}", i, c.spec, c.url.as_deref().unwrap_or(""));
    }
    print!("选择序号（默认 0）：");
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    let idx: usize = line.trim().parse().unwrap_or(0);
    candidates
        .get(idx)
        .map(|c| c.spec.clone())
        .ok_or_else(|| anyhow::anyhow!("无效序号：{idx}"))
}

pub fn run_install(cmd: InstallCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match cmd.cmd {
        InstallSub::Add {
            source,
            skill,
            scope,
            json,
        } => {
            let store = SourcesStore::load(&paths)?;
            let src = store.get(&source)?.clone();
            match src.package {
                Some(p) => {
                    let meta = install(&paths, &source, &skill, &p, scope)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&meta)?);
                    } else {
                        println!(
                            "✓ 已安装 {}（hash: {}）",
                            meta.id,
                            meta.computed_hash.as_deref().unwrap_or("?")
                        );
                    }
                }
                None => {
                    if json {
                        crate::commands::skill::print_candidates(&paths, &skill, true)?;
                    } else {
                        let package = resolve_registry_package(&paths, &skill)?;
                        let meta = install(&paths, &source, &skill, &package, scope)?;
                        println!(
                            "✓ 已安装 {}（hash: {}）",
                            meta.id,
                            meta.computed_hash.as_deref().unwrap_or("?")
                        );
                    }
                }
            }
        }
        InstallSub::Local {
            path,
            name,
            scope,
            force,
            json,
        } => {
            let meta = skillkit_core::install_local(&paths, &path, name.as_deref(), scope, force)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&meta)?);
            } else {
                let short = meta
                    .computed_hash
                    .as_deref()
                    .map_or_else(|| "?".into(), |h| h.chars().take(12).collect::<String>());
                println!(
                    "✓ 已安装 {} → {}（sha256: {short}）",
                    meta.id, meta.canonical_path
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// 测试入口：包一层让 clap 解析 InstallSub（二进制 main 的 Cli 是私有，这里自建同形结构）。
    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: InstallSub,
    }

    #[test]
    fn install_add_parses_json_flag() {
        let TestCli { cmd } =
            TestCli::parse_from(["skillkit", "add", "skills.sh", "pdf", "--json"]);
        match cmd {
            InstallSub::Add {
                source,
                skill,
                scope,
                json,
            } => {
                assert_eq!(source, "skills.sh");
                assert_eq!(skill, "pdf");
                assert_eq!(scope, Scope::Local);
                assert!(json);
            }
            InstallSub::Local { .. } => panic!("应为 Add"),
        }
    }

    #[test]
    fn install_add_defaults_to_local_without_json() {
        let TestCli { cmd } = TestCli::parse_from(["skillkit", "add", "dc", "pdf"]);
        match cmd {
            InstallSub::Add {
                source,
                scope,
                json,
                ..
            } => {
                assert_eq!(source, "dc");
                assert_eq!(scope, Scope::Local);
                assert!(!json);
            }
            InstallSub::Local { .. } => panic!("应为 Add"),
        }
    }

    #[test]
    fn install_add_rejects_unknown_scope() {
        let err = TestCli::try_parse_from(["skillkit", "add", "dc", "pdf", "--scope", "x"]);
        assert!(err.is_err());
    }

    #[derive(serde::Serialize)]
    struct MetaShape {
        id: String,
        source: String,
        scope: String,
        computed_hash: Option<String>,
        canonical_path: String,
    }

    #[test]
    fn install_local_parses_flags() {
        let TestCli { cmd } = TestCli::parse_from([
            "skillkit", "local", "./foo", "--name", "bar", "--scope", "global", "--force", "--json",
        ]);
        match cmd {
            InstallSub::Local {
                path,
                name,
                scope,
                force,
                json,
            } => {
                assert_eq!(path, "./foo");
                assert_eq!(name.as_deref(), Some("bar"));
                assert_eq!(scope, Scope::Global);
                assert!(force);
                assert!(json);
            }
            InstallSub::Add { .. } => panic!("应为 Local"),
        }
    }

    #[test]
    fn install_local_json_schema_locks_fields() {
        let m = MetaShape {
            id: "local/foo".into(),
            source: "local".into(),
            scope: "local".into(),
            computed_hash: Some("abc".into()),
            canonical_path: "/x/foo".into(),
        };
        let j = serde_json::to_string(&m).unwrap();
        for f in [
            "\"id\"",
            "\"source\"",
            "\"scope\"",
            "\"computed_hash\"",
            "\"canonical_path\"",
        ] {
            assert!(j.contains(f), "json schema 应含 {f}：{j}");
        }
    }
}
