use crossbeam::channel::{self, TryRecvError};
// use log::{error, info, warn};
// use std::env::var;
// use std::fs::File;
// use std::io::prelude::*;
use crate::controller::Controller;
use crate::proxy;
use pyo3::prelude::*;
use std::thread;
use std::thread::JoinHandle;
pub enum ClientType {
    Bot,
    Controller,
}
#[pyclass]
pub struct Server {
    ip_addr: String,
}
#[pymethods]
impl Server {
    #[new]
    fn py_new(ip_addr: &str) -> Self {
        Self::new(ip_addr)
    }
    fn py_run(&self) -> bool {
        self.run().join().is_ok()
    }
}

impl Server {
    pub fn new(ip_addr: &str) -> Self {
        Server {
            ip_addr: String::from(ip_addr),
        }
    }

    pub fn run(&self) -> JoinHandle<()> {
        let (proxy_sender, proxy_receiver) = channel::unbounded();

        let addr = self.ip_addr.clone();
        thread::spawn(move || {
            proxy::run(&addr, proxy_sender);
        });
        let mut controller = Controller::new();
        thread::spawn(move || loop {
            match proxy_receiver.try_recv() {
                Ok((c_type, client)) => match c_type {
                    ClientType::Bot => {
                        controller.add_client(client);
                        controller.send_message("{\"Bot\": \"Connected\"}")
                    }
                    ClientType::Controller => {
                        controller.add_supervisor(client);
                        controller.get_config_from_supervisor();
                    }
                },
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => break,
            }
            controller.update_clients();
            controller.update_games();
            thread::sleep(::std::time::Duration::from_millis(100));
        })
    }
}
