//! SSE：每个被监听目录（~/.skillkit/）共享一个全局 watcher，状态文件变化广播 changed 事件，
//! 多个 SSE 连接订阅同一 channel。连接断开只 drop 自己的 receiver，不重建 watcher、不累积线程
//! （修复旧实现：每次连接 spawn 一个永不退出的 watcher 线程）。
//! 用途：CLI 在另一进程改了 ~/.skillkit/ 后，浏览器视图自动刷新（apply 同步返回不走 SSE）。
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use crate::AppState;

/// 目录 → 全局变化广播（scope："registry"/"sources"/"profiles"/"projects"）。
/// 生产只有一个 ~/.skillkit，即一个 watcher；测试每个 tempdir 独立，互不串扰。
static WATCHERS: OnceLock<Mutex<HashMap<PathBuf, broadcast::Sender<String>>>> = OnceLock::new();

/// 取（或初始化）某目录的广播 sender。首个连接为该目录起 watcher 线程。
fn changes_for(watch_dir: PathBuf) -> broadcast::Sender<String> {
    let map = WATCHERS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().unwrap();
    guard
        .entry(watch_dir.clone())
        .or_insert_with(|| {
            let (tx, _) = broadcast::channel::<String>(64);
            spawn_watcher(watch_dir, tx.clone());
            tx
        })
        .clone()
}

/// 起一个 watcher 线程：阻塞监听目录，notify 回调里分类后广播 scope。线程常驻进程生命期。
fn spawn_watcher(watch_dir: PathBuf, tx: broadcast::Sender<String>) {
    std::thread::spawn(move || {
        let mut watcher = match RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(e) = res {
                    if matches!(
                        e.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    ) {
                        if let Some(scope) = classify(&e) {
                            let _ = tx.send(scope);
                        }
                    }
                }
            },
            Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!(error = ?e, "notify watcher 初始化失败");
                return;
            }
        };
        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::Recursive) {
            tracing::error!(error = ?e, "watch 失败");
        }
        // 保活：notify 的 watcher 在回调可用期间持续工作；循环防线程被回收。
        // 生产每目录一个常驻线程，量级固定，不再随连接数增长（旧实现每连接一个）。
        loop {
            std::thread::sleep(std::time::Duration::from_mins(1));
        }
    });
}

pub async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let watch_dir: PathBuf = state.paths.skillkit_dir();
    let rx = changes_for(watch_dir).subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|item| async move {
        match item {
            Ok(scope) => Some(Ok(Event::default().event("changed").data(scope))),
            // 落后丢弃（BroadcastStreamRecvError 只有 Lagged 变体；Closed 时流直接结束）
            Err(_) => None,
        }
    });
    Sse::new(stream)
}

/// 按 path 推断变化 scope（registry/sources/profiles/projects/其它忽略）。
fn classify(e: &notify::Event) -> Option<String> {
    for p in &e.paths {
        let s = p.to_string_lossy();
        if s.contains("registry.json") {
            return Some("registry".into());
        }
        if s.contains("sources.toml") {
            return Some("sources".into());
        }
        if s.contains("/profiles/") {
            return Some("profiles".into());
        }
        if s.contains("/projects/") {
            return Some("projects".into());
        }
    }
    None
}
