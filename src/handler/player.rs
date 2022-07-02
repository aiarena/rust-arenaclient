//! Bot player participant
use log::{debug, error, info, trace, warn};
use std::fmt;
use std::time::{Duration, Instant};

use protobuf::Message;
use sc2_proto::sc2api::{Request, RequestJoinGame, RequestSaveReplay, Response, Status};
use tokio_tungstenite::tungstenite::Message as TMessage;

use super::messaging::{ChannelToGame, ToGameContent};
use crate::config::Config;

use crate::handler::messaging::GameOver;
use crate::proxy::Client;
use crate::sc2::{PlayerResult, Race};
use crate::sc2process::Process;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashSet;
use std::fs::File;
use std::io::ErrorKind::{ConnectionAborted, ConnectionReset, TimedOut, WouldBlock};
use std::io::Write;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake;
use tokio_tungstenite::tungstenite::Error;
use tokio_tungstenite::WebSocketStream;

/// Player process, connection and details
pub struct Player {
    /// SC2 process for this player
    pub(crate) process: Process,
    /// SC2 websocket connection
    sc2_ws: WebSocketStream<TcpStream>,
    /// Proxy connection to connected client
    client_ws: Client,
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
    response: Response,
    request: Request,
}

impl Player {
    /// Creates new player instance and initializes sc2 process for it
    pub async fn new(connection: Client, data: PlayerData) -> tokio::task::JoinHandle<Player> {
        tokio::task::spawn(async {
            let process = Process::new();
            let sc2_ws = process.connect().await.expect("Could not connect");
            Self {
                process,
                sc2_ws,
                sc2_status: None,
                game_loops: 0,
                data,
                frame_time: 0_f32,
                player_id: None,
                tags: Default::default(),
                response: Default::default(),
                client_ws: connection,
                request: Default::default(),
            }
        })
    }
    pub async fn new_no_thread(connection: Client, data: PlayerData) -> Self {
        let process = Process::new();
        let sc2_ws = process.connect().await.expect("Could not connect");
        Self {
            process,
            sc2_ws,
            client_ws: connection,
            sc2_status: None,
            game_loops: 0,
            data,
            frame_time: 0_f32,
            player_id: None,
            tags: Default::default(),
            response: Default::default(),
            request: Default::default(),
        }
    }
    pub fn player_name(&self) -> &Option<String> {
        &self.data.name
    }
    /// Send message to the client
    async fn client_send(&mut self, msg: TMessage) {
        trace!("{:?}: Sending message to client", self.player_id);
        self.client_ws
            .send_message(msg)
            .await
            .expect("Could not send");
    }

    /// Send a protobuf response to the client
    pub async fn client_respond(&mut self, r: &Response) {
        trace!(
            "{:?}:
            Response to client: [{}]",
            self.player_id,
            format!("{:?}", r).chars().take(100).collect::<String>()
        );
        self.client_send(TMessage::binary(
            r.write_to_bytes().expect("Invalid protobuf message"),
        ))
        .await;
    }
    pub async fn client_respond_raw(&mut self, r: &[u8]) {
        trace!(
            "{:?}: Response to client: [{}]",
            self.player_id,
            format!("{:?}", r).chars().take(100).collect::<String>()
        );
        self.client_send(TMessage::binary(r)).await;
    }

    /// Receive a message from the client
    /// Returns None if the connection is already closed
    async fn client_recv(&mut self) -> anyhow::Result<TMessage> {
        trace!(
            "{:?}: Waiting for a message from the client",
            self.player_id
        );
        if let Some(res_msg) = self.client_ws.recv_message().await {
            match res_msg {
                Ok(msg) => {
                    trace!(
                        "{:?}: Message received from client:\n{:?}",
                        self.player_id,
                        &msg
                    );
                    return Ok(msg);
                }
                Err(Error::Io(e)) if e.kind() == ConnectionReset => {
                    error!(
                    "Client closed connection unexpectedly (connection reset)\nAddress:{:?}\nPlayerId:{:?}\nName:{:?}\nError:{:?}",
                    self.client_ws.peer_addr(), self.player_id, self.data.name,e
                );
                    return Err(anyhow::Error::from(e));
                }
                Err(Error::Io(e)) if e.kind() == ConnectionAborted => {
                    warn!(
                    "Client closed connection unexpectedly (connection abort)\nAddress:{:?}\nPlayerId:{:?}\nName:{:?}\nError:{:?}",
                    self.client_ws.peer_addr(),self.player_id, self.data.name,e
                );
                    return Err(anyhow::Error::new(e));
                }
                Err(Error::Io(e)) if e.kind() == TimedOut => {
                    warn!(
                    "Client stopped responding\nAddress:{:?}\nPlayerId:{:?}\nName:{:?}\nError:{:?}",
                    self.client_ws.peer_addr(),self.player_id, self.data.name,e
                );
                    return Err(anyhow::Error::new(e));
                }
                Err(Error::Io(e)) if e.kind() == WouldBlock => {
                    warn!(
                    "Client stopped responding\nAddress:{:?}\nPlayerId:{:?}\nName:{:?}\nError:{:?}",
                    self.client_ws.peer_addr(), self.player_id, self.data.name,e
                );
                    return Err(anyhow::Error::new(e));
                }
                Err(Error::Protocol(e)) if e == ResetWithoutClosingHandshake => {
                    warn!(
                    "Client stopped responding\nAddress:{:?}\nPlayerId:{:?}\nName:{:?}\nError: {:?}",
                    self.client_ws.peer_addr(), self.player_id, self.data.name,e
                );
                    return Err(anyhow::Error::new(e));
                }
                Err(err) => panic!(
                    "Could not receive: Address:{:?}\nPlayerId:{:?}\nName:{:?}\nError:{:?}",
                    self.client_ws.peer_addr(),
                    self.player_id,
                    self.data.name,
                    &err
                ),
            }
        }
        Err(anyhow::Error::msg("Message is None"))
    }

