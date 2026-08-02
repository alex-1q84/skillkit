mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

fn urlencode(s: &str) -> String {
    s.replace('/', "%2F")
}

#[tokio::test]
async fn ping_returns_pong() {
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(Request::builder().uri("/ping").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(common::body_string(resp).await, "pong");
}

#[tokio::test]
async fn protected_route_rejects_wrong_token() {
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn protected_route_accepts_right_token() {
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn static_asset_served_with_content_type() {
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/static/app.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "text/css; charset=utf-8"
    );
}

#[tokio::test]
async fn home_renders_layout_with_nav() {
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("/test-token/sources"));
    assert!(body.contains("/test-token/projects"));
    assert!(body.contains("htmx.min.js"));
}

#[tokio::test]
async fn sources_page_lists_sources() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let mut store = skillkit_core::SourcesStore::default();
    store
        .add(skillkit_core::Source {
            name: "demo".into(),
            package: Some("git@example/x.git".into()),
        })
        .unwrap();
    store.save(&state.paths).unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/sources")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::body_string(resp).await.contains("demo"));
}

#[tokio::test]
async fn fragment_response_is_main_content_only() {
    // 契约：?fragment=1 返回纯 main 内容（不含 nav），SSE 刷新用它，防导航重复。
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state);
    for path in [
        "/test-token?fragment=1",
        "/test-token/sources?fragment=1",
        "/test-token/skills?fragment=1",
        "/test-token/profiles?fragment=1",
        "/test-token/projects?fragment=1",
    ] {
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{path} 应 200");
        let body = common::body_string(resp).await;
        assert!(!body.contains("<nav"), "{path} 片段不应含导航栏");
        assert!(
            !body.contains("htmx.min.js"),
            "{path} 片段不应含 layout 脚本"
        );
    }
    // 对照：不带 fragment 的正常页含 nav。
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/sources")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(common::body_string(resp).await.contains("<nav"));
}

#[tokio::test]
async fn skills_page_lists_registry() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let mut reg = skillkit_core::Registry::default();
    reg.skills.insert(
        "demo/skill".into(),
        skillkit_core::registry::SkillMeta {
            id: "demo/skill".into(),
            name: "skill".into(),
            source: "demo".into(),
            scope: skillkit_core::Scope::Local,
            version: None,
            computed_hash: None,
            installed_at: "2026-07-31".into(),
            canonical_path: "/x".into(),
        },
    );
    reg.save(&state.paths).unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/skills")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::body_string(resp).await.contains("demo/skill"));
}

#[tokio::test]
async fn profile_add_skill_then_reorder_persists() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    skillkit_core::Profile {
        name: "fe".into(),
        description: String::new(),
        skills: Vec::new(),
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state.clone());
    for body in ["id=ab1", "id=ab2"] {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/test-token/profiles/fe/skills")
                    .header(
                        axum::http::header::CONTENT_TYPE,
                        "application/x-www-form-urlencoded",
                    )
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/profiles/fe/reorder")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("order=ab2&order=ab1"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let p = skillkit_core::Profile::load(&state.paths, "fe").unwrap();
    assert_eq!(p.skills, vec!["ab2".to_string(), "ab1".to_string()]);
}

#[tokio::test]
async fn project_workspace_renders_status() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let proj_root = dir.path().join("myproj");
    std::fs::create_dir_all(&proj_root).unwrap();
    let canon = dir.path().join("canon/logseq");
    std::fs::create_dir_all(&canon).unwrap();
    let mut reg = skillkit_core::Registry::default();
    reg.skills.insert(
        "demo/logseq".into(),
        skillkit_core::registry::SkillMeta {
            id: "demo/logseq".into(),
            name: "logseq".into(),
            source: "demo".into(),
            scope: skillkit_core::Scope::Local,
            version: None,
            computed_hash: Some("s".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        },
    );
    reg.save(&state.paths).unwrap();
    let proj = skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "myproj".into(),
        path: proj_root.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec!["demo/logseq".into()],
        locked_shas: std::collections::BTreeMap::new(),
    };
    proj.save(&state.paths).unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/projects/ABCDEF12")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("myproj"));
    assert!(body.contains("demo/logseq"));
}

#[tokio::test]
async fn source_add_persists_with_derived_name() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/sources")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("package=git%40example/x.git"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let store = skillkit_core::SourcesStore::load(&state.paths).unwrap();
    // 不传 name → 从 package 推导（git@example/x.git → x）
    assert!(store.list().iter().any(|s| s.name == "x"));
}

