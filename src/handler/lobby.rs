//! Game manages a single unstarted handler, including its configuration

use log::{error, info};

use protobuf::RepeatedField;
use sc2_proto::sc2api::RequestJoinGame;
use std::thread::JoinHandle;

use crate::maps::find_map;
use crate::portconfig::PortConfig;
use crate::proxy::Client;

use super::game::Game;
use super::player::{Player, PlayerData};
use crate::config::Config;
use crate::sc2::Race;

/// An unstarted handler
#[derive(Debug)]
pub struct GameLobby {
    /// Game configuration
    config: Config,
    /// Player participants
    pub players: Vec<Player>,
    //Player handles
    player_handles: Vec<JoinHandle<Player>>,
}
impl GameLobby {
    /// Create new empty handler lobby from config
    pub fn new(config: Config) -> Self {
        Self {
            config,
            players: Vec::new(),
            player_handles: Vec::new(),
        }
    }
    pub async fn join_player_handles(&mut self) {
        while let Some(handle) = self.player_handles.pop() {
            // self.players.push(handle.join().unwrap());
            self.players.insert(0, handle.join().unwrap());
        }
    }
    /// Checks if this lobby has any player participants
    pub fn is_valid(&self) -> bool {
        !self.players.is_empty()
    }

    /// Add a new client to the handler
    pub async fn join(
        &mut self,
        connection: Client,
        join_req: RequestJoinGame,
        client_data: (String, Option<Race>),
        must_join: bool,
        player: PlayerNum,
    ) {
        let mut pd = PlayerData::from_join_request(join_req, self.config.archon());
        if self.config.validate_race() && client_data.1.is_some() {
            pd.race = client_data.1.unwrap();
        }
        pd.name = Some(client_data.0);
        if must_join {
            match player {
                PlayerNum::One => self
                    .players
                    .insert(0, Player::new_no_thread(connection, pd).await),
                PlayerNum::Two => self.players.push(Player::new_no_thread(connection, pd).await),
            }
        } else {
            match player {
                PlayerNum::One => self.player_handles.insert(0, Player::new(connection, pd).await),
                PlayerNum::Two => self.player_handles.push(Player::new(connection, pd).await),
            }
        }
    }

    /// Protobuf to create a new handler
    fn proto_create_game(&self, players: Vec<CreateGamePlayer>) -> sc2_proto::sc2api::Request {
        use sc2_proto::sc2api::{LocalMap, Request, RequestCreateGame};

        let mut r_local_map = LocalMap::new();
        let map = self.config.clone().map().clone();
        r_local_map.set_map_path(find_map(map).expect("Map not found (Config::check?)"));

        let mut r_create_game = RequestCreateGame::new();
        r_create_game.set_local_map(r_local_map);
        r_create_game.set_realtime(self.config.realtime());

        let p_cfgs: Vec<_> = players.iter().map(CreateGamePlayer::as_proto).collect();
        r_create_game.set_player_setup(RepeatedField::from_vec(p_cfgs));

        let mut request = Request::new();
        request.set_create_game(r_create_game);
        request
    }

    /// Create the handler using the first client
    /// Returns None if handler join fails (connection close or sc2 process close)
    #[must_use]
    pub async fn create_game(&mut self) -> Option<()> {
        assert!(!self.players.is_empty());

        // Craft CrateGame request
        let player_configs: Vec<CreateGamePlayer> =
            vec![CreateGamePlayer::Participant; self.players.len()];

        // TODO: Human players?
        // TODO: Observers?

        // Send CreateGame request to first process
        let proto = self.proto_create_game(player_configs);
        let response = self.players[0].sc2_query(&proto).await?;

        assert!(response.has_create_game());
        let resp_create_game = response.get_create_game();
        if resp_create_game.has_error() {
            error!(
                "Could not create handler: {:?}",
                resp_create_game.get_error()
            );
            return None;
        } else {
            info!("Game created successfully");
        }

        Some(())
    }

    /// Protobuf to join a handler
    fn proto_join_game_participant(
        &self,
        portconfig: PortConfig,
        player_data: PlayerData,
    ) -> sc2_proto::sc2api::Request {
        use sc2_proto::sc2api::Request;

        let mut r_join_game = RequestJoinGame::new();
        r_join_game.set_options(player_data.ifopts);
        r_join_game.set_race(player_data.race.to_proto());
        portconfig.apply_proto(&mut r_join_game, self.players.len() == 1);

        if let Some(name) = player_data.name {
            r_join_game.set_player_name(name);
        }
        let mut request = Request::new();
        request.set_join_game(r_join_game);
        request
    }

    /// Joins all participants to games
    /// Returns None iff handler join fails (connection close or sc2 process close)
    #[must_use]
    pub async fn join_all_game(&mut self) -> Option<()> {
        let pc = PortConfig::new().expect("Unable to find free ports");

        let protos: Vec<_> = self
            .players
            .iter()
            .map(|p| self.proto_join_game_participant(pc.clone(), p.data.clone()))
            .collect();

        for (player, proto) in self.players.iter_mut().zip(protos) {
            player.sc2_request(&proto).await?;
        }

        for player in self.players.iter_mut() {
            let response = player.sc2_recv().await?;
            assert!(response.has_join_game());
            let resp_join_game = response.get_join_game();
            player.player_id = Some(resp_join_game.get_player_id());
            if resp_join_game.has_error() {
                error!("Could not join handler: {:?}", resp_join_game.get_error());
                return None;
            } else {
                info!("Game join successful");
            }

            // No error, pass through the response
            player.client_respond(&response).await;
        }

        // TODO: Human players?
        // TODO: Observers?

        Some(())
    }

    /// Start the handler, and send responses to join requests
    /// Returns None if handler create or join fails (connection close or sc2 process close)
    /// In that case, the connections are dropped (closed).
    #[must_use]
    pub async fn start(mut self) -> Option<Game> {
        self.create_game().await?;
        self.join_all_game().await?;
        Some(Game {
            config: self.config,
            players: self.players,
        })
    }

    /// Destroy the lobby, closing all the connections
    pub async fn close(&mut self) {
        while let Some(handle) = self.player_handles.pop() {
            if let Ok(mut p) = handle.join() {
                p.process.kill()
            }
        }
    }
}

/// Used to pass player setup info to CreateGame
#[allow(dead_code)]
#[derive(Clone, Copy)]
enum CreateGamePlayer {
    Participant,
    Observer,
}
impl CreateGamePlayer {
    fn as_proto(&self) -> sc2_proto::sc2api::PlayerSetup {
        use sc2_proto::sc2api::{PlayerSetup, PlayerType};
        let mut ps = PlayerSetup::new();
        match self {
            Self::Participant => {
                ps.set_field_type(PlayerType::Participant);
            }
            Self::Observer => {
                ps.set_field_type(PlayerType::Observer);
            }
        }
        ps
    }
}

#[derive(Debug, Copy, Clone)]
pub enum PlayerNum {
    One,
    Two,
}
