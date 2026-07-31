//! SSE：notify 监听 ~/.skillkit/，状态文件变化推 changed 事件，前端 EventSource 收到后刷新当前页。
//! 用途：CLI 在另一进程改了 ~/.skillkit/ 后，浏览器视图自动刷新（apply 同步返回不走 SSE）。
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use futures_util::stream::Stream;
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::convert::Infallible;
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::AppState;

pub async fn events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(16);
    let watch_dir: PathBuf = state.paths.skillkit_dir();
    let (notify_tx, mut notify_rx) = mpsc::channel::<notify::Event>(16);

    // notify 是同步回调，开独立线程跑 watcher 并保活（watcher drop 即停止 watch）。
    std::thread::spawn(move || {
        let mut watcher = match RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(e) = res {
                    let _ = notify_tx.blocking_send(e);
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
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });

    // 异步桥：把 notify 事件按 scope 分类后转成 SSE changed 事件。
    tokio::spawn(async move {
        while let Some(e) = notify_rx.recv().await {
            if matches!(
                e.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            ) {
                if let Some(scope) = classify(&e) {
                    let event = Event::default().event("changed").data(scope);
                    if tx.send(Ok(event)).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(rx))
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
