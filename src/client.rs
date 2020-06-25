use pyo3::exceptions;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use tungstenite::client::AutoStream;
use tungstenite::{connect, Message as Message2, Message, WebSocket};

#[pyclass]
pub struct ClientSession {
    client: WebSocket<AutoStream>,
}

impl ClientSession {
    pub fn new(url: &str) -> Result<Self, PyErr> {
        if let Ok((socket, _)) = connect(url) {
            Ok(Self { client: socket })
        } else {
            Err(exceptions::ConnectionError::py_err("Connection Refused"))
        }
    }
}
#[pymethods]
impl ClientSession {
    #[new]
    pub fn py_new(url: String) -> PyResult<ClientSession> {
        ClientSession::new(url.as_ref())
    }

    pub fn send_bytes(&mut self, data: &[u8]) {
        self.client.write_message(Message2::from(data)).unwrap()
    }

    pub fn receive_bytes(&mut self, py: Python) -> PyResult<Py<PyBytes>> {
        if let Message::Binary(t) = self.client.read_message().unwrap() {
            println!("{:?}", &t);
            Ok(PyBytes::new(py, &t).into())
        } else {
            Err(exceptions::TypeError::py_err("Expected Binary"))
        }
    }
}