#[tokio::test]
async fn source_add_with_explicit_name_overrides_derivation() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/sources")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("name=my-private&package=git%40example/x.git"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let store = skillkit_core::SourcesStore::load(&state.paths).unwrap();
    assert!(store.list().iter().any(|s| s.name == "my-private"));
}

#[tokio::test]
async fn source_add_rejects_empty_package() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/sources")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("package="))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    // 未写入任何 source
    assert!(skillkit_core::SourcesStore::load(&state.paths)
        .unwrap()
        .list()
        .is_empty());
}

#[tokio::test]
async fn source_preview_derives_name_from_package() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state);
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/test-token/sources/preview?package=git%40github.com%3Aorg%2Fteam-skills.git")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    // 服务端推导出 team-skills 并预填进 name input
    assert!(body.contains(r#"value="team-skills""#));
    // 空 package → 空 value
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/sources/preview?package=")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(common::body_string(resp).await.contains(r#"value=""#));
}

#[tokio::test]
async fn skill_uninstall_removes_from_registry() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let mut reg = skillkit_core::Registry::default();
    reg.skills.insert(
        "demo/x".into(),
        skillkit_core::registry::SkillMeta {
            id: "demo/x".into(),
            name: "x".into(),
            source: "demo".into(),
            scope: skillkit_core::Scope::Global,
            version: None,
            computed_hash: None,
            installed_at: "2026-07-31".into(),
            canonical_path: dir
                .path()
                .join(".agents/skills/x")
                .to_string_lossy()
                .into_owned(),
        },
    );
    reg.save(&state.paths).unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/test-token/skills/demo%2Fx")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let after = skillkit_core::Registry::load(&state.paths).unwrap();
    assert!(after.skills.is_empty());
}

#[tokio::test]
async fn sse_events_endpoint_reachable() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "text/event-stream"
    );
}

#[tokio::test]
async fn home_trailing_slash_reachable() {
    // serve 打印的 URL 是 /{token}/（带尾斜杠），必须可达，否则主人点开即 404。
    let app = skillkit_server::app(common::test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn skill_upgrade_endpoint_returns_500_on_unmanaged() {
    // unmanaged skill（computed_hash=None）无法升级，端点应返回 500 且不 panic。
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let mut reg = skillkit_core::Registry::default();
    reg.skills.insert(
        "unmanaged/foo".into(),
        skillkit_core::registry::SkillMeta {
            id: "unmanaged/foo".into(),
            name: "foo".into(),
            source: "unmanaged".into(),
            scope: skillkit_core::Scope::Global,
            version: None,
            computed_hash: None,
            installed_at: "2026-07-31".into(),
            canonical_path: dir
                .path()
                .join(".agents/skills/foo")
                .to_string_lossy()
                .into_owned(),
        },
    );
    reg.save(&state.paths).unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/skills/unmanaged%2Ffoo/upgrade")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn skill_upgrade_endpoint_returns_500_on_unknown() {
    // 未安装 skill upgrade → SkillNotInstalled → 500（core 报错，handler 不 panic）
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/skills/nope%2Fx/upgrade")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn skills_find_renders_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let _g = common::fake_npx(&state.paths);
    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/skills/find?q=pdf")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("owner/repo@pdf"), "应渲染候选 spec");
    assert!(
        body.contains("https://skills.sh/owner/repo/pdf"),
        "应渲染 url"
    );
}

#[tokio::test]
async fn skills_install_candidate_registers_skill() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // 种 skills.sh 源（package=None），install 需要 source 存在
    skillkit_core::SourcesStore::ensure_default(&state.paths).unwrap();
    let _g = common::fake_npx(&state.paths);
    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/skills/install-candidate")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("spec=owner%2Frepo%40pdf&skill=pdf&scope=local"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let reg = skillkit_core::Registry::load(&state.paths).unwrap();
    let m = reg.get("skills.sh/pdf").expect("应登记 skills.sh/pdf");
    assert_eq!(m.computed_hash.as_deref(), Some("hashnew"));
}

#[tokio::test]
async fn skills_import_registers_existing() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // 造存量 skill：~/.agents/skills/foo/SKILL.md
    let foo = state.paths.agents_skills_dir().join("foo");
    std::fs::create_dir_all(&foo).unwrap();
    std::fs::write(foo.join("SKILL.md"), "---\nname: foo\n---\n# foo\n").unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/skills/import")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let reg = skillkit_core::Registry::load(&state.paths).unwrap();
    let m = reg.get("unmanaged/foo").expect("应登记 unmanaged/foo");
    assert!(m.computed_hash.is_none());
}

