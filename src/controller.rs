//! Game supervisor, manages games and passes messages

#![allow(dead_code)]

use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};

use crate::build_info::BuildInfo;
use crate::config::Config;
use crate::handler::{spawn_game, FromSupervisor, GameLobby, Handle as GameHandle, PlayerNum};
use crate::proxy::Client;
use crate::result::JsonResult;
use crate::sc2::Race;
use crossbeam::channel::{Receiver, Sender};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use protobuf::Message;
use sc2_proto::{self, sc2api::RequestJoinGame};
use std::collections::HashMap;
use tokio::net::TcpStream;
use tokio::runtime::Runtime;
use tokio_tungstenite::tungstenite::error::Error;
use tokio_tungstenite::tungstenite::Message as TMessage;
use tokio_tungstenite::WebSocketStream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorAction {
    ForceQuit,
    Quit,
    NoAction,
    Received,
    Config(String),
    Ping(Vec<u8>),
}

enum PlaylistAction {
    Respond(TMessage),
    RespondQuit(TMessage),
    JoinGame(RequestJoinGame),
    Kick,
}

impl PlaylistAction {
    pub fn respond(r: sc2_proto::sc2api::Response) -> Self {
        let m = TMessage::Binary(r.write_to_bytes().expect("Invalid protobuf message"));
        PlaylistAction::Respond(m)
    }
    pub fn respond_quit(r: sc2_proto::sc2api::Response) -> Self {
        let m = TMessage::Binary(r.write_to_bytes().expect("Invalid protobuf message"));
        PlaylistAction::RespondQuit(m)
    }
}

/// Unique identifier for lobby and running games
/// Game keeps same id from lobby creation until all clients leave the handler
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameId(u64);

impl GameId {
    fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

pub type BotData = (String, Option<Race>);

/// Controller manages a pool of games and client waiting for games
pub struct Controller {
    /// Connections (in non-blocking mode) waiting for a handler
    /// If a handler join is requested is pending (with remote), then also contains that
    clients: Vec<(BotData, Client, Option<RequestJoinGame>)>,
    /// Supervisor channel writer
    supervisor: Option<SplitSink<WebSocketStream<TcpStream>, TMessage>>,
    /// Supervisor channel receiver
    super_recv: Option<Receiver<SupervisorAction>>,
    /// Game config received from supervisor
    config: Option<Config>,
    /// Pre-game lobby
    lobby: Option<GameLobby>,
    /// Running games
    game: Option<GameHandle>,
    /// Connected Clients
    pub connected_clients: usize,
    light_mode: bool,
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

impl Controller {
    /// Create new empty controller from config
    pub fn new() -> Self {
        Self {
            clients: Vec::with_capacity(2),
            supervisor: None,
            super_recv: None,
            config: None,
            lobby: None,
            game: None,
            connected_clients: 0,
            light_mode: false,
        }
    }
    /// Reset Controller for new handler
    pub fn reset(&mut self) {
        self.clients = Vec::with_capacity(2);
        self.config = None;
        self.lobby = None;
        self.game = None;
        self.connected_clients = 0;
    }
    pub async fn send_pong(&mut self, payload: Vec<u8>) {
        match &mut self.supervisor {
            Some(sender) => {
                sender
                    .send(TMessage::Pong(payload))
                    .await
                    .expect("Could not send message to supervisor");
            }
            None => {
                error!("send_message: Supervisor not set");
            }
        }
    }
    /// Sends a message to the supervisor
    pub async fn send_message(&mut self, message: &str) {
        match &mut self.supervisor {
            Some(sender) => {
                sender
                    .send(TMessage::text(message))
                    .await
                    .expect("Could not send message to supervisor");
            }
            None => {
                error!("send_message: Supervisor not set");
            }
        }
    }
    /// Create new lobby
    fn create_lobby(&mut self) -> bool {
        if let Some(config) = &self.config {
            self.lobby = Some(GameLobby::new(config.clone()));
            true
        } else {
            error!("Did not receive config from supervisor");
            false
        }
    }
    pub fn has_supervisor(&self) -> bool {
        self.supervisor.is_some()
    }
    /// Add a new client socket to playlist
    pub fn add_client(&mut self, client: Client) {
        info!("Added client {:?}", client.peer_addr());
        debug_assert!(self.clients.len() < 2);
        match self.config.clone() {
            Some(config) => {
                if self.connected_clients == 0 {
                    debug!("Adding {}", config.player1());
                    self.clients.push((
                        (config.player1().to_string(), config.player1_bot_race()),
                        client,
                        None,
                    ));
                    info!(
                        "{:?} playing {:?}",
                        config.player1(),
                        config.player1_bot_race()
                    );
                    self.connected_clients += 1;
                } else {
                    debug!("Adding {}", config.player2());
                    self.clients.push((
                        (config.player2().to_string(), config.player2_bot_race()),
                        client,
                        None,
                    ));
                    self.connected_clients += 1;
                    info!(
                        "{:?} playing {:?}",
                        config.player2(),
                        config.player2_bot_race()
                    );
                }
            }
            None => {
                panic!("Config not set");
            }
        }
    }

