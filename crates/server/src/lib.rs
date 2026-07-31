//! skillkit-server：Axum web server，调 skillkit-core，供 cli 的 serve 子命令调用。
//! M2 实现：htmx 四视图 + apply 闭环 + SSE 跨进程刷新。

/// 启动 web server（Task 4 实现）。
pub fn run(_port: u16) -> anyhow::Result<()> {
    Ok(())
}
