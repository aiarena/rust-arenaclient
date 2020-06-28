#![feature(test)]
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
    use websocket::ClientBuilder;
    use websocket::header::Headers;
    use websocket::Message;
    use crate::config::Config;
    use std::process::{Command, Stdio};
    use std::path::PathBuf;
    use serial_test::serial;

    fn start_bot(cwd: String){
        let bot_file = "run.py";

        let process = (Command::new("python3")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg(bot_file)
            .arg("--GamePort")
            .arg("8642")
            .arg("--LadderServer")
            .arg("127.0.0.1")
            .arg("--StartPort")
            .arg("8642")
            .arg("--OpponentId")
            .arg("123")
            .current_dir(PathBuf::from(&cwd)))
        .spawn()
        .expect("Could not launch SC2 process");
    }
    #[test]
    #[serial]
    fn test_server() {
        let mut sup_headers = Headers::new();
        sup_headers.set_raw("supervisor",vec![b"true".to_vec()]);

        let server = RustServer::new("127.0.0.1:8642");
        let t = server.run();
        let mut supervisor = ClientBuilder::new("ws://127.0.0.1:8642/sc2api")
            .unwrap()
            .custom_headers(&sup_headers)
            .connect_insecure()
            .unwrap();
        let msg = supervisor.recv_message();
        let config = serde_json::to_string(&Config::new()).unwrap();

        if let Ok(_) = supervisor.send_message(&Message::text(config)) {
            supervisor.send_message(&Message::text("Quit"));
            supervisor.shutdown();
        }
        assert!(t.join().is_ok())
        }

    #[test]
    #[serial]
    fn test_tie(){
        let server = RustServer::new("127.0.0.1:8642");
        let t = server.run();
        let mut sup_headers = Headers::new();
        sup_headers.set_raw("supervisor",vec![b"true".to_vec()]);
        let mut supervisor = ClientBuilder::new("ws://127.0.0.1:8642/sc2api")
            .unwrap()
            .custom_headers(&sup_headers)
            .connect_insecure()
            .unwrap();
        let msg = supervisor.recv_message();
        let config = Config{
            pids: vec![],
            average_frame_time: vec![],
            map: "AutomatonLE".to_string(),
            max_game_time: 100,
            max_frame_time: 1000,
            strikes: 10,
            result: vec![],
            player1: "loser_bot".to_string(),
            player2: "loser_bot".to_string(),
            replay_path: "".to_string(),
            match_id: 1,
            replay_name: "".to_string(),
            game_time: 0.0,
            game_time_seconds: 0.0,
            game_time_formatted: "".to_string(),
            disable_debug: true,
            real_time: false,
            visualize: false
        };
        supervisor.send_message(&Message::text(serde_json::to_string(&config).unwrap()));
        start_bot("/aiarena-test-bots-master/loser_bot".parse().unwrap());
        supervisor.recv_message();
        start_bot("/aiarena-test-bots-master/loser_bot".parse().unwrap());
        supervisor.recv_message();
        supervisor.recv_message();

    }
}