    /// Add a new supervisor client socket
    pub fn add_supervisor(
        &mut self,
        client: SplitSink<WebSocketStream<TcpStream>, TMessage>,
        recv: Receiver<SupervisorAction>,
    ) {
        if self.supervisor.is_some() {
            error!("Supervisor already set - Resetting supervisor");
        }
        debug!("Added supervisor");
        self.supervisor = Some(client);
        self.super_recv = Some(recv);
    }

    pub fn recv_msg(&self) -> Option<SupervisorAction> {
        match &self.super_recv {
            Some(recv) => {
                while let Ok(data) = recv.try_recv() {
                    if data != SupervisorAction::NoAction {
                        return Some(data);
                    } else {
                        continue;
                    }
                }
                None
            }
            None => None,
        }
    }
    pub fn set_config(&mut self, config: String) {
        let config = Config::load_from_str(&config);
        self.light_mode = config.light_mode();
        self.config = Some(config)
    }

    /// Remove client from playlist, closing the connection
    async fn drop_client(&mut self, index: usize) {
        let (_, client, _) = &mut self.clients[index];
        debug!("Removing client {:?} from playlist", client.peer_addr());
        client.shutdown().await.expect("Connection shutdown failed");
        self.clients.remove(index);
    }

    /// Remove supervisor
    pub async fn drop_supervisor(&mut self) {
        match &mut self.supervisor {
            Some(client) => {
                client
                    .close()
                    .await
                    .expect("Supervisor connection shutdown failed");
                self.supervisor = None;
            }
            None => {
                error!("Cannot drop - No supervisor set");
            }
        };
        self.super_recv = None;
    }

    /// Join to handler from playlist
    /// If handler join fails, drops connection
    async fn client_join_game(&mut self, index: usize, req: RequestJoinGame) -> Option<()> {
        let ((client_name, client_race), client, old_req) = self.clients.remove(index);
        debug!("{} client_join_game", client_name);
        if old_req.is_some() {
            error!("Client attempted to join a handler twice (dropping connection)");
            return None;
        }
        // TODO: Verify that InterfaceOptions are allowed
        // TODO: Fix this so it works without lobbies
        let player = match client_name.clone() {
            n if n == self.config.as_ref().unwrap().player1() => PlayerNum::One,
            n if n == self.config.as_ref().unwrap().player2() => PlayerNum::Two,
            _ => panic!(),
        };
        if self.lobby.is_some() {
            trace!("Lobby exists");
            let mut lobby = self.lobby.take().unwrap();
            lobby
                .join(
                    client,
                    req,
                    (client_name, client_race),
                    self.light_mode,
                    player,
                )
                .await;
            lobby.join_player_handles().await;
            let game = lobby.start().await?;
            self.game = Some(spawn_game(game));
        } else if self.create_lobby() {
            trace!("Create new lobby");
            let lobby = self.lobby.as_mut().unwrap();
            lobby
                .join(
                    client,
                    req,
                    (client_name, client_race),
                    self.light_mode,
                    player,
                )
                .await;
        } else {
            error!("Could not create lobby");
        }

        Some(())
    }

