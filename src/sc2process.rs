//! SC2 process manager

use std::io::ErrorKind::ConnectionRefused;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use log::{debug, info, warn};

use portpicker::pick_unused_port;
use tempfile::TempDir;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::WebSocketStream;

use crate::paths;

/// SC2 process
pub struct Process {
    /// The actual SC2 process
    process: Child,
    /// WebSocket port
    ws_port: u16,
}

impl Process {
    /// Launch a new process
    pub fn new() -> Self {
        let ws_port = pick_unused_port().expect("Could not find a free port");
        let tempdir = TempDir::new().expect("Could not create temp dir");

        debug!("Starting a new SC2 process");

        let process = (Command::new(paths::executable())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg("-listen")
            .arg("127.0.0.1")
            .arg("-port")
            .arg(ws_port.to_string())
            .arg("-dataDir")
            .arg(paths::base_dir().to_str().unwrap())
            .arg("-displayMode")
            .arg("0")
            .arg("-tempDir")
            .arg(tempdir.path().to_str().unwrap())
            .current_dir(paths::cwd_dir()))
        .spawn()
        .expect("Could not launch SC2 process");

        Self { process, ws_port }
    }

    /// Connect the process websocket
    pub async fn connect(&self) -> Option<WebSocketStream<TcpStream>> {
        let url = format!("ws://127.0.0.1:{}/sc2api", self.ws_port);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.ws_port);

        debug!("Connecting to the process");

        for _ in 0..60 {
            sleep(Duration::new(1, 0));
            let socket =
                match tokio::time::timeout(Duration::from_secs(120), TcpStream::connect(&addr))
                    .await
                    .ok()?
                {
                    Ok(e) => e,
                    Err(ref e) if e.kind() == ConnectionRefused => {
                        continue;
                    }
                    Err(e) => panic!("E: {:?}", e),
                };

            let config = Some(WebSocketConfig {
                max_send_queue: None,
                max_message_size: Some(128 << 20), // 128MiB
                max_frame_size: Some(32 << 20),    // 32MiB
                // This setting allows to accept client frames which are not masked
                // This is not in compliance with RFC 6455 but might be handy in some
                // rare cases where it is necessary to integrate with existing/legacy
                // clients which are sending unmasked frames
                accept_unmasked_frames: true,
            });
            let (ws_stream, _) = tokio_tungstenite::client_async_with_config(url, socket, config)
                .await
                .expect("Failed to connect");

            return Some(ws_stream);
        }

        warn!("Websocket connection could not be formed");
        None
    }

    /// Wait for the process to exit
    pub fn wait(&mut self) {
        info!("Waiting for the sc2 process to exit");
        self.process.kill().expect("SC2 process was not running");
    }

    /// Kill the process
    pub fn kill(&mut self) {
        info!("Killing the sc2 process");
        self.process.kill().expect("Could not kill SC2 process");
    }
}

impl Default for Process {
    fn default() -> Self {
        Self::new()
    }
}
