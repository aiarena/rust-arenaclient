//! Game supervisor, manages games and passes messages

#![allow(dead_code)]

use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
use std::io::ErrorKind::{ConnectionAborted, ConnectionReset, WouldBlock};

use websocket::message::{Message as ws_Message, OwnedMessage};
use websocket::result::WebSocketError;

use crate::build_info::BuildInfo;
use crate::config::Config;
use crate::handler::{
    spawn as spawn_game, FromSupervisor, GameLobby, Handle as GameHandle, PlayerNum,
};
use crate::proxy::Client;
use crate::result::JsonResult;
use crate::sc2::Race;
use crossbeam::channel::{Receiver, Sender};
use protobuf::Message;
use sc2_proto::{self, sc2api::RequestJoinGame};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;
use websocket::sync::{Reader, Writer};
use websocket::websocket_base::stream::sync::TcpStream;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisorAction {
    ForceQuit,
    Quit,
    NoAction,
    Received,
    Config(String),
    Ping
}

enum PlaylistAction {
    Respond(OwnedMessage),
    RespondQuit(OwnedMessage),
    JoinGame(sc2_proto::sc2api::RequestJoinGame),
    Kick,
}
impl PlaylistAction {
    pub fn respond(r: sc2_proto::sc2api::Response) -> Self {
        let m = OwnedMessage::Binary(r.write_to_bytes().expect("Invalid protobuf message"));
        PlaylistAction::Respond(m)
    }
    pub fn respond_quit(r: sc2_proto::sc2api::Response) -> Self {
        let m = OwnedMessage::Binary(r.write_to_bytes().expect("Invalid protobuf message"));
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
    supervisor: Option<Writer<TcpStream>>,
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
    pub fn send_pong(&mut self){
        match &mut self.supervisor {
            Some(sender) => {
                sender
                    .send_message(&ws_Message::pong(vec![0_u8]))
                    .expect("Could not send message to supervisor");
            }
            None => {
                error!("send_message: Supervisor not set");
            }
        }
    }
    /// Sends a message to the supervisor
    pub fn send_message(&mut self, message: &str) {
        match &mut self.supervisor {
            Some(sender) => {
                sender
                    .send_message(&ws_Message::text(message))
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
        client
            .set_nonblocking(true)
            .expect("Could not set non-blocking");
        client
            .stream_ref()
            .set_read_timeout(Some(Duration::new(40, 0)))
            .expect("Could not set read timeout");
        debug_assert!(self.clients.len() < 2);
        match self.config.clone() {
            Some(config) => {
                if self.connected_clients == 0 {
                    debug!("Adding {}", config.player1());
                    self.clients.push((
                        (config.player1(), config.player1_bot_race()),
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
                        (config.player2(), config.player2_bot_race()),
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
    pub fn add_supervisor(&mut self, client: Writer<TcpStream>, recv: Receiver<SupervisorAction>) {
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
    fn drop_client(&mut self, index: usize) {
        let (_, client, _) = &mut self.clients[index];
        debug!(
            "Removing client {:?} from playlist",
            client.peer_addr().unwrap()
        );
        client.shutdown().expect("Connection shutdown failed");
        self.clients.remove(index);
    }

    /// Remove supervisor
    pub fn drop_supervisor(&mut self) {
        match &mut self.supervisor {
            Some(client) => {
                client
                    .shutdown()
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
    #[must_use]
    fn client_join_game(&mut self, index: usize, req: RequestJoinGame) -> Option<()> {
        let ((client_name, client_race), client, old_req) = self.clients.remove(index);
        debug!("{} client_join_game", client_name);
        if old_req != None {
            error!("Client attempted to join a handler twice (dropping connection)");
            return None;
        }

        client
            .set_nonblocking(false)
            .expect("Could not set non-blocking");
        // TODO: Verify that InterfaceOptions are allowed
        // TODO: Fix this so it works without lobbies
        let player = match client_name.clone() {
            n if n == self.config.as_ref().unwrap().player1() => PlayerNum::One,
            n if n == self.config.as_ref().unwrap().player2() => PlayerNum::Two,
            _ => panic!(),
        };
        if self.lobby.is_some() {
            let mut lobby = self.lobby.take().unwrap();
            lobby.join(
                client,
                req,
                (client_name, client_race),
                self.light_mode,
                player,
            );
            lobby.join_player_handles();
            let game = lobby.start()?;
            self.game = Some(spawn_game(game));
        } else if self.create_lobby() {
            let lobby = self.lobby.as_mut().unwrap();
            lobby.join(
                client,
                req,
                (client_name, client_race),
                self.light_mode,
                player,
            );
        } else {
            error!("Could not create lobby");
        }

        Some(())
    }

    /// Process message from a client in the playlist
    fn process_client_message(&mut self, msg: OwnedMessage) -> PlaylistAction {
        match msg {
            OwnedMessage::Binary(bytes) => {
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
                        PlaylistAction::JoinGame(m.get_join_game().clone())
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
    pub fn update_clients(&mut self) {
        for i in (0..self.clients.len()).rev() {
            match self.clients[i].1.recv_message() {
                Ok(msg) => match self.process_client_message(msg) {
                    PlaylistAction::Kick => {
                        debug!("Kick client");
                        self.drop_client(i)
                    }
                    PlaylistAction::Respond(resp) => {
                        debug!("Respond to {:?}", self.clients[i].0);
                        self.clients[i]
                            .1
                            .send_message(&resp)
                            .expect("Could not respond");
                    }
                    PlaylistAction::RespondQuit(resp) => {
                        self.clients[i]
                            .1
                            .send_message(&resp)
                            .expect("Could not respond");
                        debug!("RespondQuit");
                        self.drop_client(i);
                    }
                    PlaylistAction::JoinGame(req) => {
                        debug!("JoinGame from {:?}", self.clients[i].0);
                        let join_response = self.client_join_game(i, req);

                        if join_response == None {
                            error!("Game creation / joining failed");
                        }
                    }
                },
                Err(WebSocketError::IoError(ref e)) if e.kind() == WouldBlock => {}
                Err(err) => {
                    error!("Invalid message {:?}", err);
                    self.drop_client(i);
                }
            };
        }
    }

    /// Update handler handles to see if they are still running
    pub fn update_games(&mut self) {
        let mut game_over = false;
        if let Some(game) = &mut self.game {
            if game.check() {
                game_over = true;
            }
        }
        if game_over {
            let game = self.game.take().unwrap();
            match game.collect_result() {
                Ok((result, players)) => {                    
                    let average_frame_time: Option<HashMap<String, f32>>;
                    let mut avg_hash: HashMap<String, f32> = HashMap::with_capacity(2);
                    for p in players.into_iter() {
                        avg_hash.insert(p.player_name().unwrap(), p.frame_time);
                    }
                    average_frame_time = Some(avg_hash);
                    let player_results = result.player_results;

                    let p1 = self.config.clone().unwrap().player1();
                    let p2 = self.config.clone().unwrap().player2();
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
                        match self.config.as_ref(){
                            Some(x) => Some(x.map.clone()),
                            None => None
                        },
                        match self.config.as_ref(){
                            Some(x) => Some(x.replay_name.clone()),
                            None => None
                        }
                    );
                    self.send_message(j_result.serialize().as_ref());

                    for i in (0..self.clients.len()).rev() {
                        self.drop_client(i)
                    }
                    self.drop_supervisor();
                    self.reset();
                    // println!("Game result: {:?}", result);
                }
                Err(msg) => {
                    error!("Game thread panicked with: {:?}", msg);
                }
            }
        }
        // let mut games_over = Vec::new();
        // for (id, game) in self.game.iter_mut() {
        //     if game.check() {
        //         println!("Game over");
        //         games_over.push(*id);
        //     }
        // }
        //
        // for id in games_over {
        //     let game = self.game.remove(&id).unwrap();
        //     // let average_frame_time: Option<HashMap<String, f32>>;
        //     // let mut avg_hash: HashMap<String, f32> = HashMap::with_capacity(2);
        //     // for p in handler.players.iter(){
        //     //     avg_hash.insert(p.player_name().unwrap(), p.frame_time);
        //     // }
        //     // average_frame_time = Some(avg_hash);
        //     match game.collect_result() {
        //         Ok((result, players)) => {
        //             let average_frame_time: Option<HashMap<String, f32>>;
        //             let mut avg_hash: HashMap<String, f32> = HashMap::with_capacity(2);
        //             for p in players.into_iter() {
        //                 avg_hash.insert(p.player_name().unwrap(), p.frame_time);
        //             }
        //             average_frame_time = Some(avg_hash);
        //             let player_results = result.player_results;
        //
        //             let p1 = self.config.clone().unwrap().clone().player1();
        //             let p2 = self.config.clone().unwrap().clone().player2();
        //             let mut game_result = HashMap::with_capacity(2);
        //             game_result.insert(p1.clone(), player_results[0].to_string());
        //             game_result.insert(p2.clone(), player_results[1].to_string());
        //             let game_time = Some(result.game_loops);
        //             let game_time_seconds = Some(game_time.unwrap() as f64 / 22.4);
        //             println!("{:?}", game_result);
        //
        //             let j_result = JsonResult::from(
        //                 Some(game_result),
        //                 game_time,
        //                 game_time_seconds,
        //                 None,
        //                 average_frame_time,
        //                 Some("Complete".to_string()),
        //             );
        //             self.send_message(j_result.serialize().as_ref());
        //             self.receive_confirmation();
        //             for i in (0..self.clients.len()).rev() {
        //                 self.drop_client(i)
        //             }
        //             self.drop_supervisor();
        //             self.reset();
        //             // println!("Game result: {:?}", result);
        //         }
        //         Err(msg) => {
        //             error!("Game thread panicked with: {:?}", msg);
        //         }
        //     }
        // }
    }

    /// Destroys the controller, ending all games,
    /// and closing all connections and threads
    pub fn close(&mut self) {
        debug!("Closing Controller");

        // Tell game to quit
        if let Some(game) = &mut self.game {
            game.send(FromSupervisor::Quit);
        }
        // Destroy lobby
        if let Some(lobby) = &mut self.lobby {
            lobby.close();
        }

        for (_, client, _) in &self.clients {
            client.shutdown().expect("Could not close connection");
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
    mut client_recv: Reader<TcpStream>,
    sender: Sender<SupervisorAction>,
) {
    thread::spawn(move || loop {
        let r_msg = client_recv.recv_message();
        trace!("message received from supervisor client");
        match r_msg {
            Ok(msg) => {
                match msg{
                    OwnedMessage::Text(data) =>{
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
                },
                
               OwnedMessage::Ping(_) =>{
                    sender.send(SupervisorAction::Ping).expect("Could not send SupervisorAction");
                },
                _ => {}
            }
        }
            Err(WebSocketError::NoDataAvailable) => {
                break;
            }
            Err(WebSocketError::IoError(ref e)) if e.kind() == ConnectionAborted => {
                sender
                    .send(SupervisorAction::ForceQuit)
                    .expect("Could not send ForceQuit");
                break;
            }
            Err(WebSocketError::IoError(ref e)) if e.kind() == ConnectionReset => {
                sender
                    .send(SupervisorAction::ForceQuit)
                    .expect("Could not send ForceQuit");
                break;
            }
            Err(e) => {
                sender
                    .send(SupervisorAction::ForceQuit)
                    .expect("Could not send ForceQuit");
                error!("Supervisor receive error: {:?}", e)
            }
        }
    });
}
