mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

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
async fn source_add_persists() {
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
                .body(Body::from("name=git-src&package=git%40x"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let store = skillkit_core::SourcesStore::load(&state.paths).unwrap();
    assert!(store.list().iter().any(|s| s.name == "git-src"));
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
async fn project_set_skills_replaces_installed() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
    let proj = skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "p".into(),
        path: dir.path().join("p").to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec!["old/x".into()],
        locked_shas: std::collections::BTreeMap::new(),
    };
    proj.save(&state.paths).unwrap();

    let app = skillkit_server::app(state.clone());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/ABCDEF12/skills")
                .header(
                    axum::http::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(Body::from("skills=new%2Fa&skills=new%2Fb"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let after = skillkit_core::Project::load(&state.paths, "ABCDEF12").unwrap();
    assert_eq!(
        after.installed_skills,
        vec!["new/a".to_string(), "new/b".to_string()]
    );
}

#[tokio::test]
async fn project_apply_lands_symlink() {
    let dir = tempfile::tempdir().unwrap();
    let state = skillkit_server::AppState {
        paths: skillkit_core::Paths::new(dir.path().to_path_buf()),
        token: "test-token".into(),
    };
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
            installed_at: "2026-07-31".into(),
            canonical_path: canon.to_string_lossy().into_owned(),
        },
    );
    reg.save(&state.paths).unwrap();
    let proj_root = dir.path().join("proj");
    std::fs::create_dir_all(proj_root.join(".git/info")).unwrap();
    skillkit_core::Project {
        id: "ABCDEF12".into(),
        name: "proj".into(),
        path: proj_root.to_string_lossy().into_owned(),
        agents: vec!["claude-code".into()],
        applied_profiles: vec![],
        installed_skills: vec!["dc/logseq".into()],
        locked_shas: std::collections::BTreeMap::new(),
    }
    .save(&state.paths)
    .unwrap();

    let app = skillkit_server::app(state);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/test-token/projects/ABCDEF12/apply")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        proj_root.join(".claude/skills/logseq").is_symlink(),
        "apply 应建 symlink"
    );
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
