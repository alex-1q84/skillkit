//! Skills 视图：registry 总览 + install/upgrade/uninstall。
use askama::Template;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::Form;
use serde::Deserialize;
use skillkit_core::{registry::SkillMeta, Candidate, Registry, Scope, SourcesStore};
use std::collections::HashMap;

use crate::routes::{error_response, SkillsQuery};
use crate::AppState;

#[derive(Template)]
#[template(path = "skills.html")]
pub struct SkillsTpl<'a> {
    pub token: &'a str,
    /// (meta, id 的 path 编码)——id 形如 source/skill，/ 须编码为 %2F 才能放进单段路径参数。
    pub skills: Vec<(SkillMeta, String)>,
    pub summary: Option<&'a str>,
    pub selected: Vec<String>,
    pub profile_filter: Vec<String>,
    /// skill_id → 所属 profile name 列表（反向 map，一次遍历现算）。global 不在（不属 profile）。
    pub profiles_of: HashMap<String, Vec<String>>,
    pub all_profile_names: Vec<String>,
    pub selected_csv: String,
    pub scope: String,
}

/// 纯 main 内容片段（SSE 刷新用），不含 nav。
#[derive(Template)]
#[template(path = "fragments/skills_main.html")]
pub struct SkillsMainTpl<'a> {
    pub token: &'a str,
    pub skills: Vec<(SkillMeta, String)>,
    pub summary: Option<&'a str>,
    pub selected: Vec<String>,
    pub profile_filter: Vec<String>,
    pub profiles_of: HashMap<String, Vec<String>>,
    pub all_profile_names: Vec<String>,
    pub selected_csv: String,
    pub scope: String,
}

pub async fn page(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<SkillsQuery>,
) -> Response {
    render_skills_with_scope(
        state,
        token,
        None,
        q.is_fragment(),
        q.selected_list(),
        q.profile_filter(),
        q.scope_filter(),
    )
}

/// 数据准备：建 skill_id→profile 反向 map（一次遍历），按 profile_filter + scope_filter 过滤 skill 列表。
/// profile_filter 空 = 全部（含 global）；非空 = OR 语义（属任一选中 profile 的 local skill，global 不显示）。
/// scope_filter 非空 = 只 global 或只 local。
#[allow(clippy::type_complexity)] // 元组承载三组返回值，调用方解构
fn build_skills_view(
    paths: &skillkit_core::Paths,
    profile_filter: &[String],
    scope_filter: Option<Scope>,
) -> skillkit_core::Result<(
    Vec<(SkillMeta, String)>,
    HashMap<String, Vec<String>>,
    Vec<String>,
)> {
    let reg = Registry::load(paths)?;
    let all_profile_names = skillkit_core::list_profile_names(paths).unwrap_or_default();
    let mut profiles_of: HashMap<String, Vec<String>> = HashMap::new();
    for name in &all_profile_names {
        if let Ok(p) = skillkit_core::Profile::load(paths, name) {
            for id in &p.skills {
                profiles_of
                    .entry(id.clone())
                    .or_default()
                    .push(name.clone());
            }
        }
    }
    let skills: Vec<(SkillMeta, String)> = reg
        .skills
        .values()
        .filter(|m| {
            if let Some(ref s) = scope_filter {
                if m.scope != *s {
                    return false;
                }
            }
            if profile_filter.is_empty() {
                true
            } else {
                profiles_of
                    .get(&m.id)
                    .is_some_and(|ps| ps.iter().any(|p| profile_filter.contains(p)))
            }
        })
        .map(|m| (m.clone(), m.id.replace('/', "%2F")))
        .collect();
    Ok((skills, profiles_of, all_profile_names))
}

fn render_skills(
    state: AppState,
    token: String,
    summary: Option<&str>,
    fragment: bool,
    selected: Vec<String>,
    profile_filter: Vec<String>,
) -> Response {
    render_skills_with_scope(
        state,
        token,
        summary,
        fragment,
        selected,
        profile_filter,
        None,
    )
}

