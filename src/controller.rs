//! Game supervisor, manages games and passes messages

#![allow(dead_code)]

use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
// use std::collections::HashMap;
use std::io::ErrorKind::{WouldBlock, ConnectionAborted};

use websocket::message::{Message as ws_Message, OwnedMessage};
use websocket::result::WebSocketError;

use protobuf::parse_from_bytes;

use crate::config::Config;
use crate::game::{spawn as spawn_game, FromSupervisor, GameLobby, Handle as GameHandle};
use crate::proxy::Client;
use crate::result::JsonResult;
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
/// Game keeps same id from lobby creation until all clients leave the game
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameId(u64);
impl GameId {
    fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Controller manages a pool of games and client waiting for games
pub struct Controller {
    /// Connections (in nonblocking mode) waiting for a game
    /// If a game join is requested is pending (with remote), then also contains that
    clients: Vec<(String, Client, Option<RequestJoinGame>)>,
    supervisor: Option<Writer<TcpStream>>,
    // super_sender: Option<Writer<TcpStream>>,
    super_recv: Option<Receiver<SupervisorAction>>,
    config: Option<Config>,
    /// Games waiting for more players
    lobbies: HashMap<GameId, GameLobby>,
    /// Running games
    games: HashMap<GameId, GameHandle>,
    /// Id counter to allocate next id
    id_counter: GameId,
    /// Connected Clients
    pub connected_clients: usize,
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
            lobbies: HashMap::with_capacity(1),
            games: HashMap::with_capacity(1),
            id_counter: GameId(0),
            connected_clients: 0,
        }
    }
    /// Reset Controller for new game
    pub fn reset(&mut self) {
        self.clients = Vec::with_capacity(2);
        self.config = None;
        self.lobbies = HashMap::with_capacity(1);
        self.games = HashMap::with_capacity(1);
        self.id_counter = GameId(0);
        self.connected_clients = 0;
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
                println!("send_message: Supervisor not set");
            }
        }

        // match &mut self.supervisor {
        //     Some(sup) => {
        //         sup.set_nonblocking(false);
        //         sup.send_message(&ws_Message::text(message))
        //             .expect("Failed to send message to supervisor")
        //     }
        //     None => {
        //         println!("send_message: Supervisor not set");
        //     }
        // }
    }
    pub fn receive_confirmation(&mut self) {
        // match &mut self.supervisor {
        //     Some(sup) => {
        //         sup.recv_message()
        //             .expect("Failed to send message to supervisor");
        //     }
        //     None => {
        //         println!("receive_confirmation: Supervisor not set");
        //     }
        // }
    }
    /// Create new lobby
    fn create_lobby(&mut self) -> Option<GameId> {
        if let Some(config) = &self.config {
            let lobby = GameLobby::new(config.clone());
            let id = self.id_counter;
            debug_assert!(!self.lobbies.contains_key(&id));
            debug_assert!(!self.games.contains_key(&id));
            self.id_counter = self.id_counter.next();
            self.lobbies.insert(id, lobby);
            Some(id)
        } else {
            println!("Did not receive config");
            None
        }
    }
    pub fn has_supervisor(&self) -> bool {
        self.supervisor.is_some()
    }
    /// Add a new client socket to playlist
    pub fn add_client(&mut self, client: Client) {
        println!("Added client {:?}", client.peer_addr());
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
                    self.clients.push((config.player1(), client, None));
                    println!("{:?}", config.player1());
                    self.connected_clients += 1;
                } else {
                    self.clients.push((config.player2(), client, None));
                    self.connected_clients += 1;
                    println!("{:?}", config.player2());
                }
            }
            None => {
                println!("Config not set");
            }
        }
    }

    /// Add a new supervisor client socket
    pub fn add_supervisor(&mut self, client: Writer<TcpStream>, recv: Receiver<SupervisorAction>) {
        if self.supervisor.is_some() {
            println!("Supervisor already set - Resetting supervisor");
        }
        println!("Added supervisor");
        // client
        //     .set_nonblocking(false)
        //     .expect("Could not set non-blocking");
        // self.supervisor = Some(client);

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
        //         match recv.try_recv() {
        //         Ok(data) => data,
        //         Err(_) => SupervisorAction::NoAction,
        //     },
        //     _ => SupervisorAction::NoAction,
        // }
    }
    pub fn set_config(&mut self, config: String) {
        self.config = Some(Config::load_from_str(&config))
    }

    /// Remove client from playlist, closing the connection
    fn drop_client(&mut self, index: usize) {
        let (_, client, _) = &mut self.clients[index];
        info!(
            "Removing client {:?} from playlist",
            client.peer_addr().unwrap()
        );
        client.shutdown().expect("Connection shutdown failed");
        self.clients.remove(index);
    }

    /// Remove supervisor
    pub(crate) fn drop_supervisor(&mut self) {
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
    //
    // /// Gets a client index by identifier (peer address for now) if any
    // #[must_use]
    // pub fn client_index_by_id(&mut self, client_id: String) -> Option<usize> {
    //     self.playlist
    //         .iter()
    //         .enumerate()
    //         .filter(|(_, (c, _))| c.peer_addr().expect("Could not get peer_addr").to_string() == client_id)
    //         .map(|(i, _)| i)
    //         .nth(0)
    // }
    //
    /// Join to game from playlist
    /// If game join fails, drops connection
    #[must_use]
    fn client_join_game(&mut self, index: usize, req: RequestJoinGame) -> Option<()> {
        let (client_name, client, old_req) = self.clients.remove(index);

        if old_req != None {
            println!("Client attempted to join a game twice (dropping connection)");
            return None;
        }

        client
            .set_nonblocking(false)
            .expect("Could not set non-blocking");
        // TODO: Verify that InterfaceOptions are allowed
        // TODO: Fix this so it works without lobbies

        if let Some(&id) = self.lobbies.keys().next() {
            let mut lobby = self.lobbies.remove(&id).unwrap();
            lobby.join(client, req, client_name);
            lobby.join_player_handles();
            let game = lobby.start()?;
            self.games.insert(id, spawn_game(game));
        } else {
            let id = self.create_lobby().expect("Could not create lobby");
            let lobby = self.lobbies.get_mut(&id).unwrap();
            lobby.join(client, req, client_name);
        }

        Some(())
    }

    /// Process message from a client in the playlist
    fn process_client_message(&mut self, msg: OwnedMessage) -> PlaylistAction {
        match msg {
            OwnedMessage::Binary(bytes) => {
                let req = parse_from_bytes::<sc2_proto::sc2api::Request>(&bytes);
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
                        let pong = sc2_proto::sc2api::ResponsePing::new();
                        // TODO: Set pong fields, like game version?
                        resp.set_ping(pong);
                        PlaylistAction::respond(resp)
                    }
                    Ok(ref m) if m.has_join_game() => {
                        println!("Game join");
                        PlaylistAction::JoinGame(m.get_join_game().clone())
                    }
                    Ok(other) => {
                        println!("Unsupported message in playlist {:?}", other);
                        PlaylistAction::Kick
                    }
                    Err(err) => {
                        println!("Invalid message {:?}", err);
                        PlaylistAction::Kick
                    }
                }
            }
            other => {
                println!("Unsupported message type {:?}", other);
                PlaylistAction::Kick
            }
        }
    }
    // pub fn update_supervisor(&mut self) -> SupervisorAction {
    //     match &mut self.supervisor {
    //         Some(supervisor) => {
    //             if let Ok(msg) = supervisor.recv_message() {
    //                 if let OwnedMessage::Text(data) = msg {
    //                     println!("{:?}", data);
    //                     if data == "Reset" {
    //                         SupervisorAction::Quit
    //                     } else {
    //                         SupervisorAction::NoAction
    //                     }
    //                 } else {
    //                     SupervisorAction::NoAction
    //                 }
    //             } else {
    //                 SupervisorAction::NoAction
    //             }
    //         }
    //         None => SupervisorAction::NoAction,
    //     }
    // }
    /// Update clients in playlist to see if they join a game or disconnect
    pub fn update_clients(&mut self) {
        for i in (0..self.clients.len()).rev() {
            match self.clients[i].1.recv_message() {
                Ok(msg) => match self.process_client_message(msg) {
                    PlaylistAction::Kick => {
                        println!("Kick client");
                        self.drop_client(i)
                    }
                    PlaylistAction::Respond(resp) => {
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
                        self.drop_client(i);
                    }
                    PlaylistAction::JoinGame(req) => {
                        // println!("Join game received");
                        let join_response = self.client_join_game(i, req);
                        if join_response == None {
                            println!("Game creation / joining failed");
                        }
                    }
                },
                Err(WebSocketError::IoError(ref e)) if e.kind() == WouldBlock => {}
                Err(err) => {
                    println!("Invalid message {:?}", err);
                    self.drop_client(i);
                }
            };
        }
    }

    /// Update game handles to see if they are still running
    pub fn update_games(&mut self) {
        let mut games_over = Vec::new();
        for (id, game) in self.games.iter_mut() {
            if game.check() {
                println!("Game over");
                games_over.push(*id);
            }
        }

        for id in games_over {
            let game = self.games.remove(&id).unwrap();
            // let average_frame_time: Option<HashMap<String, f32>>;
            // let mut avg_hash: HashMap<String, f32> = HashMap::with_capacity(2);
            // for p in game.players.iter(){
            //     avg_hash.insert(p.player_name().unwrap(), p.frame_time);
            // }
            // average_frame_time = Some(avg_hash);
            match game.collect_result() {
                Ok((result, players)) => {
                    let average_frame_time: Option<HashMap<String, f32>>;
                    let mut avg_hash: HashMap<String, f32> = HashMap::with_capacity(2);
                    for p in players.into_iter() {
                        avg_hash.insert(p.player_name().unwrap(), p.frame_time);
                    }
                    average_frame_time = Some(avg_hash);
                    let player_results = result.player_results;

                    let p1 = self.config.clone().unwrap().clone().player1();
                    let p2 = self.config.clone().unwrap().clone().player2();
                    let mut game_result = HashMap::with_capacity(2);
                    game_result.insert(p1.clone(), player_results[0].to_string());
                    game_result.insert(p2.clone(), player_results[1].to_string());
                    let game_time = Some(result.game_loops);
                    let game_time_seconds = Some(game_time.unwrap() as f64 / 22.4);
                    println!("{:?}", game_result);
                   
                    let j_result = JsonResult::from(
                        Some(game_result),
                        game_time,
                        game_time_seconds,
                        None,
                        average_frame_time,
                        Some("Complete".to_string()),
                    );
                    self.send_message(j_result.serialize().as_ref());
                    self.receive_confirmation();
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
    }

    /// Destroys the controller, ending all games,
    /// and closing all connections and threads
    pub fn close(&mut self) {
        println!("Closing Controller");

        // Tell all games to quit
        for (_, game) in self.games.iter_mut() {
            game.send(FromSupervisor::Quit);
        }

        // Destroy all lobbies
        for (_, lobby) in self.lobbies.iter_mut() {
            lobby.close();
        }
        for (_, client, _) in &self.clients {
            client.shutdown().expect("Could not close connection");
        }
        self.reset()

        // Close all game list connections by drop
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
    thread::spawn(move || {
        loop {
            let r_msg = client_recv.recv_message();
            match r_msg {
                Ok(msg) => {
                    if let OwnedMessage::Text(data) = msg {
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
                }
                Err(WebSocketError::NoDataAvailable) => {
                    break;
                }
                Err(WebSocketError::IoError(ref e)) if e.kind() == ConnectionAborted => {
                    break;
                }
                Err(e) => println!("Supervisor receive error: {:?}", e),
            }
        }});
}
