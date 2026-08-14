//! CLI 全链路 e2e（assert_cmd 驱动真实 skillkit 二进制 + 临时 HOME 隔离）。
//! 覆盖 M3+ 手动验证的 CLI 场景：import-existing / upgrade 冲突交互 / remove 保护 / find / list。
//!
//! BDD 风格：每个测试按 Given（前置）/ When（动作）/ Then（断言）组织。
//! 依赖编译出的 skillkit 二进制（assert_cmd cargo_bin）与系统 npx skills。
//! 需要 npx 的用例标 #[ignore]（如 m0/m3 端到端），日常 make check 跳过。

use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// 带临时 HOME 的测试环境。
struct Env {
    home: TempDir,
}

impl Env {
    fn new() -> Self {
        Self {
            home: tempfile::tempdir().unwrap(),
        }
    }

    fn home_path(&self) -> &Path {
        self.home.path()
    }

    /// 建一个 SKILL.md 存量目录（模拟用户手工放置）。
    fn make_skill(&self, dir: &str, name: &str) -> PathBuf {
        let p = self.home_path().join(dir).join(name);
        fs::create_dir_all(&p).unwrap();
        fs::write(
            p.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: e2e fixture\n---\n# {name}\n"),
        )
        .unwrap();
        p
    }

    /// 建带 .git 的本地 skill 仓库（可溯源源），返回仓库根。
    fn git_fixture(&self, skill: &str) -> PathBuf {
        let repo = self.home_path().join("fixture-repo");
        let skill_dir = repo.join(skill);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!("---\nname: {skill}\ndescription: e2e fixture\n---\n# {skill}\n"),
        )
        .unwrap();
        run_git(&repo, &["init", "-q"]);
        run_git(&repo, &["config", "user.email", "e2e@test"]);
        run_git(&repo, &["config", "user.name", "e2e"]);
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-q", "-m", "init"]);
        repo
    }

    /// skillkit 命令（已设临时 HOME），追加参数返回。
    fn skillkit(&self) -> Command {
        let mut c = Command::new(assert_cmd::cargo::cargo_bin("skillkit"));
        c.env("HOME", self.home_path());
        c
    }
}

fn run_git(dir: &Path, args: &[&str]) {
    let st = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?} 失败");
}

/// 读临时 HOME 下 registry.json 的 skill id 列表。
fn registry_ids(env: &Env) -> Vec<String> {
    let p = env.home_path().join(".skillkit/registry.json");
    if !p.exists() {
        return Vec::new();
    }
    let content = fs::read_to_string(p).unwrap();
    let reg: serde_json::Value = serde_json::from_str(&content).unwrap();
    reg["skills"].as_object().unwrap().keys().cloned().collect()
}

// ===========================================================================
// import-existing
// ===========================================================================