    /// Process message from a client in the playlist
    fn process_client_message(&mut self, msg: TMessage) -> PlaylistAction {
        match msg {
            TMessage::Binary(bytes) => {
                let req = sc2_proto::sc2api::Request::parse_from_bytes(&bytes);
                debug!("Incoming playlist request: {:?}", req);

                match req {
                    Ok(ref m) if m.has_quit() => {
                        info!("Client quit");
                        let mut resp = sc2_proto::sc2api::Response::new();
                        let quit = sc2_proto::sc2api::ResponseQuit::new();
                        resp.set_quit(quit);
                        PlaylistAction::respond_quit(resp)
                    }
                    Ok(ref m) if m.has_ping() => {
                        trace!("Ping => Pong");
                        let mut resp = sc2_proto::sc2api::Response::new();
                        let mut pong = sc2_proto::sc2api::ResponsePing::new();
                        let b = BuildInfo::get_build_info_from_file();
                        pong.set_game_version(b.version);
                        pong.set_base_build(b.base_build);
                        pong.set_data_build(b.data_build);
                        pong.set_data_version("".to_string());
                        resp.set_ping(pong);
                        PlaylistAction::respond(resp)
                    }
                    Ok(ref m) if m.has_join_game() => {
                        info!("Game join");
                        PlaylistAction::JoinGame(m.join_game().clone())
                    }
                    Ok(other) => {
                        error!("Unsupported message in playlist {:?}", other);
                        PlaylistAction::Kick
                    }
                    Err(err) => {
                        error!("Invalid message {:?}", err);
                        PlaylistAction::Kick
                    }
                }
            }
            other => {
                error!("Unsupported message type {:?}", other);
                PlaylistAction::Kick
            }
        }
    }

    /// Update clients in playlist to see if they join a handler or disconnect
    pub async fn update_clients(&mut self) {
        for i in (0..self.clients.len()).rev() {
            match self.clients[i].1.recv_message().await {
                Some(Ok(msg)) => match self.process_client_message(msg) {
                    PlaylistAction::Kick => {
                        debug!("Kick client");
                        self.drop_client(i).await
                    }
                    PlaylistAction::Respond(resp) => {
                        debug!("Respond to {:?}", self.clients[i].0);
                        self.clients[i]
                            .1
                            .stream
                            .send(resp)
                            .await
                            .expect("Could not respond");
                    }
                    PlaylistAction::RespondQuit(resp) => {
                        self.clients[i]
                            .1
                            .stream
                            .send(resp)
                            .await
                            .expect("Could not respond");
                        debug!("RespondQuit");
                        self.drop_client(i).await;
                    }
                    PlaylistAction::JoinGame(req) => {
                        debug!("JoinGame from {:?}", self.clients[i].0);
                        let join_response = self.client_join_game(i, req).await;

                        if join_response.is_none() {
                            error!("Game creation / joining failed");
                        }
                    }
                },
                None => {
                    error!("None message");
                }
                Some(Err(err)) => {
                    error!("Invalid message {:?}", err);
                    self.drop_client(i).await;
                }
            };
        }
    }

