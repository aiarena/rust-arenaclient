//! Bot player participant

use log::{debug, error, trace, warn};
use std::fmt;
use std::io::ErrorKind::{ConnectionAborted, ConnectionReset, TimedOut};
use std::time::Instant;

use websocket::result::WebSocketError;
use websocket::OwnedMessage;

use protobuf::parse_from_bytes;
use protobuf::{Message, RepeatedField};
use sc2_proto::sc2api::{Request, RequestJoinGame, RequestSaveReplay, Response, Status};

use super::messaging::{ChannelToGame, ToGameContent, ToPlayer};
use crate::config::Config;
use crate::proxy::Client;
use crate::sc2::{PlayerResult, Race};
use crate::sc2process::Process;
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
            }
        })
    }
    pub fn player_name(&self) -> Option<String> {
        self.data.name.clone()
    }
    /// Send message to the client
    fn client_send(&mut self, msg: &OwnedMessage) {
        trace!("Sending message to client");
        self.connection.send_message(msg).expect("Could not send");
    }

    /// Send a protobuf response to the client
    pub fn client_respond(&mut self, r: Response) {
        trace!(
            "Response to client: [{}]",
            format!("{:?}", r).chars().take(100).collect::<String>()
        );
        self.client_send(&OwnedMessage::Binary(
            r.write_to_bytes().expect("Invalid protobuf message"),
        ));
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
            Err(err) => panic!("Could not receive: {:?}", err),
        }
    }

    /// Get a protobuf request from the client
    /// Returns None if the connection is already closed
    #[must_use]
    pub fn client_get_request(&mut self) -> Option<Request> {
        match self.client_recv()? {
            OwnedMessage::Binary(bytes) => {
                let resp = parse_from_bytes::<Request>(&bytes).expect("Invalid protobuf message");
                trace!("Request from the client: {:?}", resp);
                Some(resp)
            }
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
    #[must_use]
    pub fn sc2_request(&mut self, r: Request) -> Option<()> {
        self.sc2_send(&OwnedMessage::Binary(
            r.write_to_bytes().expect("Invalid protobuf message"),
        ))
    }

    /// Wait and receive a protobuf request from sc2
    /// Returns None if the connection is already closed
    #[must_use]
    pub fn sc2_recv(&mut self) -> Option<Response> {
        match self.sc2_ws.recv_message().ok()? {
            OwnedMessage::Binary(bytes) => {
                Some(parse_from_bytes::<Response>(&bytes).expect("Invalid data"))
            }
            OwnedMessage::Close(_) => None,
            other => panic!("Expected binary message, got {:?}", other),
        }
    }

    /// Send a request to SC2 and return the reponse
    /// Returns None if the connection is already closed
    #[must_use]
    pub fn sc2_query(&mut self, r: Request) -> Option<Response> {
        self.sc2_request(r)?;
        self.sc2_recv()
    }
    /// Saves replay to path
    pub fn save_replay(&mut self, path: String) -> bool {
        if path == "" {
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
                        println!("Replay saved to {:?}", &path);
                        true
                    }
                    Err(e) => {
                        println!("Failed to create replay file {:?}: {:?}", &path, e);
                        false
                    }
                }
            } else {
                println!("No replay data available");
                false
            }
        } else {
            println!("Could not save replay");
            false
        }
    }
    /// Run game communication loop
    #[must_use]
    pub fn run(mut self, config: Config, mut gamec: ChannelToGame) -> Option<Self> {
        let replay_path = config.replay_path();
        let mut start_timer = false;
        let mut frame_time = 0_f32;
        let mut start_time: Instant = Instant::now();
        // Get request
        while let Some(req) = self.client_get_request() {
            if start_timer {
                frame_time += start_time.elapsed().as_secs_f32();
            }
            // Check for debug requests
            if config.disable_debug() && req.has_debug() {
                let mut response = Response::new();
                response.set_error(RepeatedField::from_vec(vec![
                    "Proxy: Request denied".to_owned()
                ]));
                self.client_respond(response.clone());
            }

            // Send request to SC2 and get response
            let response = match self.sc2_query(req) {
                Some(d) => d,
                None => {
                    error!("SC2 unexpectedly closed the connection");
                    gamec.send(ToGameContent::SC2UnexpectedConnectionClose);
                    debug!("Killing the process");
                    self.process.kill();
                    return Some(self);
                }
            };
            self.sc2_status = Some(response.get_status());

            // Send SC2 response to client
            self.client_respond(response.clone());
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
            } else if response.has_leave_game() {
                self.save_replay(replay_path);
                self.frame_time = frame_time / self.game_loops as f32;
                self.frame_time = if self.frame_time.is_nan() {
                    0_f32
                } else {
                    self.frame_time
                };
                debug!("Client left the game");
                gamec.send(ToGameContent::LeftGame);
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
                    self.frame_time = frame_time / self.game_loops as f32;
                    self.frame_time = if self.frame_time.is_nan() {
                        0_f32
                    } else {
                        self.frame_time
                    };
                    debug!("Max time reached");
                    gamec.send(ToGameContent::GameOver((
                        vec![PlayerResult::Tie, PlayerResult::Tie],
                        self.game_loops,
                        self.frame_time,
                    )));
                    self.process.kill();
                    return Some(self);
                }
            }

            if let Some(msg) = gamec.recv() {
                return match msg {
                    ToPlayer::Quit => {
                        self.save_replay(replay_path);
                        self.frame_time = frame_time / self.game_loops as f32;
                        self.frame_time = if self.frame_time.is_nan() {
                            0_f32
                        } else {
                            self.frame_time
                        };
                        debug!("Killing the process by request from the game");
                        self.process.kill();
                        Some(self)
                    }
                };
            }
        }

        // Connection already closed
        gamec.send(ToGameContent::UnexpectedConnectionClose);
        debug!("Killing process after unexpected connection close");
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
    pub fn from_join_request(req: RequestJoinGame) -> Self {
        Self {
            race: Race::from_proto(req.get_race()),
            name: if req.has_player_name() {
                Some(req.get_player_name().to_owned())
            } else {
                None
            },
            ifopts: {
                let mut ifopts = req.get_options().clone();
                ifopts.set_raw_affects_selection(true);
                ifopts
            },
        }
    }
}
