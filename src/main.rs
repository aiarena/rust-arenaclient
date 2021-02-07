mod build_info;
mod config;
mod controller;
mod handler;
mod maps;
mod paths;
mod portconfig;
mod proxy;
mod result;
mod sc2;
mod sc2process;
mod server;

fn main() -> Result<(), String> {
    let s = server::RustServer::new("127.0.0.1:8642");
    s.run().join().expect("Could not join");
    Ok(())
}
