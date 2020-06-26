#![feature(test)]
pub mod client;
use pyo3::prelude::*;

pub mod config;
pub mod controller;
pub mod game;
pub mod maps;
mod paths;
mod portconfig;
pub mod proxy;
mod result;
pub mod sc2;
mod sc2process;
pub mod server;
extern crate test;

#[pymodule]
fn rust_ac(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<server::PServer>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::RustServer;

    #[test]
    fn test_server() {
        let server = RustServer::new("127.0.0.1:8642");
        let t = server.run();
        t.join().unwrap();
        assert_eq!(1, 1);
    }
}
