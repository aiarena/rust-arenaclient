//! Bot player participant
use log::{debug, error, info, trace, warn};
use std::fmt;
use std::io::ErrorKind::{ConnectionAborted, ConnectionReset, TimedOut, WouldBlock};
use std::time::Instant;

use protobuf::Clear;
use protobuf::Message;
use sc2_proto::sc2api::{Request, RequestJoinGame, RequestSaveReplay, Response, Status};
use websocket::result::WebSocketError;
use websocket::Message as WMessage;
use websocket::OwnedMessage;

use super::messaging::{ChannelToGame, ToGameContent};
use crate::config::Config;

use crate::handler::messaging::GameOver;
use crate::proxy::Client;
use crate::sc2::{PlayerResult, Race};
use crate::sc2process::Process;
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::thread;
use std::thread::JoinHandle;

/// Player process, connection and details
pub struct Player {
    /// SC2 process for this player
    pub(crate) process: Process,
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
    /// Tags
    pub tags: HashSet<String>,
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
                tags: Default::default(),
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
            tags: Default::default(),
        }
    }
    pub fn player_name(&self) -> &Option<String> {
        &self.data.name
    }
    /// Send message to the client
    fn client_send(&mut self, msg: WMessage) {
        trace!("Sending message to client");
        self.connection.send_message(&msg).expect("Could not send");
    }

    /// Send a protobuf response to the client
    pub fn client_respond(&mut self, r: &Response) {
        trace!(
            "Response to client: [{}]",
            format!("{:?}", r).chars().take(100).collect::<String>()
        );
        self.client_send(WMessage::binary(
            r.write_to_bytes().expect("Invalid protobuf message"),
        ));
    }
    pub fn client_respond_raw(&mut self, r: &[u8]) {
        trace!(
            "Response to client: [{}]",
            format!("{:?}", r).chars().take(100).collect::<String>()
        );
        self.client_send(WMessage::binary(r));
    }

    /// Receive a message from the client
    /// Returns None if the connection is already closed
    #[must_use]
    fn client_recv(&mut self) -> Option<OwnedMessage> {
        trace!("Waiting for a message from the client");
        match self.connection.recv_message() {
            Ok(msg) => {
                trace!("Message received");
                Some(msg)
            }
            Err(WebSocketError::NoDataAvailable) => {
                warn!(
                    "Client {:?} closed connection unexpectedly (ws disconnect)",
                    self.connection.peer_addr().expect("PeerAddr")
                );
                None
            }
            Err(WebSocketError::IoError(ref e)) if e.kind() == ConnectionReset => {
                warn!(
                    "Client {:?} closed connection unexpectedly (connection reset)",
                    self.connection.peer_addr().expect("PeerAddr")
                );
                None
            }
            Err(WebSocketError::IoError(ref e)) if e.kind() == ConnectionAborted => {
                warn!(
                    "Client {:?} closed connection unexpectedly (connection abort)",
                    self.connection.peer_addr().expect("PeerAddr")
                );
                None
            }
            Err(WebSocketError::IoError(ref e)) if e.kind() == TimedOut => {
                warn!(
                    "Client {:?} stopped responding",
                    self.connection.peer_addr().expect("PeerAddr")
                );
                None
            }
            Err(WebSocketError::IoError(ref e)) if e.kind() == WouldBlock => {
                warn!(
                    "Client {:?} stopped responding",
                    self.connection.peer_addr().expect("PeerAddr")
                );
                None
            }
            Err(err) => panic!("Could not receive: {:?}", err),
        }
    }

    /// Get a protobuf request from the client
    /// Returns None if the connection is already closed
    pub fn client_get_request(&mut self) -> Option<Request> {
        match self.client_recv()? {
            OwnedMessage::Binary(bytes) => {
                let resp = Message::parse_from_bytes(&bytes).expect("Invalid protobuf message");
                Some(resp)
            }
            OwnedMessage::Close(_) => None,
            other => panic!("Expected binary message, got {:?}", other),
        }
    }

    pub fn client_get_request_raw(&mut self) -> Option<Vec<u8>> {
        match self.client_recv()? {
            OwnedMessage::Binary(bytes) => Some(bytes),
            OwnedMessage::Close(_) => None,
            other => panic!("Expected binary message, got {:?}", other),
        }
    }

    /// Send message to sc2
    /// Returns None if the connection is already closed
    #[must_use]
    fn sc2_send(&mut self, msg: &OwnedMessage) -> Option<()> {
        self.sc2_ws.send_message(msg).ok()
    }

    /// Send protobuf request to sc2
    /// Returns None if the connection is already closed
    pub fn sc2_request(&mut self, r: Request) -> Option<()> {
        self.sc2_send(&OwnedMessage::Binary(
            r.write_to_bytes().expect("Invalid protobuf message"),
        ))
    }
    pub fn sc2_request_raw(&mut self, r: Vec<u8>) -> Option<()> {
        self.sc2_send(&OwnedMessage::Binary(r))
    }

    /// Wait and receive a protobuf request from sc2
    /// Returns None if the connection is already closed
    pub fn sc2_recv(&mut self) -> Option<Response> {
        match self.sc2_ws.recv_message().ok()? {
            OwnedMessage::Binary(bytes) => {
                Some(Message::parse_from_bytes(&bytes).expect("Invalid data"))
            }
            OwnedMessage::Close(_) => None,
            other => panic!("Expected binary message, got {:?}", other),
        }
    }
    pub fn sc2_recv_raw(&mut self) -> Option<Vec<u8>> {
        match self.sc2_ws.recv_message().ok()? {
            OwnedMessage::Binary(bytes) => Some(bytes),
            OwnedMessage::Close(_) => None,
            other => panic!("Expected binary message, got {:?}", other),
        }
    }

    /// Send a request to SC2 and return the reponse
    /// Returns None if the connection is already closed
    pub fn sc2_query(&mut self, r: Request) -> Option<Response> {
        self.sc2_request(r)?;
        self.sc2_recv()
    }
    pub fn sc2_query_raw(&mut self, r: Vec<u8>) -> Option<Vec<u8>> {
        self.sc2_request_raw(r)?;
        self.sc2_recv_raw()
    }
    /// Saves replay to path
    pub fn save_replay(&mut self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        let mut r = Request::new();
        r.set_save_replay(RequestSaveReplay::new());
        if let Some(response) = self.sc2_query(r) {
            if response.has_save_replay() {
                match File::create(&path) {
                    Ok(mut buffer) => {
                        let data: &[u8] = response.get_save_replay().get_data();
                        buffer
                            .write_all(data)
                            .expect("Could not write to replay file");
                        info!("Replay saved to {:?}", &path);
                        true
                    }
                    Err(e) => {
                        error!("Failed to create replay file {:?}: {:?}", &path, e);
                        false
                    }
                }
            } else {
                error!("No replay data available");
                false
            }
        } else {
            error!("Could not save replay");
            false
        }
    }
    /// Run handler communication loop
    #[must_use]
    pub fn run(mut self, config: Config, mut gamec: ChannelToGame) -> Option<Self> {
        let mut debug_response = Response::new();
        debug_response.set_id(0);
        debug_response.set_status(Status::in_game);
        let mut response = Response::new();
        let mut req = Request::new();

        let replay_path = config.replay_path();
        let mut start_timer = false;
        let mut frame_time = 0_f32;
        let mut start_time: Instant = Instant::now();
        let mut surrender = false;
        let mut response_raw: Vec<u8>;
        // let mut crash = false;

        // if let Some(p) = gamec.recv() {
        //     match p {
        //         ToPlayer::Quit => {
        //             crash = true;
        //             break;
        //         }
        //     }
        // }

        // Get request
        while let Some(req_raw) = self.client_get_request_raw() {
            req.merge_from_bytes(&req_raw).ok()?;

            if start_timer {
                frame_time += start_time.elapsed().as_secs_f32();
            }
            // Check for debug requests
            if config.disable_debug() && req.has_debug() {
                debug_response.set_id(req.get_id());
                self.client_respond(&debug_response);
                continue;
            } else if req.has_leave_game() {
                surrender = true;
                break;
            }
            for tag in req
                .get_action()
                .actions
                .iter()
                .filter(|a| a.has_action_chat() && a.get_action_chat().has_message())
                .filter_map(|x| {
                    let msg = x.get_action_chat().get_message();
                    if msg.contains("Tag:") {
                        msg.strip_prefix("Tag:").map(String::from)
                    } else {
                        None
                    }
                })
            {
                self.tags.insert(tag);
            }

            // Send request to SC2 and get response
            response_raw = match self.sc2_query_raw(req_raw) {
                Some(d) => d,
                None => {
                    error!("SC2 unexpectedly closed the connection");
                    gamec.send(ToGameContent::SC2UnexpectedConnectionClose);
                    debug!("Killing the process");
                    self.process.kill();
                    return Some(self);
                }
            };

            response.merge_from_bytes(&response_raw).ok()?;
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
            self.client_respond_raw(&response_raw);
            start_timer = true;
            start_time = Instant::now();

            if response.has_quit() {
                self.save_replay(replay_path);
                self.frame_time = frame_time / self.game_loops as f32;
                self.frame_time = if self.frame_time.is_nan() {
                    0_f32
                } else {
                    self.frame_time
                };
                debug!("SC2 is shutting down");
                gamec.send(ToGameContent::QuitBeforeLeave);
                debug!("Waiting for the process");
                self.process.wait();
                return Some(self);
            } else if response.has_observation() {
                self.frame_time = frame_time / self.game_loops as f32;
                self.frame_time = if self.frame_time.is_nan() {
                    0_f32
                } else {
                    self.frame_time
                };
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
                    gamec.send(ToGameContent::GameOver(GameOver {
                        results,
                        game_loops: self.game_loops,
                        frame_time: self.frame_time,
                        tags: self.tags.iter().cloned().collect(),
                    }));
                    self.save_replay(replay_path);
                    self.process.kill();
                    return Some(self);
                }
                if self.game_loops > config.max_game_time() {
                    self.save_replay(replay_path);
                    self.frame_time = frame_time / self.game_loops as f32;
                    self.frame_time = if self.frame_time.is_nan() {
                        0_f32
                    } else {
                        self.frame_time
                    };
                    debug!("Max time reached");
                    gamec.send(ToGameContent::GameOver(GameOver {
                        results: vec![PlayerResult::Tie, PlayerResult::Tie],
                        game_loops: self.game_loops,
                        frame_time: self.frame_time,
                        tags: self.tags.iter().cloned().collect(),
                    }));
                    self.process.kill();
                    return Some(self);
                }
            } else if surrender {
                self.save_replay(replay_path);
            }

            clear_request(&mut req);
            clear_response(&mut response);
            response_raw.clear();

            // if let Some(msg) = gamec.recv() {
            //     return match msg {
            //         ToPlayer::Quit => {
            //             self.save_replay(replay_path);
            //             self.frame_time = frame_time / self.game_loops as f32;
            //             self.frame_time = if self.frame_time.is_nan() {
            //                 0_f32
            //             } else {
            //                 self.frame_time
            //             };
            //             debug!("Killing the process by request from the handler");
            //             self.process.kill();
            //             Some(self)
            //         }
            //     };
            // }
        }

        // Connection already closed
        // Populate result if bot has left the game, otherwise it will show as
        // a crash
        // if crash {
        //     let mut results: Vec<PlayerResult> = vec![PlayerResult::Crash; 2];
        //     results[(self.player_id.unwrap() - 1) as usize] = PlayerResult::Victory;
        //     gamec.send(ToGameContent::GameOver((
        //         results,
        //         self.game_loops,
        //         self.frame_time,
        //     )));
        //     self.process.kill();
        //     return Some(self);
        // }
        if surrender {
            let mut results: Vec<PlayerResult> = vec![PlayerResult::Victory; 2];
            results[(self.player_id.unwrap() - 1) as usize] = PlayerResult::Defeat;
            gamec.send(ToGameContent::GameOver(GameOver {
                results,
                game_loops: self.game_loops,
                frame_time: self.frame_time,
                tags: self.tags.iter().cloned().collect(),
            }));
            self.process.kill();
            return Some(self);
        }
        gamec.send(ToGameContent::UnexpectedConnectionClose);
        info!("Killing process after unexpected connection close (Crash)");
        self.process.kill();
        Some(self)
    }

    /// Terminate the process, and return the client
    pub fn extract_client(mut self) -> Client {
        assert_eq!(self.sc2_status, Some(Status::launched));
        self.process.kill();
        self.connection
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
#[derive(Debug, Copy, Clone)]
pub enum Visibility {
    Hidden,
    Fogged,
    Visible,
    FullHidden,
}
impl Default for Visibility {
    fn default() -> Self {
        Visibility::Hidden
    }
}

pub fn clear_request(req: &mut Request) {
    req.request = None;
    req.id = None;
    req.unknown_fields.clear();
}

pub fn clear_response(response: &mut Response) {
    response.response = None;
    response.id = None;
    response.error.clear();
    response.status = None;
    response.unknown_fields.clear();
}
