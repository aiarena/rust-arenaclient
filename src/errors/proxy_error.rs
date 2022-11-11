#[derive(Debug, Clone)]
pub enum ProxyError {
    ShutdownRequest,
    AcceptError,
}