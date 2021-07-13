use crate::controller::{create_supervisor_listener, Controller, SupervisorAction};
use crate::proxy;
#[cfg(not(feature = "no-pyo3"))]
use bincode::{deserialize, serialize};
use crossbeam::channel::{self, TryRecvError};
use log::info;
#[cfg(not(feature = "no-pyo3"))]
use pyo3::prelude::*;
#[cfg(not(feature = "no-pyo3"))]
use pyo3::types::{PyBytes, PyTuple};
#[cfg(not(feature = "no-pyo3"))]
use pyo3::ToPyObject;
#[cfg(not(feature = "no-pyo3"))]
use serde::{Deserialize, Serialize};
use std::thread;
use std::thread::JoinHandle;
pub enum ClientType {
    Bot,
    Controller,
}
#[cfg_attr(not(feature = "no-pyo3"), derive(Serialize, Deserialize))]
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
                            info!("No supervisor - Client shutdown");
                            client.shutdown().expect("Could not close connection");
                        } else {
                            controller.add_client(client);
                            controller.send_message("{\"Bot\": \"Connected\"}")
                        }
                    }
                    ClientType::Controller => {
                        let client_split = client.split().unwrap();
                        controller.add_supervisor(client_split.1, sup_recv.to_owned());
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
                        info!("Quit request received");
                        controller.close();
                        controller.send_message("Reset");
                        controller.drop_supervisor();
                    }
                    SupervisorAction::Config(config) => {
                        controller.set_config(config);
                        controller.send_message("{\"Config\": \"Received\"}");
                    }
                    SupervisorAction::ForceQuit => break,
                    SupervisorAction::Ping => {
                        controller.send_pong();
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
#[cfg(not(feature = "no-pyo3"))]
#[pyclass(module = "rust_ac")]
#[pyo3(text_signature = "(ip_addr)")]
pub(crate) struct PServer {
    server: Option<RustServer>,
}

#[cfg(not(feature = "no-pyo3"))]
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
    pub fn run(&self, py: Python) -> Result<(), PyErr> {
        match &self.server {
            Some(server) => py.allow_threads(move || {
                info!("Starting server on {:?}", server.ip_addr);
                match server.run().join() {
                    Ok(_) => Ok(()),
                    Err(_) => Err(pyo3::exceptions::PyConnectionError::new_err(
                        "Could not start server. Address in use {:?}",
                    )),
                }
            }),
            None => Err(pyo3::exceptions::PyAssertionError::new_err(
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
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "no-pyo3"))]
    use pyo3::py_run;
    #[cfg(not(feature = "no-pyo3"))]
    use pyo3::types::PyDict;

    #[cfg(not(feature = "no-pyo3"))]
    fn add_module(py: Python, module: &PyModule) -> PyResult<()> {
        py.import("sys")?
            .dict()
            .get_item("modules")
            .unwrap()
            .downcast::<PyDict>()?
            .set_item(module.name()?, module)
    }

    #[cfg(not(feature = "no-pyo3"))]
    #[test]
    fn test_pickle() {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let module = PyModule::new(py, "rust_ac").unwrap();
        let addr_tuple = PyTuple::new(py, ["127.0.0.1:8642"].iter());
        module.add_class::<PServer>().unwrap();
        add_module(py, module).unwrap();
        let inst = PyCell::new(py, PServer::new(&addr_tuple)).unwrap();
        py_run!(
            py,
            inst,
            r#"
            import pickle
            inst2 = pickle.loads(pickle.dumps(inst))
        "#
        );
    }
    // #[test]
    // fn test_server() {
    //     let addr = format!("127.0.0.1:{}", portpicker::pick_unused_port().unwrap());
    //     let ws_addr = format!("ws://{}", addr.clone());
    //     let server = RustServer::new(addr.as_str());
    //     let _t = server.run();
    //     let c = ClientBuilder::new(ws_addr.as_str())
    //         .unwrap()
    //         .connect_insecure();
    //     assert!(c.is_ok());
    // }
}
