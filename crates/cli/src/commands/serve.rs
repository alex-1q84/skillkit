//! serve 子命令：启动本地 web GUI。
use clap::Args;

#[derive(Args)]
pub struct ServeCmd {
    /// 监听端口
    #[arg(long, default_value_t = 7317)]
    port: u16,
}

pub fn run(cmd: ServeCmd) -> anyhow::Result<()> {
    skillkit_server::run(cmd.port)
}