    /// Get a protobuf request from the client
    /// Returns None if the connection is already closed
    pub async fn client_get_request(&mut self) -> anyhow::Result<Request> {
        match self.client_recv().await? {
            TMessage::Binary(bytes) => {
                let resp = Message::parse_from_bytes(&bytes).expect("Invalid protobuf message");
                trace!(
                    "{:?} Message from client parsed:\n{:?}",
                    self.player_id,
                    &resp
                );
                Ok(resp)
            }
            TMessage::Close(e) => Err(anyhow::Error::msg(format!("{:?}", e))),
            other => panic!("Expected binary message, got {:?}", other),
        }
    }

    pub async fn client_get_request_raw(&mut self) -> anyhow::Result<Vec<u8>> {
        match self.client_recv().await? {
            TMessage::Binary(bytes) => Ok(bytes),
            TMessage::Close(e) => Err(anyhow::Error::msg(format!("{:?}", e))),
            other => panic!("Expected binary message, got {:?}", other),
        }
    }

    /// Send message to sc2
    /// Returns None if the connection is already closed
    async fn sc2_send(&mut self, msg: TMessage) -> Option<()> {
        self.sc2_ws.send(msg).await.ok()
    }

    /// Send protobuf request to sc2
    /// Returns None if the connection is already closed
    pub async fn sc2_request(&mut self, r: &Request) -> Option<()> {
        self.sc2_send(TMessage::binary(
            r.write_to_bytes().expect("Invalid protobuf message"),
        ))
        .await
    }
    pub async fn sc2_request_raw(&mut self, r: Vec<u8>) -> Option<()> {
        self.sc2_send(TMessage::binary(r)).await
    }

    /// Wait and receive a protobuf request from sc2
    /// Returns None if the connection is already closed
    pub async fn sc2_recv(&mut self) -> Option<Response> {
        match self.sc2_ws.next().await?.ok()? {
            TMessage::Binary(bytes) => {
                Some(Message::parse_from_bytes(&bytes).expect("Invalid data"))
            }
            TMessage::Close(_) => None,
            other => panic!("Expected binary message, got {:?}", other),
        }
    }
    pub async fn sc2_recv_raw(&mut self) -> Option<Vec<u8>> {
        match self.sc2_ws.next().await?.ok()? {
            TMessage::Binary(bytes) => Some(bytes),
            TMessage::Close(_) => None,
            other => panic!("Expected binary message, got {:?}", other),
        }
    }

