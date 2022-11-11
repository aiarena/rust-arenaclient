mod build_info;
pub mod config;
pub mod controller;
pub mod handler;
pub mod maps;
mod paths;
mod portconfig;
pub mod proxy;
mod result;
pub mod sc2;
mod sc2process;
pub mod server;
use std::io::Write;

#[tokio::main]
async fn main() {
    env_logger::Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "{}:{} {} [{}] - {}",
                record.file().unwrap_or("unknown"),
                record.line().unwrap_or(0),
                chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"),
                record.level(),
                record.args()
            )
        })
        .init();
    let s = server::RustServer::new("127.0.0.1:8642");
    s.run().await.expect("Could not join");
}