#[test]
fn import_existing_registers_unmanaged_and_skips_invalid() {
    // Given：agents 目录有存量 skill，claude 目录有真实目录 + symlink
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-a");
    env.make_skill(".codex/skills", "legacy-b");
    env.make_skill(".claude/skills", "legacy-c");
    fs::create_dir_all(env.home_path().join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(
        env.home_path().join(".agents/skills/legacy-a"),
        env.home_path().join(".claude/skills/legacy-a-link"),
    )
    .unwrap();
    // 无 SKILL.md 的目录 → 跳过
    fs::create_dir_all(env.home_path().join(".codex/skills/no-md")).unwrap();

    // When：跑 import-existing
    let out = env.skillkit().args(["import-existing"]).assert().success();

    // Then：三个真实目录登记 unmanaged，symlink/无 SKILL.md 跳过
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(stdout.contains("imported 3"), "应导入 3 个：{stdout}");
    let ids = registry_ids(&env);
    assert!(ids.contains(&"unmanaged/legacy-a".to_string()));
    assert!(ids.contains(&"unmanaged/legacy-b".to_string()));
    assert!(ids.contains(&"unmanaged/legacy-c".to_string()));
    assert!(
        !ids.contains(&"unmanaged/legacy-a-link".to_string()),
        "symlink 跳过"
    );
    assert!(
        !ids.contains(&"unmanaged/no-md".to_string()),
        "无 SKILL.md 跳过"
    );
}

#[test]
fn import_existing_dry_run_writes_nothing() {
    // Given：agents 目录有存量 skill
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-a");

    // When：dry-run
    let out = env
        .skillkit()
        .args(["import-existing", "--dry-run"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(
        stdout.contains("imported 1"),
        "dry-run 应报告会导入 1 个：{stdout}"
    );

    // Then：不写 registry
    assert!(registry_ids(&env).is_empty(), "dry-run 不应写 registry");
}

#[test]
fn import_existing_is_idempotent() {
    // Given：agents 目录有存量 skill
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-a");

    // When：跑两次
    env.skillkit().args(["import-existing"]).assert().success();
    let out = env.skillkit().args(["import-existing"]).assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();

    // Then：第二次 imported 0（同名已登记，跳过），registry 仍 1 条
    assert!(stdout.contains("imported 0"), "第二次应 0 导入：{stdout}");
    let ids = registry_ids(&env);
    assert_eq!(ids.len(), 1);
    assert!(ids.contains(&"unmanaged/legacy-a".to_string()));
}

#[test]
fn import_existing_json_emits_import_report() {
    // Given：agents 目录有存量 skill
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-a");

    // When：--json
    let out = env
        .skillkit()
        .args(["import-existing", "--json"])
        .assert()
        .success();

    // Then：输出 ImportReport 结构（imported/unmanaged/reinstalled/skipped）
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["imported"], serde_json::json!(["legacy-a"]));
    assert_eq!(v["unmanaged"], serde_json::json!(["legacy-a"]));
    assert_eq!(v["reinstalled"], serde_json::json!([]));
}

// ===========================================================================
// find（真跑 npx skills find）
// ===========================================================================

#[test]
#[ignore = "需真跑 npx skills find（联网）；cargo test -- --ignored 手动跑"]
fn find_json_returns_candidate_array() {
    // Given/When：find pdf --json（query 选 skills.sh 上确实存在的 skill 名）
    let env = Env::new();
    let out = env
        .skillkit()
        .args(["find", "pdf", "--json"])
        .assert()
        .success();
    // Then：stdout 是 JSON 数组，元素含 spec 字段（不断言具体值，skills.sh 内容会变）
    let body: serde_json::Value =
        serde_json::from_slice(&out.get_output().stdout).expect("find --json 应输出合法 JSON 数组");
    let arr = body.as_array().expect("应为数组");
    assert!(!arr.is_empty(), "pdf 应至少有一个候选");
    assert!(arr[0].get("spec").is_some(), "候选元素含 spec 字段");
}

// ===========================================================================
// list（不依赖 npx）
// ===========================================================================

#[test]
fn list_marks_unmanaged_skill() {
    // Given：import-existing 登记一个 unmanaged skill
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-b");
    env.skillkit().args(["import-existing"]).assert().success();

    // When：list（人看输出）
    let out = env.skillkit().args(["list"]).assert().success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);

    // Then：输出含该 skill 且标 unmanaged
    assert!(
        stdout.contains("unmanaged/legacy-b"),
        "list 应列出 unmanaged skill"
    );
    assert!(stdout.contains("unmanaged"), "unmanaged 行应有标识");

    // And：--json 输出含 id 与 computed_hash=null
    let outj = env.skillkit().args(["list", "--json"]).assert().success();
    let v: serde_json::Value = serde_json::from_slice(&outj.get_output().stdout).unwrap();
    assert_eq!(v[0]["id"], "unmanaged/legacy-b");
    assert!(v[0]["computed_hash"].is_null());
}

// ===========================================================================
// remove 确认交互（unmanaged，不依赖 npx）
// ===========================================================================

#[test]
fn remove_unmanaged_default_confirm_with_stdin_y() {
    // Given：import 登记 unmanaged
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-c");
    env.skillkit().args(["import-existing"]).assert().success();

    // When：默认确认，stdin 给 y
    env.skillkit()
        .args(["remove", "unmanaged/legacy-c"])
        .write_stdin("y\n")
        .assert()
        .success();

    // Then：目录保留（unmanaged 保护），registry 移除
    assert!(
        env.home_path().join(".agents/skills/legacy-c").exists(),
        "unmanaged 目录不能被删"
    );
    assert!(registry_ids(&env).is_empty(), "registry 记录应移除");
}

#[test]
fn remove_cancel_with_stdin_n_keeps_registry() {
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-d");
    env.skillkit().args(["import-existing"]).assert().success();

    // stdin 给 n → 取消，registry 记录保留
    env.skillkit()
        .args(["remove", "unmanaged/legacy-d"])
        .write_stdin("n\n")
        .assert()
        .success();
    assert!(
        registry_ids(&env).contains(&"unmanaged/legacy-d".to_string()),
        "取消则 registry 记录保留"
    );
}

#[test]
fn remove_yes_skips_confirm_and_json_implies_yes() {
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-e");
    env.skillkit().args(["import-existing"]).assert().success();

    // --json 隐含跳过确认，输出 {id, removed_canonical:false}
    let out = env
        .skillkit()
        .args(["remove", "unmanaged/legacy-e", "--json"])
        .assert()
        .success();
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["id"], "unmanaged/legacy-e");
    assert_eq!(v["removed_canonical"], false);
    assert!(registry_ids(&env).is_empty());
}

