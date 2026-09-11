//! dupes 子命令：处理 ~/.agents/skills/ 下与 registry 同名的冗余副本
//! （import 按名去重跳过的那批）。list 列出并对比内容；trash 删至系统回收站；
//! adopt 以副本覆盖池子正主（正主先送回收站，可逆）。
use clap::{Parser, Subcommand};
use skillkit_core::{
    adopt_duplicate, list_duplicates, paths::Paths, system_trash, trash_duplicate,
};

#[derive(Parser)]
pub struct DupesCmd {
    #[command(subcommand)]
    cmd: DupesSub,
}

#[derive(Subcommand)]
enum DupesSub {
    /// 列出同名副本（含与池子正主的内容对比）
    List,
    /// 把副本移入系统回收站（可从废纸篓捞回），可一次多个名字
    Trash {
        /// 副本名（dupes list 里的名字），至少一个
        #[arg(required = true, num_args = 1..)]
        names: Vec<String>,
    },
    /// 以副本覆盖池子正主（正主移入回收站；仅 unmanaged 登记）
    Adopt { name: String },
}

pub fn run(cmd: DupesCmd) -> anyhow::Result<()> {
    let paths = Paths::production();
    match cmd.cmd {
        DupesSub::List => {
            let report = list_duplicates(&paths)?;
            if report.entries.is_empty() {
                println!("无同名副本，无需处理。");
                return Ok(());
            }
            println!(
                "{} 个同名副本（~/.agents/skills/ 下不被 registry 认领的目录）：",
                report.entries.len()
            );
            for e in &report.entries {
                let same = match e.identical_with_canonical {
                    Some(true) => "与池子正主内容一致",
                    Some(false) => "与池子正主内容有差异",
                    None => "无法与池子正主对比（canonical 不一致/不在池子）",
                };
                let scope = if e.owner_managed {
                    "，含 managed 登记"
                } else {
                    ""
                };
                println!(
                    "  - {}\n    {same}；占用方：{}{scope}",
                    e.name,
                    e.owner_ids.join("、")
                );
            }
            println!(
                "处理：skillkit dupes trash <name>... 删至回收站；skillkit dupes adopt <name> 以副本覆盖池子正主"
            );
        }
        DupesSub::Trash { names } => {
            for name in &names {
                let note = trash_duplicate(&paths, name, &system_trash)?;
                println!("{name}：{note}");
            }
        }
        DupesSub::Adopt { name } => {
            let note = adopt_duplicate(&paths, &name, &system_trash)?;
            println!("{note}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dupes_cmd_parses_subcommands() {
        let cmd = DupesCmd::parse_from(["skillkit", "list"]);
        assert!(matches!(cmd.cmd, DupesSub::List));

        let cmd = DupesCmd::parse_from(["skillkit", "trash", "a", "b"]);
        match cmd.cmd {
            DupesSub::Trash { names } => assert_eq!(names, vec!["a", "b"]),
            _ => panic!("应为 trash"),
        }

        // trash 不带名字应被 clap 拒绝（防误删全部）
        assert!(DupesCmd::try_parse_from(["skillkit", "trash"]).is_err());

        let cmd = DupesCmd::parse_from(["skillkit", "adopt", "foo"]);
        match cmd.cmd {
            DupesSub::Adopt { name } => assert_eq!(name, "foo"),
            _ => panic!("应为 adopt"),
        }
    }
}
