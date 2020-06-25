//! SC2 process manager

use std::io::ErrorKind::ConnectionRefused;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use log::{debug, info, warn};

use portpicker::pick_unused_port;
// use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use websocket::client::sync::Client;
use websocket::stream::sync::TcpStream;
use websocket::ClientBuilder;

use crate::paths;

/// Default verbosity level for SC2 process
// fn default_verbosity() -> bool {
//     true
// }

/// Options for SC2 process
// #[allow(missing_docs)]
// #[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
// pub struct ProcessOptions {
//     #[serde(default)]
//     pub fullscreen: bool,
//     #[serde(default = "default_verbosity")]
//     pub verbose: bool,
// }
// impl ProcessOptions {
//     fn apply(self, mut cmd: &mut Command) -> &mut Command {
//         cmd = cmd
//             .arg("-displayMode")
//             .arg(if self.fullscreen { "1" } else { "0" });
//         if self.verbose {
//             cmd = cmd.arg("-verbose");
//         }
//         cmd
//     }
// }
// impl Default for ProcessOptions {
//     fn default() -> Self {
//         Self {
//             fullscreen: false,
//             verbose: true,
//         }
//     }
// }

/// SC2 process
#[derive(Debug)]
pub struct Process {
    /// The actual SC2 process
    process: Child,
    /// Temp data dir used by SC2
    tempdir: TempDir,
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

        Self {
            process,
            tempdir,
            ws_port,
        }
    }

    /// Connect the process websocket
    pub fn connect(&self) -> Option<Client<std::net::TcpStream>> {
        let url = format!("ws://127.0.0.1:{}/sc2api", self.ws_port);
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.ws_port);

        debug!("Connecting to the process");

        for _ in 0..60 {
            sleep(Duration::new(1, 0));

            let tcp_stream = match TcpStream::connect_timeout(&addr, Duration::new(120, 0)) {
                Ok(s) => s,
                Err(ref e) if e.kind() == ConnectionRefused => {
                    continue;
                }
                Err(e) => panic!("E: {:?}", e),
            };

            match ClientBuilder::new(&url).unwrap().connect_on(tcp_stream) {
                Ok(client) => {
                    debug!("Connection created");
                    return Some(client);
                }
                Err(error) => panic!("Could not connect: {:#?}", error),
            }
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