    /// Send a request to SC2 and return the reponse
    /// Returns None if the connection is already closed
    pub async fn sc2_query(&mut self, r: &Request) -> Option<Response> {
        self.sc2_request(r).await?;
        self.sc2_recv().await
    }
    pub async fn sc2_query_raw(&mut self, r: Vec<u8>) -> Option<Vec<u8>> {
        self.sc2_request_raw(r).await?;
        self.sc2_recv_raw().await
    }
    /// Saves replay to path
    pub async fn save_replay(&mut self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }
        let mut r = Request::new();
        r.set_save_replay(RequestSaveReplay::new());
        if let Some(response) = self.sc2_query(&r).await {
            if response.has_save_replay() {
                match File::create(&path) {
                    Ok(mut buffer) => {
                        let data: &[u8] = response.save_replay().data();
                        buffer
                            .write_all(data)
                            .expect("Could not write to replay file");
                        info!("{:?}: Replay saved to {:?}", self.player_id, &path);
                        true
                    }
                    Err(e) => {
                        error!(
                            "{:?}:Failed to create replay file {:?}: {:?}",
                            self.player_id, &path, e
                        );
                        false
                    }
                }
            } else {
                error!("{:?}:No replay data available", self.player_id);
                false
            }
        } else {
            error!("{:?}:Could not save replay", self.player_id);
            false
        }
    }

    /// Run handler communication loop
    pub async fn run(mut self, config: Config, mut gamec: ChannelToGame) -> Option<Self> {
        let mut debug_response = Response::new();
        debug_response.set_id(0);
        debug_response.set_status(Status::in_game);
        let timeout_secs = Duration::from_secs(config.max_frame_time as u64);
        let replay_path = config.replay_path();
        let mut start_timer = false;
        let mut frame_time = 0_f32;
        let mut start_time: Instant = Instant::now();
        let mut surrender = false;
        let mut response_raw: Vec<u8>;

        // Get request
        while let Ok(Ok(req_raw)) = timeout(timeout_secs, self.client_get_request_raw()).await {
            self.request.merge_from_bytes(&req_raw).ok()?;
            if start_timer {
                frame_time += start_time.elapsed().as_secs_f32();
            }
            // Check for debug requests
            if config.disable_debug() && self.request.has_debug() {
                debug_response.set_id(self.request.id());
                self.client_respond(&debug_response).await;

                continue;
            } else if self.request.has_leave_game() {
                surrender = true;
                break;
            }

            for tag in self
                .request
                .action()
                .actions
                .iter()
                .filter(|a| a.action_chat.has_message())
                .filter_map(|x| {
                    let msg = x.action_chat.message();
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
            response_raw = match self.sc2_query_raw(req_raw).await {
                Some(d) => d,
                None => {
                    error!(
                        "{:?}: SC2 unexpectedly closed the connection",
                        self.player_id
                    );
                    gamec.send(ToGameContent::SC2UnexpectedConnectionClose);
                    debug!("{:?}: Killing the process", self.player_id);
                    self.process.kill();
                    return Some(self);
                }
            };

            self.response.merge_from_bytes(&response_raw).ok()?;
            self.sc2_status = Some(self.response.status());
            if self.response.has_game_info() {
                for pi in self.response.mut_game_info().player_info.iter_mut() {
                    if pi.player_id() != self.player_id.unwrap() {
                        pi.race_actual = pi.race_requested;
                    }
                }
                response_raw = self.response.write_to_bytes().unwrap();
            }

            // Send SC2 response to client
            self.client_respond_raw(&response_raw).await;
            start_timer = true;
            start_time = Instant::now();

            if self.response.has_quit() {
                self.save_replay(replay_path).await;
                self.frame_time = frame_time / self.game_loops as f32;
                self.frame_time = if self.frame_time.is_nan() {
                    0_f32
                } else {
                    self.frame_time
                };
                debug!("{:?}: SC2 is shutting down", self.player_id);
                gamec.send(ToGameContent::QuitBeforeLeave);
                debug!("{:?}: Waiting for the process", self.player_id);
                self.process.wait();
                return Some(self);
            } else if self.response.has_observation() {
                self.frame_time = frame_time / self.game_loops as f32;
                self.frame_time = if self.frame_time.is_nan() {
                    0_f32
                } else {
                    self.frame_time
                };

                let obs = self.response.observation();
                let obs_results = &obs.player_result;
                self.game_loops = obs.observation.game_loop();

                if !obs_results.is_empty() {
                    // Game is over and results available
                    let mut results_by_id: Vec<(u32, PlayerResult)> = obs_results
                        .iter()
                        .map(|r| (r.player_id(), PlayerResult::from_proto(r.result())))
                        .collect();
                    results_by_id.sort();
                    let results: Vec<_> = results_by_id.into_iter().map(|(_, v)| v).collect();
                    gamec.send(ToGameContent::GameOver(GameOver {
                        results,
                        game_loops: self.game_loops,
                        frame_time: self.frame_time,
                        tags: self.tags.iter().cloned().collect(),
                    }));
                    self.save_replay(replay_path).await;
                    self.process.kill();
                    return Some(self);
                }
                if self.game_loops > config.max_game_time() {
                    self.save_replay(replay_path).await;
                    self.frame_time = frame_time / self.game_loops as f32;
                    self.frame_time = if self.frame_time.is_nan() {
                        0_f32
                    } else {
                        self.frame_time
                    };
                    debug!("{:?}: Max time reached", self.player_id);
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
                self.save_replay(replay_path).await;
            }

            clear_request(&mut self.request);
            clear_response(&mut self.response);
            response_raw.clear();
        }
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
        info!(
            "{:?}: Killing process after unexpected connection close (Crash or Timeout)",
            self.player_id
        );
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
    pub interface_options: sc2_proto::sc2api::InterfaceOptions,
}

impl PlayerData {
    pub fn from_join_request(req: RequestJoinGame, archon: bool) -> Self {
        Self {
            race: Race::from_proto(req.race()),
            name: if req.has_player_name() {
                Some(req.player_name().to_owned())
            } else {
                None
            },

            interface_options: {
                let mut ifopts = req.options.unwrap();

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
    req.mut_unknown_fields().clear();
}

pub fn clear_response(response: &mut Response) {
    response.response = None;
    response.id = None;
    response.error.clear();
    response.status = None;
    response.mut_unknown_fields().clear();
}