#[tokio::test]
async fn skills_upgrade_all_batch_upgrades() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // 两个 managed skill：dc/ok 无人锁 → 正常升级；dc/conflict 被项目 P1 锁 oldhash → 冲突进 blocked
    let mut reg = skillkit_core::Registry::default();
    for (id, name) in [("dc/ok", "ok"), ("dc/conflict", "conflict")] {
        let canon = state.paths.skillkit_skills_dir().join(name);
        std::fs::create_dir_all(&canon).unwrap();
        std::fs::write(canon.join("SKILL.md"), "x").unwrap();
        reg.skills.insert(
            id.into(),
            skillkit_core::registry::SkillMeta {
                id: id.into(),
                name: name.into(),
                source: "dc".into(),
                scope: skillkit_core::Scope::Local,
                version: None,
                computed_hash: Some("oldhash".into()),
                installed_at: "2026-07-31".into(),
                canonical_path: canon.to_string_lossy().into_owned(),
            },
        );
    }
    reg.save(&state.paths).unwrap();

    // P1 锁 dc/conflict=oldhash：upgrade_all(false) 必须把它列进 blocked，而非静默升级
    skillkit_core::Project {
        id: "P1".into(),
        name: "P1".into(),
        path: dir.path().join("p1").to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec![],
        locked_shas: [("dc/conflict".to_string(), "oldhash".to_string())]
            .into_iter()
            .collect(),
    }
    .save(&state.paths)
    .unwrap();

    let _g = common::fake_npx(&state.paths);
    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/skills/upgrade-all")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    let after = skillkit_core::Registry::load(&state.paths).unwrap();
    // dc/ok 无冲突 → 正常升到 hashnew
    assert_eq!(
        after.get("dc/ok").unwrap().computed_hash.as_deref(),
        Some("hashnew"),
        "无冲突的 dc/ok 应升级到 hashnew",
    );
    // dc/conflict 被锁 → 进 blocked 不升级，hash 保持 oldhash（不静默漂移）
    assert_eq!(
        after.get("dc/conflict").unwrap().computed_hash.as_deref(),
        Some("oldhash"),
        "被项目锁定的 dc/conflict 应进 blocked 不升级，hash 不变",
    );
    // summary 反馈冲突 skill + 受影响项目（列出不静默）
    assert!(
        body.contains("dc/conflict") && body.contains("P1"),
        "summary 应列出冲突 skill 与受影响项目：{body}"
    );
}

#[tokio::test]
async fn projects_add_registers_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let proj_root = dir.path().join("myapp");
    std::fs::create_dir_all(&proj_root).unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from(format!(
                    "path={}",
                    urlencode(&proj_root.to_string_lossy())
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ids = skillkit_core::list_project_ids(&state.paths).unwrap();
    assert_eq!(ids.len(), 1, "应注册 1 个项目");
    let proj = skillkit_core::Project::load(&state.paths, &ids[0]).unwrap();
    assert!(proj.path.contains("myapp"));
}

#[tokio::test]
async fn projects_scan_finds_git_dirs() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let root = dir.path().join("scanroot");
    std::fs::create_dir_all(root.join("proj1/.git")).unwrap();
    std::fs::create_dir_all(root.join("proj2/.git")).unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/scan")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from(format!(
                    "dir={}&depth=2",
                    urlencode(&root.to_string_lossy())
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("proj1"), "scan 结果含 proj1");
    assert!(body.contains("proj2"), "scan 结果含 proj2");
}

#[tokio::test]
async fn projects_rebind_updates_path() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let old = dir.path().join("old-name");
    std::fs::create_dir_all(&old).unwrap();
    let new = dir.path().join("new-name");
    std::fs::create_dir_all(&new).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "old-name".into(),
        path: old.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec![],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/ABCDEF12/rebind")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from(format!(
                    "path={}",
                    urlencode(&new.to_string_lossy())
                )))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let after = skillkit_core::Project::load(&state.paths, "ABCDEF12").unwrap();
    assert_eq!(after.id, "ABCDEF12", "rebind 不变 id");
    assert_eq!(after.name, "new-name");
    assert!(after.path.contains("new-name"));
}

#[tokio::test]
async fn project_sync_agents_updates_to_config_default() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let proj_dir = dir.path().join("proj");
    std::fs::create_dir_all(&proj_dir).unwrap();
    // 旧项目只绑了 claude-code（旧默认）
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "proj".into(),
        path: proj_dir.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec![],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/ABCDEF12/sync-agents")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let after = skillkit_core::Project::load(&state.paths, "ABCDEF12").unwrap();
    assert_eq!(
        after.agents,
        vec!["claude-code".to_string(), "cursor".into(), "codex".into()],
        "sync-agents 应把 agents 同步成 Config 默认全 agent"
    );
}