// ===========================================================================
// remove managed（真跑 npx install，验证删目录）
// ===========================================================================

#[test]
#[ignore = "需真跑 npx skills 装 local source；cargo test -- --ignored 手动跑"]
fn remove_managed_deletes_canonical_directory() {
    // Given：装一个 local source managed skill
    let env = Env::new();
    install_local_skill(&env, "dc", "pdf");
    // When：--yes remove（跳过确认）
    let out = env
        .skillkit()
        .args(["remove", "dc/pdf", "--yes", "--json"])
        .assert()
        .success();
    // Then：--json removed_canonical=true；canonical 目录已删；registry 移除
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["removed_canonical"], true);
    assert!(
        !env.home_path()
            .join(".skillkit/.agents/skills/pdf")
            .exists(),
        "managed canonical 目录应被删"
    );
    assert!(registry_ids(&env).is_empty(), "registry 记录应移除");
}

// ===========================================================================
// upgrade
// ===========================================================================

/// 装一个 local source skill 到临时 HOME（source add + install local）。
fn install_local_skill(env: &Env, source_name: &str, skill: &str) {
    let repo = env.git_fixture(skill);
    let pkg = repo.to_string_lossy().into_owned();
    env.skillkit()
        .args(["source", "add", &pkg, "--name", source_name])
        .assert()
        .success();
    env.skillkit()
        .args(["install", "add", source_name, skill, "--scope", "local"])
        .assert()
        .success();
}