fn render_skills_with_scope(
    state: AppState,
    token: String,
    summary: Option<&str>,
    fragment: bool,
    selected: Vec<String>,
    profile_filter: Vec<String>,
    scope_filter: Option<Scope>,
) -> Response {
    match build_skills_view(&state.paths, &profile_filter, scope_filter) {
        Ok((skills, profiles_of, all_profile_names)) => {
            let selected_csv = selected.join(",");
            let scope_str = scope_filter
                .as_ref()
                .map(Scope::to_string)
                .unwrap_or_default();
            let rendered = if fragment {
                SkillsMainTpl {
                    token: &token,
                    skills,
                    summary,
                    selected,
                    profile_filter,
                    profiles_of,
                    all_profile_names,
                    selected_csv,
                    scope: scope_str,
                }
                .render()
            } else {
                SkillsTpl {
                    token: &token,
                    skills,
                    summary,
                    selected,
                    profile_filter,
                    profiles_of,
                    all_profile_names,
                    selected_csv,
                    scope: scope_str,
                }
                .render()
            };
            render_str(rendered)
        }
        Err(e) => {
            tracing::error!(error = ?e, "加载 skills 视图失败");
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
            error_response("搜索失败，检查网络/Node 后重试")
        }
        Err(e) => {
            tracing::error!(error = ?e, "find join 失败：{}", q.q);
            error_response("搜索失败，检查网络/Node 后重试")
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
        Ok(_) => render_skills(state, token, None, false, vec![], vec![]),
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
            vec![],
            vec![],
        ),
        Err(skillkit_core::SkillkitError::SkillAlreadyInstalled { .. }) => {
            error_response("该 skill 已安装，可在列表中 upgrade 或 remove")
        }
        Err(e) => {
            tracing::error!(error = ?e, "install-candidate 失败：{}", f.spec);
            error_response("安装失败，检查网络/Node 后重试")
        }
    }
}

pub async fn uninstall(
    State(state): State<AppState>,
    Path((token, id)): Path<(String, String)>,
) -> Response {
    match skillkit_core::uninstall(&state.paths, &id) {
        Ok(()) => render_skills(state, token, None, false, vec![], vec![]),
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
        Ok(_) => render_skills(state, token, None, false, vec![], vec![]),
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
            render_skills(state, token, Some(&summary), false, vec![], vec![])
        }
        Err(e) => {
            tracing::error!(error = ?e, "import 失败");
            error_response("导入失败")
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
            render_skills(state, token, Some(&summary), false, vec![], vec![])
        }
        Err(e) => {
            tracing::error!(error = ?e, "upgrade-all 失败");
            error_response("批量升级失败")
        }
    }
}