#[tokio::test]
async fn projects_browse_lists_subdirs_skips_hidden_and_files() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    std::fs::create_dir_all(dir.path().join("b")).unwrap();
    std::fs::create_dir_all(dir.path().join(".hidden")).unwrap();
    std::fs::write(dir.path().join("file.txt"), "x").unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/test-token/projects/browse?path={}&into=path&panel=browse-panel-add",
                    dir.path().display()
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("a/"), "应含子目录 a");
    assert!(body.contains("b/"), "应含子目录 b");
    assert!(!body.contains(".hidden"), "跳过隐藏目录");
    assert!(!body.contains("file.txt"), "跳过文件");
    assert!(body.contains("进入"), "每条有进入按钮");
    assert!(body.contains("选定"), "每条有选定按钮");
    assert!(body.contains("上级"), "有上级按钮");
}

#[tokio::test]
async fn projects_browse_select_returns_oob_to_fill_input() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    std::fs::create_dir_all(dir.path().join("a")).unwrap();
    let base = dir.path().display().to_string();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/test-token/projects/browse?path={base}&select=a&into=path&panel=browse-panel-add"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    // oob 回填：input 带 id=name=path + value=选定路径 + hx-swap-oob
    assert!(body.contains(r#"id="path""#), "oob input id=path");
    assert!(
        body.contains(r#"name="path""#),
        "oob input name=path 保留（提交用）"
    );
    assert!(
        body.contains(&format!("{base}/a")),
        "input value 是选定绝对路径"
    );
    assert!(body.contains(r#"hx-swap-oob="true""#), "oob 标记");
    // oob 清空面板
    assert!(
        body.contains(r#"id="browse-panel-add""#),
        "含 panel oob（清空关闭）"
    );
}

#[tokio::test]
async fn projects_browse_unreadable_path_returns_hint() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/projects/browse?path=/nonexistent-skillkit-xyz-123&into=path&panel=browse-panel-add")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("不可读"), "不可读路径给可读提示，不 panic");
}

#[tokio::test]
async fn projects_main_renders_browse_buttons_and_panels() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains(r#"id="path""#), "注册 input id=path");
    assert!(
        body.contains("/projects/browse?into=path&panel=browse-panel-add"),
        "注册浏览按钮调 browse"
    );
    assert!(body.contains(r#"id="browse-panel-add""#), "注册面板 div");
    assert!(body.contains(r#"id="dir""#), "扫描 input id=dir");
    assert!(
        body.contains("/projects/browse?into=dir&panel=browse-panel-scan"),
        "扫描浏览按钮调 browse"
    );
    assert!(body.contains(r#"id="browse-panel-scan""#), "扫描面板 div");
}

#[tokio::test]
async fn workspace_renders_status_badge_profile_cards_and_local_only() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // registry：1 local + 1 global
    let mut reg = skillkit_core::Registry::default();
    reg.skills.insert(
        "dc/local".into(),
        skillkit_core::registry::SkillMeta {
            id: "dc/local".into(),
            name: "local".into(),
            source: "dc".into(),
            scope: skillkit_core::Scope::Local,
            version: None,
            computed_hash: Some("s1".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: dir
                .path()
                .join("canon/local")
                .to_string_lossy()
                .into_owned(),
        },
    );
    reg.skills.insert(
        "dc/glob".into(),
        skillkit_core::registry::SkillMeta {
            id: "dc/glob".into(),
            name: "glob".into(),
            source: "dc".into(),
            scope: skillkit_core::Scope::Global,
            version: None,
            computed_hash: Some("s2".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: dir.path().join("canon/glob").to_string_lossy().into_owned(),
        },
    );
    reg.save(&state.paths).unwrap();
    // profile fe
    skillkit_core::Profile {
        name: "fe".into(),
        description: String::new(),
        skills: vec!["dc/local".into()],
    }
    .save(&state.paths)
    .unwrap();
    // project：绑了 fe，installed 含 local + global
    let proj_root = dir.path().join("p");
    std::fs::create_dir_all(&proj_root).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "p".into(),
        path: proj_root.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec!["fe".into()],
        installed_skills: vec!["dc/local".into(), "dc/glob".into()],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/projects/ABCDEF12")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("status-panel"), "含 status badge 条");
    assert!(body.contains("missing"), "未落地显 missing badge");
    assert!(
        body.contains("绑定: fe") || body.contains("绑定：fe"),
        "展示绑定 profiles"
    );
    assert!(
        body.contains(r#"name="profiles""#),
        "profile 卡片是 checkbox"
    );
    assert!(body.contains("fe"), "列出 profile fe");
    assert!(body.contains("dc/local"), "local 区块含 local skill");
    assert!(body.contains("local installed skills"), "local 区块标题");
    assert!(
        !body.contains(r#"name="skills""#),
        "不再有手动勾选 skill 的 checkbox"
    );
}

#[tokio::test]
async fn project_set_profiles_binds_lands_and_reports() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    // registry：1 local skill（带 canonical 目录，供落地）
    let canon = dir.path().join(".skillkit/.agents/skills/logseq");
    std::fs::create_dir_all(&canon).unwrap();
    std::fs::write(canon.join("SKILL.md"), "x").unwrap();
    let mut reg = skillkit_core::Registry::default();
    reg.skills.insert(
        "dc/logseq".into(),
        skillkit_core::registry::SkillMeta {
            id: "dc/logseq".into(),
            name: "logseq".into(),
            source: "dc".into(),
            scope: skillkit_core::Scope::Local,
            version: None,
            computed_hash: Some("sha1".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        },
    );
    reg.save(&state.paths).unwrap();
    // profile fe 含 dc/logseq
    skillkit_core::Profile {
        name: "fe".into(),
        description: String::new(),
        skills: vec!["dc/logseq".into()],
    }
    .save(&state.paths)
    .unwrap();
    // project（需 .git/info 供落地写 exclude）
    let proj_root = dir.path().join("proj");
    std::fs::create_dir_all(proj_root.join(".git/info")).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "proj".into(),
        path: proj_root.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec![],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/ABCDEF12/profiles")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("profiles=fe"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let after = skillkit_core::Project::load(&state.paths, "ABCDEF12").unwrap();
    assert_eq!(after.applied_profiles, vec!["fe".to_string()]);
    assert_eq!(
        after.installed_skills,
        vec!["dc/logseq".to_string()],
        "绑定 fe 后重算 installed_skills"
    );
    assert!(
        proj_root.join(".claude/skills/logseq").is_symlink(),
        "set_profiles 应一步落地建 symlink"
    );
    let body = common::body_string(resp).await;
    assert!(body.contains("上次应用"), "响应含落地结果区");
    assert!(body.contains("status-panel"), "响应含 status");
}

#[tokio::test]
async fn project_set_profiles_unknown_profile_returns_hint() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let proj_root = dir.path().join("p");
    std::fs::create_dir_all(&proj_root).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "p".into(),
        path: proj_root.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec![],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/ABCDEF12/profiles")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("profiles=nope"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("不存在"), "未知 profile 给可读提示，不 500");
}

#[tokio::test]
async fn project_remove_deletes_and_returns_list_page() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let proj_root = dir.path().join("p");
    std::fs::create_dir_all(&proj_root).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "p".into(),
        path: proj_root.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec![],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();
    assert!(state.paths.projects_dir().join("ABCDEF12.toml").exists());

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/test-token/projects/ABCDEF12")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        !state.paths.projects_dir().join("ABCDEF12.toml").exists(),
        "toml 已删"
    );
    let body = common::body_string(resp).await;
    assert!(body.contains("Projects"), "返回列表页");
}

#[tokio::test]
async fn projects_list_renders_section_cards_delete_and_local_count() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let mut reg = skillkit_core::Registry::default();
    reg.skills.insert(
        "dc/local".into(),
        skillkit_core::registry::SkillMeta {
            id: "dc/local".into(),
            name: "local".into(),
            source: "dc".into(),
            scope: skillkit_core::Scope::Local,
            version: None,
            computed_hash: Some("s".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: "/canon/local".into(),
        },
    );
    reg.skills.insert(
        "dc/glob".into(),
        skillkit_core::registry::SkillMeta {
            id: "dc/glob".into(),
            name: "glob".into(),
            source: "dc".into(),
            scope: skillkit_core::Scope::Global,
            version: None,
            computed_hash: Some("s".into()),
            installed_at: "2026-08-01".into(),
            canonical_path: "/canon/glob".into(),
        },
    );
    reg.save(&state.paths).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "myapp".into(),
        path: "/tmp/myapp".into(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec!["dc/local".into(), "dc/glob".into()],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/test-token/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = common::body_string(resp).await;
    assert!(body.contains("注册项目"), "注册卡片");
    assert!(body.contains("扫描发现"), "扫描卡片");
    assert!(
        body.contains("hx-delete=\"/test-token/projects/ABCDEF12\""),
        "列表项含删除按钮"
    );
    assert!(body.contains("1 local skills"), "local skill 数过滤");
}