/// 建 project 并把 skill 加入 + apply（锁 hash）。
fn lock_skill_in_project(env: &Env, skill_id: &str) -> String {
    let proj_dir = env.home_path().join("myproj");
    fs::create_dir_all(&proj_dir).unwrap();
    env.skillkit()
        .args(["project", "add", &proj_dir.to_string_lossy()])
        .assert()
        .success();
    let pid = fs::read_dir(env.home_path().join(".skillkit/projects"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .replace(".toml", "");
    env.skillkit()
        .args(["project", "add-skill", &pid, skill_id])
        .assert()
        .success();
    env.skillkit()
        .args(["project", "apply", &pid])
        .assert()
        .success();
    pid
}

#[test]
#[ignore = "需真跑 npx skills 下载 local source；cargo test -- --ignored 手动跑"]
fn upgrade_with_conflict_prompts_and_cancel() {
    // Given：装了 local skill 并锁进 project
    let env = Env::new();
    install_local_skill(&env, "src", "demo-skill");
    let pid = lock_skill_in_project(&env, "src/demo-skill");

    // When：upgrade 不带 --yes，stdin 输入 n（取消）
    let out = env
        .skillkit()
        .args(["upgrade", "src/demo-skill"])
        .write_stdin("n\n")
        .assert()
        .code(1);

    // Then：受影响项目列在 stdout，取消错误在 stderr，exit 1 不升级
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(stdout.contains(&pid), "应列出受影响项目 {pid}：{stdout}");
    assert!(stderr.contains("已取消升级"), "应输出取消提示：{stderr}");
}

#[test]
#[ignore = "需真跑 npx skills；cargo test -- --ignored 手动跑"]
fn upgrade_with_yes_skips_confirmation() {
    // Given：装了 local skill 并锁进 project
    let env = Env::new();
    install_local_skill(&env, "src", "demo-skill");
    lock_skill_in_project(&env, "src/demo-skill");

    // When：upgrade --yes
    let out = env
        .skillkit()
        .args(["upgrade", "src/demo-skill", "--yes"])
        .assert()
        .success();

    // Then：不提示、成功、列出受影响项目需重新 apply
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(stdout.contains("已升级"), "应输出已升级：{stdout}");
    assert!(
        stdout.contains("需重新 apply"),
        "应提示项目需重新 apply：{stdout}"
    );
}

#[test]
#[ignore = "需真跑 npx skills；cargo test -- --ignored 手动跑"]
fn upgrade_json_conflict_goes_to_stderr() {
    // Given：装了 local skill 并锁进 project
    let env = Env::new();
    install_local_skill(&env, "src", "demo-skill");
    lock_skill_in_project(&env, "src/demo-skill");

    // When：upgrade --json（冲突，无 --yes）
    let out = env
        .skillkit()
        .args(["upgrade", "src/demo-skill", "--json"])
        .assert()
        .code(1);

    // Then：stdout 空，stderr 是 JSON 错误（机器可读错误走 stderr）
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.get_output().stderr).to_string();
    assert!(stdout.is_empty(), "stdout 应为空：{stdout}");
    let v: serde_json::Value = serde_json::from_str(&stderr).unwrap();
    assert_eq!(v["error"], "upgrade_blocked");
    assert_eq!(v["id"], "src/demo-skill");
    assert!(!v["affected"].as_array().unwrap().is_empty());
}

#[test]
#[ignore = "需真跑 npx skills；cargo test -- --ignored 手动跑"]
fn upgrade_all_lists_blocked_not_abort() {
    // Given：装了 local skill 并锁进 project
    let env = Env::new();
    install_local_skill(&env, "src", "demo-skill");
    lock_skill_in_project(&env, "src/demo-skill");

    // When：upgrade --all（无 --yes）
    let out = env.skillkit().args(["upgrade", "--all"]).assert().success();

    // Then：列出 blocked（受影响项目），exit 0，不交互
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    assert!(
        stdout.contains("升级将影响项目"),
        "应列出受影响项目：{stdout}"
    );
    assert!(stdout.contains("如需升级请"), "应给下一步引导：{stdout}");
}

#[test]
#[ignore = "需真跑 npx skills；cargo test -- --ignored 手动跑"]
fn upgrade_all_json_emits_upgrade_all_report() {
    // Given：装了 local skill 并锁进 project
    let env = Env::new();
    install_local_skill(&env, "src", "demo-skill");
    lock_skill_in_project(&env, "src/demo-skill");

    // When：upgrade --all --json
    let out = env
        .skillkit()
        .args(["upgrade", "--all", "--json"])
        .assert()
        .success();

    // Then：输出 UpgradeAllReport {upgraded, blocked}
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(v["blocked"].is_array());
    assert!(v["blocked"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["id"] == "src/demo-skill"));
}

// ===========================================================================
// project remove（注销：删 toml 注册信息，不碰项目目录本身）
// ===========================================================================

#[test]
fn project_remove_yes_deletes_registration_but_keeps_project_dir() {
    // Given：注册一个项目（真实目录 + 文件）
    let env = Env::new();
    let proj_dir = env.home_path().join("myproj");
    fs::create_dir_all(&proj_dir).unwrap();
    fs::write(proj_dir.join("README.md"), "hello").unwrap();
    env.skillkit()
        .args(["project", "add", &proj_dir.to_string_lossy()])
        .assert()
        .success();
    let pid = fs::read_dir(env.home_path().join(".skillkit/projects"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .replace(".toml", "");

    // When：remove --yes 注销
    let out = env
        .skillkit()
        .args(["project", "remove", &pid, "--yes"])
        .assert()
        .success();

    // Then：注册 toml 删除，项目目录与文件保留（只移除注册信息，不删项目本身）
    let stdout = String::from_utf8_lossy(&out.get_output().stdout);
    assert!(stdout.contains("已注销项目"), "应提示已注销：{stdout}");
    assert!(
        !env.home_path()
            .join(format!(".skillkit/projects/{pid}.toml"))
            .exists(),
        "注册 toml 应已删除"
    );
    assert!(proj_dir.exists(), "项目目录必须保留");
    assert!(proj_dir.join("README.md").exists(), "项目文件必须保留");
}

// ===========================================================================
// rescope
// ===========================================================================

#[test]
fn rescope_same_scope_is_noop_and_locks_json_schema() {
    // Given：import 一个 unmanaged skill（canonical 在 ~/.agents/skills/ → 登记为 global）
    let env = Env::new();
    env.make_skill(".agents/skills", "legacy-r");
    env.skillkit().args(["import-existing"]).assert().success();
    // When：rescope global→global（同 scope，noop），--yes --json 跳过确认
    let out = env
        .skillkit()
        .args(["rescope", "unmanaged/legacy-r", "global", "--yes", "--json"])
        .assert()
        .success();
    // Then：--json schema 锁定（id/from/to/affected_*），affected 空（noop）
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["id"], "unmanaged/legacy-r");
    assert_eq!(v["from"], "global");
    assert_eq!(v["to"], "global");
    assert!(v["affected_profiles"].as_array().unwrap().is_empty());
    assert!(v["affected_projects"].as_array().unwrap().is_empty());
}

#[test]
#[ignore = "需真跑 npx skills 装 local source；cargo test -- --ignored 手动跑"]
fn rescope_local_to_global_lands_symlink_and_clears_profile() {
    // Given：装 managed local skill + 归入 profile fe
    let env = Env::new();
    install_local_skill(&env, "dc", "pdf");
    env.skillkit()
        .args(["profile", "create", "fe"])
        .assert()
        .success();
    env.skillkit()
        .args(["profile", "add-skill", "fe", "dc/pdf"])
        .assert()
        .success();
    // When：rescope local→global --json（隐含跳过确认）
    let out = env
        .skillkit()
        .args(["rescope", "dc/pdf", "global", "--json"])
        .assert()
        .success();
    // Then：from/to + affected_profiles 含 fe（引用被清）
    let v: serde_json::Value = serde_json::from_slice(&out.get_output().stdout).unwrap();
    assert_eq!(v["from"], "local");
    assert_eq!(v["to"], "global");
    assert_eq!(v["affected_profiles"][0], "fe");
    // registry scope=global
    let reg: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(env.home_path().join(".skillkit/registry.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(reg["skills"]["dc/pdf"]["scope"], "global");
    // 全局 symlink 在位（ensure_global_claude 建）
    assert!(
        env.home_path().join(".agents/skills/pdf").is_symlink(),
        "local→global 应建全局 symlink"
    );
}