    /// Update handler handles to see if they are still running
    pub async fn update_games(&mut self) {
        let mut game_over = false;
        if let Some(game) = &mut self.game {
            if game.check() {
                game_over = true;
            }
        }
        if game_over {
            let game = self.game.take().unwrap();
            match game.collect_result().await {
                Ok((result, players)) => {
                    let mut avg_hash: HashMap<String, f32> = HashMap::with_capacity(2);
                    let mut tags_hash: HashMap<String, Vec<String>> = HashMap::with_capacity(2);
                    for p in players.iter() {
                        let player_name = p.player_name().as_ref().unwrap().to_string();
                        avg_hash.insert(player_name.clone(), p.frame_time);
                        tags_hash.insert(player_name.clone(), p.tags.iter().cloned().collect());
                    }
                    let tags: Option<HashMap<String, Vec<String>>> = Some(tags_hash);
                    let average_frame_time: Option<HashMap<String, f32>> = Some(avg_hash);

                    let player_results = result.player_results;

                    let p1 = self.config.as_ref().unwrap().player1().to_string();
                    let p2 = self.config.as_ref().unwrap().player2().to_string();
                    let mut game_result = HashMap::with_capacity(1);
                    game_result.insert(p1.clone(), player_results[0].to_string());
                    game_result.insert(p2.clone(), player_results[1].to_string());
                    let game_time = Some(result.game_loops);
                    let mut bots: HashMap<u8, String> = HashMap::with_capacity(1);
                    bots.insert(1, p1);
                    bots.insert(2, p2);
                    let game_time_seconds = Some(game_time.unwrap() as f64 / 22.4);
                    info!("{:?}", game_result);

                    let j_result = JsonResult::from(
                        Some(game_result),
                        game_time,
                        game_time_seconds,
                        None,
                        average_frame_time,
                        Some("Complete".to_string()),
                        Some(bots),
                        self.config.as_ref().map(|x| x.map.clone()),
                        self.config.as_ref().map(|x| x.replay_name.clone()),
                        self.config.as_ref().map(|x| x.match_id),
                        tags,
                    );
                    self.send_message(j_result.serialize().as_ref()).await;

                    for i in (0..self.clients.len()).rev() {
                        self.drop_client(i).await
                    }
                    self.drop_supervisor().await;
                    self.reset();
                }
                Err(msg) => {
                    error!("Game thread panicked with: {:?}", msg);
                }
            }
        }
    }

    /// Destroys the controller, ending all games,
    /// and closing all connections and threads
    pub async fn close(&mut self) {
        debug!("Closing Controller");

        // Tell game to quit
        if let Some(game) = &mut self.game {
            game.send(FromSupervisor::Quit);
        }
        // Destroy lobby
        if let Some(lobby) = &mut self.lobby {
            lobby.close().await;
        }

        for (_, client, _) in &mut self.clients {
            client.shutdown().await.expect("Could not close connection");
        }
        self.reset()

        // Close all handler list connections by drop
    }
}

/// Return type of Controller.update_remote
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteUpdateStatus {
    /// Server quit requested
    Quit,
    /// A request was processed
    Processed,
    /// No action was taken
    NoAction,
}

pub fn create_supervisor_listener(
    mut client_recv: SplitStream<WebSocketStream<TcpStream>>,
    sender: Sender<SupervisorAction>,
) {
    std::thread::spawn(move || {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            while let Some(r_msg) = client_recv.next().await {
                trace!("Message received from supervisor client");
                match r_msg {
                    Ok(msg) => match msg {
                        TMessage::Text(data) => {
                            if data == "Reset" {
                                sender
                                    .send(SupervisorAction::Quit)
                                    .expect("Could not send SupervisorAction");
                                break;
                            } else if data == "Received" {
                                sender
                                    .send(SupervisorAction::Received)
                                    .expect("Could not send SupervisorAction");
                            } else if data.contains("Map") || data.contains("map") {
                                sender
                                    .send(SupervisorAction::Config(data))
                                    .expect("Could not send config");
                            } else if data == "Quit" {
                                sender
                                    .send(SupervisorAction::ForceQuit)
                                    .expect("Could not send ForceQuit");
                            }
                        }
                        TMessage::Ping(payload) => {
                            sender
                                .send(SupervisorAction::Ping(payload))
                                .expect("Could not send SupervisorAction");
                        }
                        _ => {}
                    },
                    Err(Error::AlreadyClosed) => {
                        error!("Supervisor Error::AlreadyClosed");
                        sender
                            .send(SupervisorAction::ForceQuit)
                            .expect("Could not send ForceQuit");
                        break;
                    }
                    Err(Error::Capacity(e)) => {
                        error!("{:?}", e);
                        sender
                            .send(SupervisorAction::ForceQuit)
                            .expect("Could not send ForceQuit");
                        break;
                    }
                    Err(e) => {
                        error!("{:?}", e);
                        sender
                            .send(SupervisorAction::ForceQuit)
                            .expect("Could not send ForceQuit");
                        break;
                    }
                }
            }
        });
    });
}
