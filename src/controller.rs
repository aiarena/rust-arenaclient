//! Game supervisor, manages games and passes messages

#![allow(dead_code)]

use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
// use std::collections::HashMap;
use std::io::ErrorKind::WouldBlock;

use websocket::message::{Message as ws_Message, OwnedMessage};
use websocket::result::WebSocketError;

use protobuf::parse_from_bytes;

use crate::config::Config;
use crate::game::{
spawn as spawn_game, FromSupervisor, GameLobby, Handle as GameHandle,
};
use crate::proxy::Client;
use crate::result::JsonResult;
use protobuf::Message;
use sc2_proto::{self, sc2api::RequestJoinGame};
use std::collections::HashMap;

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
    supervisor: Option<Client>,
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
        self.supervisor = None;
        self.config = None;
        self.lobbies = HashMap::with_capacity(1);
        self.games = HashMap::with_capacity(1);
        self.id_counter = GameId(0);
        self.connected_clients = 0;
    }
    /// Sends a message to the supervisor
    pub fn send_message(&mut self, message: &str) {
        match &mut self.supervisor {
            Some(sup) => sup
                .send_message(&ws_Message::text(message))
                .expect("Failed to send message to supervisor"),
            None => {
                println!("Supervisor not set");
            }
        }
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

    /// Add a new client socket to playlist
    pub fn add_client(&mut self, client: Client) {
        debug!("Added client");
        client
            .set_nonblocking(true)
            .expect("Could not set non-blocking");
        debug_assert!(self.clients.len() < 2);
        match self.config.clone() {
            Some(config) => {
                if self.connected_clients == 0 {
                    self.clients.push((config.player1(), client, None));
                    self.connected_clients += 1;
                } else {
                    self.clients.push((config.player2(), client, None));
                    self.connected_clients += 1;
                }
            }
            None => {
                println!("Config not set");
            }
        }
    }

    /// Add a new supervisor client socket
    pub fn add_supervisor(&mut self, client: Client) {
        if self.supervisor.is_some() {
            println!("Supervisor already set - Resetting supervisor");
        }
        debug!("Added supervisor");
        client
            .set_nonblocking(true)
            .expect("Could not set non-blocking");
        self.supervisor = Some(client);
    }

    /// Get the config from the supervisor
    pub fn get_config_from_supervisor(&mut self) {
        if self.config.is_none() {
            match &mut self.supervisor {
                Some(sup) => match sup.recv_message() {
                    Ok(msg) => match msg {
                        OwnedMessage::Text(data) => {
                            self.config = Some(Config::load_from_str(&data));
                        }
                        e => println!("Unknown message: Expected Text, received {:?}", &e),
                    },
                    Err(e) => {
                        println!("Could not receive from supervisor: {:?}", &e);
                    }
                },
                None => {
                    println!("No Supervisor set");
                }
            }
        }
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
    fn drop_supervisor(&mut self) {
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
        }
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
            warn!("Client attempted to join a game twice (dropping connection)");
            return None;
        }

        client
            .set_nonblocking(false)
            .expect("Could not set non-blocking");
        // println!("Joining game");
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
                        debug!("Game join");
                        PlaylistAction::JoinGame(m.get_join_game().clone())
                    }
                    Ok(other) => {
                        warn!("Unsupported message in playlist {:?}", other);
                        PlaylistAction::Kick
                    }
                    Err(err) => {
                        warn!("Invalid message {:?}", err);
                        PlaylistAction::Kick
                    }
                }
            }
            other => {
                warn!("Unsupported message type {:?}", other);
                PlaylistAction::Kick
            }
        }
    }

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
                            warn!("Game creation / joining failed");
                        }
                    }
                },
                Err(WebSocketError::IoError(ref e)) if e.kind() == WouldBlock => {}
                Err(err) => {
                    warn!("Invalid message {:?}", err);
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
                println!("game over");
                games_over.push(id.clone());
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
                    for p in players.into_iter(){
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
                    // let average_frame_time: Option<HashMap<String, f32>>;

                    // if let Some(avg) = result.average_frame_time {
                    //     println!("{:?}", avg);
                    //     let mut avg_hash: HashMap<String, f32> = HashMap::with_capacity(2);
                    //     avg_hash.insert(p1.clone(), avg[0]);
                    //     avg_hash.insert(p2.clone(), avg[1]);
                    //     average_frame_time = Some(avg_hash);
                    // } else {
                    //     average_frame_time = None
                    // }
                    let j_result = JsonResult::from(
                        Some(game_result),
                        game_time,
                        game_time_seconds,
                        None,
                        average_frame_time,
                        Some("Complete".to_string()),
                    );
                    self.send_message(j_result.serialize().as_ref());
                    for i in (0..self.clients.len()).rev() {
                        self.drop_client(i)
                    }
                    self.drop_supervisor();
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
    pub fn close(self) {
        debug!("Closing Controller");

        // Tell all games to quit
        for (_id, mut game) in self.games.into_iter() {
            game.send(FromSupervisor::Quit);
        }

        // Destroy all lobbies
        for (_id, lobby) in self.lobbies.into_iter() {
            lobby.close();
        }

        // Close all game list connections by drop
    }
}

/// Return type of Controller.update_remote
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteUpdateStatus {
    /// Server quit requested
    Quit,
    /// A request was processed
    Processed,
    /// No action was taken
    NoAction,
}
