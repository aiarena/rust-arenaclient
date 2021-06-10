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

fn main() {
    let s = server::RustServer::new("127.0.0.1:8642");
    s.run().join().expect("Could not join");
}
