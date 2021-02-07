//! Bot player participant
use log::{debug, error, info, trace};
use std::fmt;
use std::io::ErrorKind::{ConnectionAborted, ConnectionReset, TimedOut, WouldBlock};
use std::time::Instant;

use websocket::result::WebSocketError;
use websocket::OwnedMessage;

use protobuf::Clear;
use protobuf::Message;
use sc2_proto::sc2api::{Request, RequestJoinGame, RequestSaveReplay, Response, Status};

use super::messaging::{ChannelToGame, ToGameContent, ToPlayer};
use crate::config::Config;
use crate::proxy::Client;
use crate::result::SC2Result;
use crate::sc2::{PlayerResult, Race};
use crate::sc2process::Process;
use std::fs::File;
use std::io::Write;
use std::thread;
use std::thread::JoinHandle;

/// Player process, connection and details
pub struct Player {
    /// SC2 process for this player
    pub process: Process,
    /// SC2 websocket connection
    sc2_ws: Client,
    /// Proxy connection to connected client
    connection: Client,
    /// Status of the connected sc2 process
    sc2_status: Option<Status>,
    /// Additonal data
    pub data: PlayerData,
    /// Game loops
    pub game_loops: u32,
    /// Frame time
    pub frame_time: f32,
    /// Player id
    pub player_id: Option<u32>,
}

impl Player {
    /// Creates new player instance and initializes sc2 process for it
    pub fn new(connection: Client, data: PlayerData) -> JoinHandle<Self> {
        thread::spawn(|| {
            let process = Process::new();
            let sc2_ws = process.connect().expect("Could not connect");
            Self {
                process,
                sc2_ws,
                connection,
                sc2_status: None,
                game_loops: 0,
                data,
                frame_time: 0_f32,
                player_id: None,
            }
        })
    }
    pub fn new_no_thread(connection: Client, data: PlayerData) -> Self {
        let process = Process::new();
        let sc2_ws = process.connect().expect("Could not connect");
        Self {
            process,
            sc2_ws,
            connection,
            sc2_status: None,
            game_loops: 0,
            data,
            frame_time: 0_f32,
            player_id: None,
        }
    }
    pub fn player_name(&self) -> Option<&String> {
        self.data.name.as_ref()
    }
    /// Send message to the client
    fn client_send(&mut self, msg: &OwnedMessage) {
        trace!("Sending message to client");
        self.connection.send_message(msg).expect("Could not send");
    }

    /// Send a protobuf response to the client
    pub fn client_respond(&mut self, r: &Response) {
        trace!(
            "Response to client: [{}]",
            format!("{:?}", r).chars().take(100).collect::<String>()
        );
        self.client_send(&OwnedMessage::Binary(
            r.write_to_bytes().expect("Invalid protobuf message"),
        ));
    }
    pub fn client_respond_raw(&mut self, r: Vec<u8>) {
        trace!(
            "Response to client: [{}]",
            format!("{:?}", r).chars().take(100).collect::<String>()
        );
        self.client_send(&OwnedMessage::Binary(r));
    }
    pub fn error(&self, msg: String) {
        error!("Bot - {}: {}", self.player_name().unwrap(), msg);
    }
    pub fn info(&self, msg: String) {
        info!("Bot - {}: {}", self.player_name().unwrap(), msg);
    }
    pub fn debug(&self, msg: String) {
        debug!("Bot - {}: {}", self.player_name().unwrap(), msg);
    }
    pub fn trace(&self, msg: String) {
        trace!("Bot - {}: {}", self.player_name().unwrap(), msg);
    }
    /// Receive a message from the client
    /// Returns None if the connection is already closed
    #[must_use]
    fn client_recv(&mut self) -> Option<OwnedMessage> {
        self.trace("Waiting for a message from the client".to_string());
        match self.connection.recv_message() {
            Ok(msg) => {
                self.trace("Message received".to_string());
                Some(msg)
            }
            Err(WebSocketError::NoDataAvailable) => {
                self.error(format!(
                    "Client {:?} closed connection unexpectedly (ws disconnect)",
                    self.connection.peer_addr().expect("PeerAddr")
                ));
                None
            }
            Err(WebSocketError::IoError(ref e)) if e.kind() == ConnectionReset => {
                self.error(format!(
                    "Client {:?} closed connection unexpectedly (connection reset)",
                    self.connection.peer_addr().expect("PeerAddr")
                ));
                None
            }
            Err(WebSocketError::IoError(ref e)) if e.kind() == ConnectionAborted => {
                self.error(format!(
                    "Client {:?} closed connection unexpectedly (connection abort)",
                    self.connection.peer_addr().expect("PeerAddr")
                ));
                None
            }
            Err(WebSocketError::IoError(ref e)) if e.kind() == TimedOut => {
                self.error(format!(
                    "Client {:?} stopped responding",
                    self.connection.peer_addr().expect("PeerAddr")
                ));
                None
            }
            Err(WebSocketError::IoError(ref e)) if e.kind() == WouldBlock => {
                self.error(format!(
                    "Client {:?} stopped responding",
                    self.connection.peer_addr().expect("PeerAddr")
                ));
                None
            }
            Err(err) => panic!(
                "{}: Could not receive {:?}",
                self.player_name().unwrap(),
                err
            ),
        }
    }

