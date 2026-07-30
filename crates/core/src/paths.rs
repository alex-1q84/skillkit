//! 用户目录路径解析。生产用真实 home，测试注入 tempdir，保证可测且不硬编码。
use std::path::PathBuf;

/// 路径根。生产环境指向真实 $HOME，测试用任意目录注入。
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
    pub fn skm_dir(&self) -> PathBuf {
        self.home.join(".skm")
    }

    /// 全局公共 skill 的 canonical（Cursor 等直读，Claude 需 symlink 桥接）。
    pub fn agents_skills_dir(&self) -> PathBuf {
        self.home.join(".agents").join("skills")
    }

    /// Claude 全局 skill 目录（symlink 落到这里）。
    pub fn claude_skills_dir(&self) -> PathBuf {
        self.home.join(".claude").join("skills")
    }

    /// 项目 local skill 的集中 canonical（M0 装进来，M1 才 per-project 落地）。
    pub fn skm_skills_dir(&self) -> PathBuf {
        self.skm_dir().join("skills")
    }

    pub fn sources_path(&self) -> PathBuf {
        self.skm_dir().join("sources.toml")
    }

    pub fn registry_path(&self) -> PathBuf {
        self.skm_dir().join("registry.json")
    }

    pub fn config_path(&self) -> PathBuf {
        self.skm_dir().join("config.toml")
    }

    /// profile 注册表目录（~/.skm/profiles/）。
    pub fn profiles_dir(&self) -> PathBuf {
        self.skm_dir().join("profiles")
    }

    /// project 注册表目录（~/.skm/projects/）。
    pub fn projects_dir(&self) -> PathBuf {
        self.skm_dir().join("projects")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_and_injected_share_layout() {
        let p = Paths::new(PathBuf::from("/tmp/fakehome"));
        assert_eq!(p.skm_dir(), PathBuf::from("/tmp/fakehome/.skm"));
        assert_eq!(
            p.agents_skills_dir(),
            PathBuf::from("/tmp/fakehome/.agents/skills")
        );
        assert_eq!(
            p.claude_skills_dir(),
            PathBuf::from("/tmp/fakehome/.claude/skills")
        );
        assert_eq!(
            p.skm_skills_dir(),
            PathBuf::from("/tmp/fakehome/.skm/skills")
        );
        assert_eq!(
            p.sources_path(),
            PathBuf::from("/tmp/fakehome/.skm/sources.toml")
        );
        assert_eq!(
            p.registry_path(),
            PathBuf::from("/tmp/fakehome/.skm/registry.json")
        );
        assert_eq!(
            p.config_path(),
            PathBuf::from("/tmp/fakehome/.skm/config.toml")
        );
        assert_eq!(
            p.profiles_dir(),
            PathBuf::from("/tmp/fakehome/.skm/profiles")
        );
        assert_eq!(
            p.projects_dir(),
            PathBuf::from("/tmp/fakehome/.skm/projects")
        );
    }
}
