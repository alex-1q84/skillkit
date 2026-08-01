use axum::body::Body;
use axum::response::Response;
use http_body_util::BodyExt;
use skillkit_core::Paths;
use skillkit_server::AppState;
use std::path::PathBuf;

/// 固定 token 的测试 AppState（home 指向 fake 路径；写文件的测试自建 tempdir state）。
pub fn test_state() -> AppState {
    AppState {
        paths: Paths::new(PathBuf::from("/tmp/skillkit-fakehome")),
        token: "test-token".to_string(),
    }
}

/// 同名 token 拼接，便于视图测试构造 uri（后续视图 task 用）。
#[allow(dead_code)]
pub fn uri(path: &str) -> String {
    format!("/test-token/{path}")
}

pub async fn body_string(resp: Response<Body>) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).to_string()
}

/// 前置一个假 npx 到 PATH，响应 skills@latest 的 find/add/update。
/// RAII guard：drop 还原 PATH，避免污染其他测试。
pub struct NpxGuard {
    old_path: String,
}

impl Drop for NpxGuard {
    fn drop(&mut self) {
        if self.old_path.is_empty() {
            std::env::remove_var("PATH");
        } else {
            std::env::set_var("PATH", &self.old_path);
        }
    }
}

/// 在 paths.skillkit_dir()/bin 放假 npx，前置 PATH。cwd（skillkit_dir）由 core 的 npx() 设置，
/// 假 npx 在 cwd 写 skills-lock.json / .agents/skills，与真实 npx skills 行为同构。
pub fn fake_npx(paths: &Paths) -> NpxGuard {
    let bin = paths.skillkit_dir().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let sh = bin.join("npx");
    std::fs::write(
        &sh,
        "#!/bin/sh\n\
         if [ \"$1\" = \"skills@latest\" ] && [ \"$2\" = \"find\" ]; then\n\
         \x20 echo \"owner/repo@$3  1K installs  https://skills.sh/owner/repo/$3\"\n\
         \x20 exit 0\n\
         fi\n\
         if [ \"$1\" = \"skills@latest\" ] && [ \"$2\" = \"add\" ]; then\n\
         \x20 skill=\"$5\"\n\
         \x20 mkdir -p \".agents/skills/$skill\"\n\
         \x20 printf -- '---\\nname: %s\\n---\\n# %s\\n' \"$skill\" \"$skill\" > \".agents/skills/$skill/SKILL.md\"\n\
         \x20 printf '{\"skills\":{\"%s\":{\"computedHash\":\"hashnew\"}}}' \"$skill\" > skills-lock.json\n\
         \x20 exit 0\n\
         fi\n\
         if [ \"$1\" = \"skills@latest\" ] && [ \"$2\" = \"update\" ]; then\n\
         \x20 printf '{\"skills\":{\"%s\":{\"computedHash\":\"hashnew\"}}}' \"$3\" > skills-lock.json\n\
         \x20 exit 0\n\
         fi\n\
         exit 1\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let old = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", bin.display(), old));
    NpxGuard { old_path: old }
}