/// 批量归入核心：循环 add_skill，SkillAlreadyInstalled 跳过、其余（如 SkillIsGlobal）抛错不 save（原子）。
fn apply_assign(
    profile: &mut skillkit_core::Profile,
    ids: &[String],
    reg: &skillkit_core::Registry,
) -> skillkit_core::Result<()> {
    for id in ids {
        match profile.add_skill(id, reg) {
            // 成功 / 已装（SkillAlreadyInstalled）静默跳过；其余错误（如 SkillIsGlobal）向上抛
            Ok(()) | Err(skillkit_core::SkillkitError::SkillAlreadyInstalled { .. }) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// 批量移出核心：循环 remove_skill，SkillNotInstalled（不属该 profile）跳过，其余抛错。
fn apply_unassign(
    profile: &mut skillkit_core::Profile,
    ids: &[String],
) -> skillkit_core::Result<()> {
    for id in ids {
        match profile.remove_skill(id) {
            Ok(()) | Err(skillkit_core::SkillkitError::SkillNotInstalled { .. }) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// 批量移出 profile。body: profile=<名>&id=<...>。从 profile 移除 ids（不属的跳过）。返回完整 Skills 页。
pub async fn unassign(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<SkillsQuery>,
    body: Bytes,
) -> Response {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(&body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let name = pairs
        .iter()
        .find(|(k, _)| k == "profile")
        .map(|(_, v)| v.clone());
    let ids: Vec<String> = pairs
        .iter()
        .filter(|(k, _)| k == "id")
        .map(|(_, v)| v.clone())
        .collect();
    let Some(name) = name else {
        return error_response("缺少 profile");
    };
    if ids.is_empty() {
        return error_response("缺少 id");
    }
    match skillkit_core::Profile::load(&state.paths, &name) {
        Ok(mut p) => {
            if let Err(e) = apply_unassign(&mut p, &ids) {
                return error_response(format!("移出失败：{e}"));
            }
            if p.save(&state.paths).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            render_skills(
                state,
                token,
                None,
                false,
                q.selected_list(),
                q.profile_filter(),
            )
        }
        Err(_) => error_response(format!("profile {name} 不存在")),
    }
}

/// 批量归入已有 profile。body: profile=<名>&id=<...>（id 重复 key）。返回完整 Skills 页（透传 selected/profiles）。
pub async fn assign(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<SkillsQuery>,
    body: Bytes,
) -> Response {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(&body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let name = pairs
        .iter()
        .find(|(k, _)| k == "profile")
        .map(|(_, v)| v.clone());
    let ids: Vec<String> = pairs
        .iter()
        .filter(|(k, _)| k == "id")
        .map(|(_, v)| v.clone())
        .collect();
    let Some(name) = name else {
        return error_response("缺少 profile");
    };
    if ids.is_empty() {
        return error_response("缺少 id");
    }
    let reg = Registry::load(&state.paths).unwrap_or_default();
    match skillkit_core::Profile::load(&state.paths, &name) {
        Ok(mut p) => {
            if let Err(e) = apply_assign(&mut p, &ids, &reg) {
                return error_response(format!("归入失败：{e}"));
            }
            if p.save(&state.paths).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            render_skills(
                state,
                token,
                None,
                false,
                q.selected_list(),
                q.profile_filter(),
            )
        }
        Err(_) => error_response(format!("profile {name} 不存在，改用新建或先创建")),
    }
}

/// 新建 profile 并归入。body: name=<新名>&id=<...>。先校验不存在（防 create 覆盖清空已有 profile）。
pub async fn assign_new(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<SkillsQuery>,
    body: Bytes,
) -> Response {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(&body)
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let Some(name) = pairs
        .iter()
        .find(|(k, _)| k == "name")
        .map(|(_, v)| v.clone())
    else {
        return error_response("缺少 name");
    };
    let ids: Vec<String> = pairs
        .iter()
        .filter(|(k, _)| k == "id")
        .map(|(_, v)| v.clone())
        .collect();
    if skillkit_core::Profile::load(&state.paths, &name).is_ok() {
        return error_response(format!("profile {name} 已存在，改用归入或换名"));
    }
    let reg = Registry::load(&state.paths).unwrap_or_default();
    let mut p = skillkit_core::Profile {
        name,
        description: String::new(),
        skills: vec![],
    };
    if let Err(e) = apply_assign(&mut p, &ids, &reg) {
        return Html(format!(r#"<p class="err">归入失败：{e}</p>"#)).into_response();
    }
    if p.save(&state.paths).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    render_skills(
        state,
        token,
        None,
        false,
        q.selected_list(),
        q.profile_filter(),
    )
}

/// chip ×：从 profile 移除单个 skill 归属。返回完整 Skills 页。
pub async fn delete_profile(
    State(state): State<AppState>,
    Path((token, id, name)): Path<(String, String, String)>,
    Query(q): Query<SkillsQuery>,
) -> Response {
    let id = id.replace("%2F", "/");
    match skillkit_core::Profile::load(&state.paths, &name) {
        Ok(mut p) => {
            if p.remove_skill(&id).is_err() || p.save(&state.paths).is_err() {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
            render_skills(
                state,
                token,
                None,
                false,
                q.selected_list(),
                q.profile_filter(),
            )
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// GUI scope 转移：POST /skills/rescope?to=global|local&id=<enc>。直接执行 + summary 横幅（去 hx-confirm 方向）。
pub async fn rescope(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<RescopeGuiQuery>,
) -> Response {
    let id = q.id.replace("%2F", "/");
    let target = if q.to.as_deref() == Some("global") {
        Scope::Global
    } else {
        Scope::Local
    };
    match skillkit_core::set_scope(&state.paths, &id, target) {
        Ok(report) => {
            let summary = match target {
                Scope::Global => format!(
                    "✓ 已转全局，从 {} 个 profile / {} 个项目移除引用；以下项目需重新 apply：{}",
                    report.affected_profiles.len(),
                    report.affected_projects.len(),
                    report.affected_projects.join(", ")
                ),
                Scope::Local => "✓ 已转 local，撤销全局落地（可 rescope global 恢复）".to_string(),
            };
            render_skills(state, token, Some(&summary), false, vec![], vec![])
        }
        Err(e) => {
            tracing::error!(error = ?e, "GUI rescope 失败：{id}");
            error_response(format!("rescope 失败：{e}"))
        }
    }
}

#[derive(Deserialize)]
pub struct RescopeGuiQuery {
    pub to: Option<String>,
    pub id: String,
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

#[cfg(test)]
mod tests {
    use super::*;
    use skillkit_core::{Paths, Profile, SkillMeta};
    use tempfile::tempdir;

    fn paths() -> Paths {
        Paths::new(tempdir().unwrap().path().to_path_buf())
    }

    fn seed(paths: &Paths, id: &str, scope: Scope) {
        let mut reg = Registry::load(paths).unwrap_or_default();
        reg.upsert(SkillMeta {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(),
            scope,
            version: None,
            computed_hash: Some("abc".into()),
            installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: format!(
                "~/.skillkit/.agents/skills/{}",
                id.rsplit('/').next().unwrap()
            ),
        });
        reg.save(paths).unwrap();
    }

    #[test]
    fn build_view_filters_by_profile_and_maps_reverse() {
        let p = paths();
        seed(&p, "dc/fe", Scope::Local);
        seed(&p, "dc/be", Scope::Local);
        seed(&p, "dc/g", Scope::Global);
        Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec!["dc/fe".into()],
        }
        .save(&p)
        .unwrap();

        // 全部（filter 空）：含 global
        let (all, m, _) = build_skills_view(&p, &[], None).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(
            m.get("dc/fe").cloned().unwrap_or_default(),
            vec!["fe".to_string()]
        );
        assert!(!m.contains_key("dc/g"), "global 不在反向 map");

        // 过滤 fe：只 local 且属 fe 的（global 不显示）
        let (filtered, _, _) = build_skills_view(&p, &["fe".into()], None).unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].0.id, "dc/fe");
    }

    #[test]
    fn apply_assign_skips_dup_but_throws_scope() {
        let p = paths();
        seed(&p, "dc/fe", Scope::Local);
        seed(&p, "dc/g", Scope::Global);
        let reg = Registry::load(&p).unwrap();
        let mut profile = Profile {
            name: "fe".into(),
            description: String::new(),
            skills: vec!["dc/fe".into()],
        };

        // 含已装的 fe（跳过）+ global g（抛错）→ 整批不 save
        let outcome = apply_assign(&mut profile, &["dc/fe".into(), "dc/g".into()], &reg);
        assert!(matches!(
            outcome,
            Err(skillkit_core::SkillkitError::SkillIsGlobal { .. })
        ));
        // 原子：fe 仍在（原本就在），g 没进
        assert_eq!(profile.skills, vec!["dc/fe".to_string()]);
    }

    fn meta(id: &str, scope: Scope) -> SkillMeta {
        SkillMeta {
            id: id.into(),
            name: id.rsplit('/').next().unwrap().into(),
            source: id.split('/').next().unwrap().into(),
            scope,
            version: None,
            computed_hash: Some("abc".into()),
            installed_at: "2026-08-04T00:00:00Z".into(),
            canonical_path: format!(
                "~/.skillkit/.agents/skills/{}",
                id.rsplit('/').next().unwrap()
            ),
        }
    }

    #[test]
    fn skills_main_renders_profile_chips_and_selected_row() {
        let skills = vec![(meta("dc/fe", Scope::Local), "dc%2Ffe".into())];
        let mut profiles_of = std::collections::HashMap::new();
        profiles_of.insert("dc/fe".into(), vec!["fe".into()]);
        let html = SkillsMainTpl {
            token: "tok",
            skills,
            summary: None,
            selected: vec!["dc/fe".into()],
            profile_filter: vec![],
            profiles_of,
            all_profile_names: vec!["fe".into()],
            selected_csv: "dc/fe".into(),
            scope: String::new(),
        }
        .render()
        .unwrap();
        assert!(html.contains("dc/fe"), "id 渲染");
        assert!(html.contains("selected"), "选中行有 selected 标记");
        assert!(html.contains("fe"), "所属 profile chip");
        assert!(html.contains("assign"), "归入端点");
    }
}
