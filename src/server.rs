use crossbeam::channel::{self, TryRecvError};
// use log::{error, info, warn};
// use std::env::var;
// use std::fs::File;
// use std::io::prelude::*;
use crate::controller::{create_supervisor_listener, Controller, SupervisorAction};
use crate::proxy;
use bincode::{deserialize, serialize};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};
use pyo3::ToPyObject;
use serde::{Deserialize, Serialize};
use std::thread;
use std::thread::JoinHandle;
use pretty_env_logger;
pub enum ClientType {
    Bot,
    Controller,
}

#[derive(Serialize, Deserialize)]
pub struct RustServer {
    ip_addr: String,
}

impl RustServer {
    pub fn new(ip_addr: &str) -> Self {
        RustServer {
            ip_addr: String::from(ip_addr),
        }
    }

    pub fn run(&self) -> JoinHandle<()> {
        let (proxy_sender, proxy_receiver) = channel::unbounded();
        let (sup_send, sup_recv) = channel::unbounded();
        let addr = self.ip_addr.clone();
        thread::spawn(move || {
            proxy::run(&addr, proxy_sender);
        });
        let mut controller = Controller::new();
        thread::spawn(move || loop {
            match proxy_receiver.try_recv() {
                Ok((c_type, client)) => match c_type {
                    ClientType::Bot => {
                        if !controller.has_supervisor() {
                            println!("No supervisor - Client shutdown");
                            client.shutdown().expect("Could not close connection");
                        } else {
                            controller.add_client(client);
                            controller.send_message("{\"Bot\": \"Connected\"}")
                        }
                    }
                    ClientType::Controller => {
                        let client_split = client.split().unwrap();
                        controller.add_supervisor(client_split.1, sup_recv.to_owned());
                        // controller.get_config_from_supervisor();
                        create_supervisor_listener(client_split.0, sup_send.to_owned());
                        controller.send_message("{\"Status\": \"Connected\"}");
                    }
                },
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => break,
            }
            if let Some(action) = controller.recv_msg() {
                match action {
                    SupervisorAction::Quit => {
                        println!("Quit request received");
                        controller.close();
                        controller.send_message("Reset");
                        controller.drop_supervisor();
                    }
                    SupervisorAction::Config(config) => {
                        controller.set_config(config);
                    }
                    SupervisorAction::ForceQuit =>{
                        break
                    }
                    _ => {}
                }
            }

            controller.update_clients();
            controller.update_games();
            thread::sleep(::std::time::Duration::from_millis(100));
        })
    }
}
#[pyclass(module = "rust_ac")]
pub(crate) struct PServer {
    server: Option<RustServer>,
}

#[pymethods]
impl PServer {
    #[new]
    #[args(args = "*")]
    fn new(args: &PyTuple) -> Self {
        match args.len() {
            0 => Self { server: None },
            1 => {
                if let Ok(f) = args.get_item(0).extract::<&str>() {
                    Self {
                        server: Some(RustServer::new(f)),
                    }
                } else {
                    Self { server: None }
                }
            }
            _ => unreachable!(),
        }
    }
    pub fn run(&self) -> Result<(), PyErr> {
        match &self.server {
            Some(server) => {
                println!("Starting server on {:?}", server.ip_addr);
                match server.run().join() {
                    Ok(_) => Ok(()),
                    Err(_) => Err(pyo3::exceptions::ConnectionError::py_err(
                        "Could not start server. Address in use",
                    )),
                }
            }
            None => Err(pyo3::exceptions::AssertionError::py_err(
                "Server not set. Did you initialize the object?",
            )),
        }
    }

    pub fn __setstate__(&mut self, py: Python, state: PyObject) -> PyResult<()> {
        match state.extract::<&PyBytes>(py) {
            Ok(s) => {
                self.server = deserialize(s.as_bytes()).unwrap();
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    pub fn __getstate__(&self, py: Python) -> PyResult<PyObject> {
        Ok(PyBytes::new(py, &serialize(&self.server).unwrap()).to_object(py))
    }
}
