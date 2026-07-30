//! git 操作封装（走系统 git，避免 libgit2 编译依赖）。skill 源本就是 git，系统
//! git 必然存在。
use crate::error::{Result, SkillkitError};
use std::path::Path;
use std::process::Command;

/// clone 仓库到 target，可选 checkout 指定 ref。返回 HEAD 的 commit_sha。
pub fn clone(url: &str, target: &Path, ref_: Option<&str>) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(["clone", "--quiet", url]).arg(target);
    if !run(&mut cmd)? {
        return Err(SkillkitError::Git {
            message: format!("clone 失败：{url}"),
        });
    }
    if let Some(r) = ref_ {
        let ok = run(Command::new("git")
            .arg("-C")
            .arg(target)
            .args(["checkout", "--quiet", r]))?;
        if !ok {
            return Err(SkillkitError::Git {
                message: format!("checkout {r} 失败"),
            });
        }
    }
    rev_parse(target)
}

/// 返回 target 仓库 HEAD 的 commit_sha。
pub fn rev_parse(target: &Path) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(target)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| SkillkitError::Git {
            message: e.to_string(),
        })?;
    if !out.status.success() {
        return Err(SkillkitError::Git {
            message: String::from_utf8_lossy(&out.stderr).into(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn run(cmd: &mut Command) -> Result<bool> {
    Ok(cmd
        .status()
        .map_err(|e| SkillkitError::Git {
            message: e.to_string(),
        })?
        .success())
}