    pub fn client_get_request_raw(&mut self) -> Option<Vec<u8>> {
        match self.client_recv()? {
            OwnedMessage::Binary(bytes) => Some(bytes),
            OwnedMessage::Close(_) => None,
            other => panic!(
                "{:?}: Expected binary message, got {:?}",
                self.player_name().unwrap(),
                other
            ),
        }
    }

    /// Send message to sc2
    /// Returns None if the connection is already closed
    fn sc2_send(&mut self, msg: &OwnedMessage) -> SC2Result<()> {
        self.sc2_ws.send_message(msg).map_err(|e| e.to_string())
    }

    /// Send protobuf request to sc2
    /// Returns None if the connection is already closed
    pub fn sc2_request(&mut self, r: Request) -> SC2Result<()> {
        self.sc2_send(&OwnedMessage::Binary(
            r.write_to_bytes().expect("Invalid protobuf message"),
        ))
    }
    pub fn sc2_request_raw(&mut self, r: Vec<u8>) -> SC2Result<()> {
        self.sc2_send(&OwnedMessage::Binary(r))
    }

    /// Wait and receive a protobuf request from sc2
    /// Returns None if the connection is already closed
    pub fn sc2_recv(&mut self, requester: &str) -> SC2Result<Response> {
        match self.sc2_ws.recv_message() {
            Ok(OwnedMessage::Binary(bytes)) => {
                let resp: Response = Message::parse_from_bytes(&bytes).expect("Invalid data");
                if !resp.get_error().is_empty() {
                    self.error(format!("{} - {:?}", requester, resp.get_error()));
                }
                Ok(resp)
            }
            Ok(OwnedMessage::Close(close_data)) => {
                match close_data {
                    Some(x) => {
                        self.error(format!(
                            "SC2 connection closed:\nReason {}\nStatus {}",
                            x.reason, x.status_code
                        ));
                    }
                    None => {
                        self.error("SC2 connection closed. No reason given".to_string());
                    }
                }

                Err("SC2 connection closed".to_string())
            }
            Ok(other) => panic!(
                "{}: Expected binary message, got {:?}",
                self.player_name().unwrap(),
                other
            ),
            Err(err) => Err(err.to_string()),
        }
    }
    pub fn sc2_recv_raw(&mut self) -> SC2Result<Vec<u8>> {
        match self.sc2_ws.recv_message() {
            Ok(OwnedMessage::Binary(bytes)) => Ok(bytes),
            Ok(OwnedMessage::Close(close_data)) => {
                match close_data {
                    Some(x) => {
                        self.error(format!(
                            "SC2 connection closed:\nReason {}\nStatus {}",
                            x.reason, x.status_code
                        ));
                    }
                    None => {
                        self.error("SC2 connection closed. No reason given".to_string());
                    }
                }

                Err("SC2 connection closed".to_string())
            }
            Ok(other) => panic!(
                "{}: Expected binary message, got {:?}",
                self.player_name().unwrap(),
                other
            ),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Send a request to SC2 and return the reponse
    /// Returns None if the connection is already closed
    pub fn sc2_query(&mut self, r: Request, requester: &str) -> SC2Result<Response> {
        self.sc2_request(r)?;
        self.sc2_recv(requester)
    }
    pub fn sc2_query_raw(&mut self, r: Vec<u8>) -> SC2Result<Vec<u8>> {
        self.sc2_request_raw(r)?;
        self.sc2_recv_raw()
    }
    /// Saves replay to path
    pub fn save_replay(&mut self, path: String) -> bool {
        if path.is_empty() {
            return false;
        }
        let mut r = Request::new();
        r.set_save_replay(RequestSaveReplay::new());
        if let Ok(response) = self.sc2_query(r, "Supervisor") {
            if response.has_save_replay() {
                match File::create(&path) {
                    Ok(mut buffer) => {
                        let data: &[u8] = response.get_save_replay().get_data();
                        buffer
                            .write_all(data)
                            .expect("Could not write to replay file");
                        self.info(format!("Replay saved to {:?}", &path));
                        true
                    }
                    Err(e) => {
                        self.error(format!("Failed to create replay file {:?}: {:?}", &path, e));
                        false
                    }
                }
            } else {
                self.error("No replay data available".to_string());
                false
            }
        } else {
            self.error("Could not save replay".to_string());
            false
        }
    }

    fn set_frame_time(&mut self, frame_time: f32) {
        self.frame_time = frame_time / self.game_loops as f32;
        self.frame_time = if self.frame_time.is_nan() {
            0_f32
        } else {
            self.frame_time
        };
    }
    /// Run handler communication loop
    #[must_use]
    pub fn run(mut self, config: Config, mut gamec: ChannelToGame) -> Option<Self> {
        // Instantiate variables
        let mut debug_response = Response {
            id: Some(0),
            status: Some(Status::in_game),
            ..Default::default()
        };
        let mut response = Response::new();
        let mut request = Request::new();

        let replay_path = config.replay_path();
        let mut start_timer = false;
        let mut frame_time = 0_f32;
        let mut start_time: Instant = Instant::now();
        let mut surrender = false;
        // Get request
        while let Some(req_raw) = self.client_get_request_raw() {
            request.clear();
            if let Err(e) = request.merge_from_bytes(&req_raw) {
                self.error(e.to_string());
                panic!("Could not create request object from client request")
            }

            if start_timer {
                frame_time += start_time.elapsed().as_secs_f32();
            }
            // Check for debug requests
            if config.disable_debug() && request.has_debug() {
                debug_response.set_id(request.get_id());
                self.client_respond(&debug_response);
                continue;
            } else if request.has_leave_game() {
                surrender = true;
            }

            // Send request to SC2 and get response
            let mut response_raw = match self.sc2_query_raw(req_raw) {
                Ok(bytes) => bytes,
                Err(e) => {
                    self.error(format!("SC2 unexpectedly closed the connection: {}", e));
                    gamec.send(ToGameContent::SC2UnexpectedConnectionClose);
                    self.debug("Killing the process".to_string());
                    self.process.kill();
                    return Some(self);
                }
            };

            response.clear();
            if let Err(e) = response.merge_from_bytes(&response_raw) {
                self.error(format!(
                    "Could not create request object from client request: {}",
                    e.to_string()
                ));
                panic!("Could not create request object from client request")
            }
            self.sc2_status = Some(response.get_status());
            if response.has_game_info() {
                for pi in response.mut_game_info().mut_player_info().iter_mut() {
                    if pi.get_player_id() != self.player_id.unwrap() {
                        pi.race_actual = pi.race_requested;
                    }
                }
                response_raw = response.write_to_bytes().unwrap();
            }

            // Send SC2 response to client
            self.client_respond_raw(response_raw);
            start_timer = true;
            start_time = Instant::now();

            if response.has_quit() {
                self.save_replay(replay_path);
                self.set_frame_time(frame_time);
                self.debug("Quit request received from bot".to_string());
                self.debug("SC2 is shutting down".to_string());
                gamec.send(ToGameContent::QuitBeforeLeave);
                self.debug("Waiting for the process".to_string());
                self.process.wait();
                return Some(self);
            } else if response.has_observation() {
                self.set_frame_time(frame_time);
                let obs = response.get_observation();
                let obs_results = obs.get_player_result();
                self.game_loops = obs.get_observation().get_game_loop();
                if !obs_results.is_empty() {
                    // Game is over and results available
                    let mut results_by_id: Vec<(u32, PlayerResult)> = obs_results
                        .iter()
                        .map(|r| (r.get_player_id(), PlayerResult::from_proto(r.get_result())))
                        .collect();
                    results_by_id.sort();
                    let results: Vec<_> = results_by_id.into_iter().map(|(_, v)| v).collect();
                    gamec.send(ToGameContent::GameOver((
                        results,
                        self.game_loops,
                        self.frame_time,
                    )));
                    self.save_replay(replay_path);
                    self.process.kill();
                    return Some(self);
                }
                if self.game_loops > config.max_game_time() {
                    self.save_replay(replay_path);
                    self.set_frame_time(frame_time);
                    self.debug("Max time reached".to_string());
                    gamec.send(ToGameContent::GameOver((
                        vec![PlayerResult::Tie, PlayerResult::Tie],
                        self.game_loops,
                        self.frame_time,
                    )));
                    self.process.kill();
                    return Some(self);
                }
            } else if surrender {
                self.save_replay(replay_path.clone());
            }

            if let Some(msg) = gamec.recv() {
                return match msg {
                    ToPlayer::Quit => {
                        self.save_replay(replay_path);
                        self.set_frame_time(frame_time);
                        self.debug("Killing the process by request from the handler".to_string());
                        self.process.kill();
                        Some(self)
                    }
                };
            }
        }

        // Connection already closed
        // Populate result if bot has left the game, otherwise it will show as
        // a crash
        if surrender {
            let mut results: Vec<PlayerResult> = vec![PlayerResult::Victory; 2];
            results[(self.player_id.unwrap() - 1) as usize] = PlayerResult::Defeat;
            gamec.send(ToGameContent::GameOver((
                results,
                self.game_loops,
                self.frame_time,
            )));
            self.process.kill();
            return Some(self);
        }
        gamec.send(ToGameContent::UnexpectedConnectionClose);
        self.info("Killing process after unexpected connection close (Crash)".to_string());
        self.process.kill();
        Some(self)
    }
}

impl fmt::Debug for Player {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Player {{ ... }}")
    }
}

/// Player data, like join parameters
#[derive(Debug, Clone)]
pub struct PlayerData {
    pub race: Race,
    pub name: Option<String>,
    pub ifopts: sc2_proto::sc2api::InterfaceOptions,
}
impl PlayerData {
    pub fn from_join_request(req: RequestJoinGame, archon: bool) -> Self {
        Self {
            race: Race::from_proto(req.get_race()),
            name: if req.has_player_name() {
                Some(req.get_player_name().to_owned())
            } else {
                None
            },
            ifopts: {
                let mut ifopts = req.get_options().clone();
                ifopts.set_raw_affects_selection(!archon);
                ifopts
            },
        }
    }
}
