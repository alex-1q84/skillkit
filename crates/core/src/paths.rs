//! 用户目录路径解析。生产用真实 home，测试注入 tempdir，保证可测且不硬编码。
use std::path::PathBuf;

/// 路径根。生产环境指向真实 $HOME，测试用任意目录注入。
#[derive(Clone)]
pub struct Paths {
    home: PathBuf,
}

impl Paths {
    /// 生产环境：解析真实用户 home 目录。
    pub fn production() -> Self {
        let home = dirs::home_dir().expect("无法定位用户 home 目录，请检查 $HOME 环境变量");
        Self { home }
    }

    /// 测试用：注入任意 home 根目录。
    pub fn new(home: PathBuf) -> Self {
        Self { home }
    }

    /// 元数据根目录（config/sources/registry/profiles/projects）。
    pub fn skillkit_dir(&self) -> PathBuf {
        self.home.join(".skillkit")
    }

    /// 全局公共 skill 的 canonical（Cursor 等直读，Claude 需 symlink 桥接）。
    pub fn agents_skills_dir(&self) -> PathBuf {
        self.home.join(".agents").join("skills")
    }

    /// Claude 全局 skill 目录（symlink 落到这里）。
    pub fn claude_skills_dir(&self) -> PathBuf {
        self.home.join(".claude").join("skills")
    }

    /// Codex 历史私有目录（import-existing 扫描用；新设计下 agent 直读 ~/.agents/skills/）。
    pub fn codex_skills_dir(&self) -> PathBuf {
        self.home.join(".codex").join("skills")
    }

    /// Cursor 历史私有目录。
    pub fn cursor_skills_dir(&self) -> PathBuf {
        self.home.join(".cursor").join("skills")
    }

    /// canonical 池子：所有 skill 集中存储（单版本），npx skills project scope 直接写入。
    pub fn skillkit_skills_dir(&self) -> PathBuf {
        self.skillkit_dir().join(".agents").join("skills")
    }

    pub fn sources_path(&self) -> PathBuf {
        self.skillkit_dir().join("sources.toml")
    }

    pub fn registry_path(&self) -> PathBuf {
        self.skillkit_dir().join("registry.json")
    }

    pub fn config_path(&self) -> PathBuf {
        self.skillkit_dir().join("config.toml")
    }

    /// profile 注册表目录（~/.skillkit/profiles/）。
    pub fn profiles_dir(&self) -> PathBuf {
        self.skillkit_dir().join("profiles")
    }

    /// project 注册表目录（~/.skillkit/projects/）。
    pub fn projects_dir(&self) -> PathBuf {
        self.skillkit_dir().join("projects")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_and_injected_share_layout() {
        let p = Paths::new(PathBuf::from("/tmp/fakehome"));
        assert_eq!(p.skillkit_dir(), PathBuf::from("/tmp/fakehome/.skillkit"));
        assert_eq!(
            p.agents_skills_dir(),
            PathBuf::from("/tmp/fakehome/.agents/skills")
        );
        assert_eq!(
            p.claude_skills_dir(),
            PathBuf::from("/tmp/fakehome/.claude/skills")
        );
        assert_eq!(
            p.skillkit_skills_dir(),
            PathBuf::from("/tmp/fakehome/.skillkit/.agents/skills")
        );
        assert_eq!(
            p.sources_path(),
            PathBuf::from("/tmp/fakehome/.skillkit/sources.toml")
        );
        assert_eq!(
            p.registry_path(),
            PathBuf::from("/tmp/fakehome/.skillkit/registry.json")
        );
        assert_eq!(
            p.config_path(),
            PathBuf::from("/tmp/fakehome/.skillkit/config.toml")
        );
        assert_eq!(
            p.profiles_dir(),
            PathBuf::from("/tmp/fakehome/.skillkit/profiles")
        );
        assert_eq!(
            p.projects_dir(),
            PathBuf::from("/tmp/fakehome/.skillkit/projects")
        );
        assert_eq!(
            p.codex_skills_dir(),
            PathBuf::from("/tmp/fakehome/.codex/skills")
        );
        assert_eq!(
            p.cursor_skills_dir(),
            PathBuf::from("/tmp/fakehome/.cursor/skills")
        );
    }
}
