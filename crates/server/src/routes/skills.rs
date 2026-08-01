//! Skills 视图：registry 总览 + install/upgrade/uninstall。
use askama::Template;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use serde::Deserialize;
use skillkit_core::{registry::SkillMeta, Candidate, Registry, Scope, SourcesStore};

use crate::routes::FragmentQuery;
use crate::AppState;

#[derive(Template)]
#[template(path = "skills.html")]
pub struct SkillsTpl<'a> {
    pub token: &'a str,
    /// (meta, id 的 path 编码)——id 形如 source/skill，/ 须编码为 %2F 才能放进单段路径参数。
    pub skills: Vec<(SkillMeta, String)>,
    pub summary: Option<&'a str>,
}

/// 纯 main 内容片段（SSE 刷新用），不含 nav。
#[derive(Template)]
#[template(path = "fragments/skills_main.html")]
pub struct SkillsMainTpl<'a> {
    pub token: &'a str,
    pub skills: Vec<(SkillMeta, String)>,
    pub summary: Option<&'a str>,
}

pub async fn page(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<FragmentQuery>,
) -> Response {
    render_skills(state, token, None, q.is_fragment())
}

fn render_skills(
    state: AppState,
    token: String,
    summary: Option<&str>,
    fragment: bool,
) -> Response {
    match Registry::load(&state.paths) {
        Ok(reg) => {
            let skills: Vec<(SkillMeta, String)> = reg
                .skills
                .values()
                .map(|m| (m.clone(), m.id.replace('/', "%2F")))
                .collect();
            let rendered = if fragment {
                SkillsMainTpl {
                    token: &token,
                    skills,
                    summary,
                }
                .render()
            } else {
                SkillsTpl {
                    token: &token,
                    skills,
                    summary,
                }
                .render()
            };
            render_str(rendered)
        }
        Err(e) => {
            tracing::error!(error = ?e, "加载 registry 失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct FindQuery {
    pub q: String,
}

#[derive(Template)]
#[template(path = "fragments/find_results.html")]
pub struct FindResultsTpl<'a> {
    pub token: &'a str,
    pub query: &'a str,
    /// 候选列表，每条带 install 表单。
    pub candidates: Vec<Candidate>,
}

/// find：搜 skills.sh registry，渲染候选片段（每条带 install 按钮）。
pub async fn find(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<FindQuery>,
) -> Response {
    // npx::find 同步阻塞（Command::output），用 spawn_blocking 卸到 blocking 线程池，
    // 避免占用 tokio 工作线程（默认 = CPU 核数）；闭包 move state、clone query。
    let qstr = q.q.clone();
    let result =
        tokio::task::spawn_blocking(move || skillkit_core::npx::find(&state.paths, &qstr)).await;
    match result {
        Ok(Ok(cs)) => {
            let rendered = FindResultsTpl {
                token: &token,
                query: &q.q,
                candidates: cs,
            }
            .render();
            render_str(rendered)
        }
        Ok(Err(e)) => {
            tracing::error!(error = ?e, "find 失败：{}", q.q);
            Html("<p class=\"err\">搜索失败，检查网络/Node 后重试</p>").into_response()
        }
        Err(e) => {
            tracing::error!(error = ?e, "find join 失败：{}", q.q);
            Html("<p class=\"err\">搜索失败，检查网络/Node 后重试</p>").into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct InstallForm {
    scope: Option<String>,
}

pub async fn install(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
    Form(f): Form<InstallForm>,
) -> Response {
    let Some((source, skill)) = id.split_once('/') else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let scope = if matches!(f.scope.as_deref(), Some("global")) {
        Scope::Global
    } else {
        Scope::Local
    };
    // web install 仅支持固定源（有 package）；registry 源（skills.sh）走 CLI find 选候选
    let store = match SourcesStore::load(&state.paths) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = ?e, "加载 sources 失败");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let src = match store.get(source) {
        Ok(s) => s.clone(),
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(package) = src.package else {
        tracing::error!(
            "registry 源 {source} 的 install 请用 CLI（skillkit install add {source} {skill}）走 find 选候选"
        );
        return StatusCode::BAD_REQUEST.into_response();
    };
    match skillkit_core::install(&state.paths, source, skill, &package, scope) {
        Ok(_) => render_skills(state, token, None, false),
        Err(e) => {
            tracing::error!(error = ?e, "install 失败：{id}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct InstallCandidateForm {
    /// owner/repo@skill，npx skills add 的 package 参数。
    pub spec: String,
    /// skill 名（=find 时的 query），作 canonical 目录名 + registry id 后缀。
    pub skill: String,
    pub scope: Option<String>,
}

/// registry 源（skills.sh）install：find 候选选中后装。source 固定 skills.sh，package 用 spec。
pub async fn install_candidate(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Form(f): Form<InstallCandidateForm>,
) -> Response {
    let scope = if matches!(f.scope.as_deref(), Some("global")) {
        Scope::Global
    } else {
        Scope::Local
    };
    match skillkit_core::install(&state.paths, "skills.sh", &f.skill, &f.spec, scope) {
        Ok(_) => render_skills(
            state,
            token,
            Some(&format!("✓ 已安装 skills.sh/{}", f.skill)),
            false,
        ),
        Err(skillkit_core::SkillkitError::SkillAlreadyInstalled { .. }) => {
            Html("<p class=\"err\">该 skill 已安装，可在列表中 upgrade 或 remove</p>")
                .into_response()
        }
        Err(e) => {
            tracing::error!(error = ?e, "install-candidate 失败：{}", f.spec);
            Html("<p class=\"err\">安装失败，检查网络/Node 后重试</p>").into_response()
        }
    }
}

pub async fn uninstall(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
) -> Response {
    match skillkit_core::uninstall(&state.paths, &id) {
        Ok(()) => render_skills(state, token, None, false),
        Err(e) => {
            tracing::error!(error = ?e, "uninstall 失败：{id}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub async fn upgrade(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
) -> Response {
    // GUI 场景已显式点击升级，yes=true 不二次确认
    match skillkit_core::upgrade_skill(&state.paths, &id, true) {
        Ok(_) => render_skills(state, token, None, false),
        Err(e) => {
            tracing::error!(error = ?e, "upgrade 失败：{id}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// 导入存量 skill 目录，登记进 registry（无源 → unmanaged）。
pub async fn import(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    match skillkit_core::import_existing(&state.paths, false) {
        Ok(r) => {
            let summary = format!(
                "imported {}，unmanaged {}，reinstalled {}，skipped {}",
                r.imported.len(),
                r.unmanaged.len(),
                r.reinstalled.len(),
                r.skipped.len()
            );
            render_skills(state, token, Some(&summary), false)
        }
        Err(e) => {
            tracing::error!(error = ?e, "import 失败");
            Html("<p class=\"err\">导入失败</p>").into_response()
        }
    }
}

/// 全部升级：批量升级 registry 全部 managed skill，冲突进 blocked 列出（不升级）。
pub async fn upgrade_all(State(state): State<AppState>, Path(token): Path<String>) -> Response {
    match skillkit_core::upgrade_all(&state.paths, false) {
        Ok(all) => {
            use std::fmt::Write as _;
            let mut summary = format!("已升级 {} 个", all.upgraded.len());
            for b in &all.blocked {
                let _ = write!(
                    summary,
                    "；跳过 {}（影响项目 {}，需重新 apply）",
                    b.id,
                    b.affected.join(", ")
                );
            }
            render_skills(state, token, Some(&summary), false)
        }
        Err(e) => {
            tracing::error!(error = ?e, "upgrade-all 失败");
            Html("<p class=\"err\">批量升级失败</p>").into_response()
        }
    }
}

fn render_str(rendered: askama::Result<String>) -> Response {
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!(error = ?e, "渲染 skills 模板失败");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
