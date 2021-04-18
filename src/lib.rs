#[cfg(not(feature = "no-pyo3"))]
use pyo3::prelude::*;

pub mod build_info;
pub mod config;
pub mod controller;
pub mod handler;
pub mod maps;
pub mod paths;
pub mod portconfig;
pub mod proxy;
pub mod result;
pub mod sc2;
pub mod sc2process;
pub mod server;

#[cfg(not(feature = "no-pyo3"))]
#[pymodule]
fn rust_ac(_py: Python, m: &PyModule) -> PyResult<()> {
    pyo3_log::init();
    m.add_class::<server::PServer>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    // use crate::config::Config;
    // use crate::server::RustServer;
    // use std::path::PathBuf;
    // use std::process::{Command, Stdio};
    // use websocket::header::Headers;
    // use websocket::ClientBuilder;
    // use websocket::Message;

    // fn start_bot(cwd: String) {
    //     let bot_file = "run.py";
    //
    //     let process = (Command::new("python3")
    //         .stdout(Stdio::piped())
    //         .stderr(Stdio::piped())
    //         .arg(bot_file)
    //         .arg("--GamePort")
    //         .arg("8642")
    //         .arg("--LadderServer")
    //         .arg("127.0.0.1")
    //         .arg("--StartPort")
    //         .arg("8642")
    //         .arg("--OpponentId")
    //         .arg("123")
    //         .current_dir(PathBuf::from(&cwd)))
    //     .spawn()
    //     .expect("Could not launch Bot");
    // }
    //
    // #[test]
    // fn test_tie() {
    //     let server = RustServer::new("127.0.0.1:8642");
    //     let _t = server.run();
    //     let mut sup_headers = Headers::new();
    //     sup_headers.set_raw("supervisor", vec![b"true".to_vec()]);
    //     let mut supervisor = ClientBuilder::new("ws://127.0.0.1:8642/sc2api")
    //         .unwrap()
    //         .custom_headers(&sup_headers)
    //         .connect_insecure()
    //         .unwrap();
    //     let _msg = supervisor.recv_message().expect("Could not receive");
    //     let config = Config {
    //         pids: vec![],
    //         average_frame_time: vec![],
    //         map: "AutomatonLE".to_string(),
    //         max_game_time: 100,
    //         max_frame_time: 1000,
    //         strikes: 10,
    //         result: vec![],
    //         player1: "loser_bot".to_string(),
    //         player2: "loser_bot".to_string(),
    //         replay_path: "".to_string(),
    //         match_id: 1,
    //         replay_name: "".to_string(),
    //         game_time: 0.0,
    //         game_time_seconds: 0.0,
    //         game_time_formatted: "".to_string(),
    //         disable_debug: true,
    //         real_time: false,
    //         visualize: false,
    //         light_mode: false,
    //     };
    //     supervisor.send_message(&Message::text(serde_json::to_string(&config).unwrap()));
    //     start_bot("/aiarena-test-bots-master/loser_bot".parse().unwrap());
    //     supervisor.recv_message().expect("Could not receive");
    //     start_bot("/aiarena-test-bots-master/loser_bot".parse().unwrap());
    //     supervisor.recv_message().expect("Could not receive");
    //     supervisor.recv_message().expect("Could not receive");
    // }
}
