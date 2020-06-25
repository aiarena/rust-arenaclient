//! Game manages a single unstarted game, including its configuration

use log::{debug, error};
use std::thread::JoinHandle;

use protobuf::RepeatedField;
use sc2_proto::sc2api::RequestJoinGame;

use crate::maps::find_map;
use crate::portconfig::PortConfig;
use crate::proxy::Client;
use crate::sc2::{Difficulty, Race};

use super::game::Game;
use super::player::{Player, PlayerData};
use crate::config::Config;

/// An unstarted game
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
    /// Create new empty game lobby from config
    pub fn new(config: Config) -> Self {
        Self {
            config,
            players: Vec::new(),
            player_handles: Vec::new(),
        }
    }
    pub fn join_player_handles(&mut self) {
        while let Some(handle) = self.player_handles.pop() {
            self.players.push(handle.join().unwrap());
        }
    }
    /// Checks if this lobby has any player participants
    pub fn is_valid(&self) -> bool {
        !self.players.is_empty()
    }

    /// Add a new client to the game
    pub fn join(&mut self, connection: Client, join_req: RequestJoinGame, client_name: String) {
        let mut pd = PlayerData::from_join_request(join_req);
        pd.name = Some(client_name);
        self.player_handles.push(Player::new(connection, pd))
        // self.players.push(Player::new(connection, pd));
    }

    /// Protobuf to create a new game
    fn proto_create_game(&self, players: Vec<CreateGamePlayer>) -> sc2_proto::sc2api::Request {
        use sc2_proto::sc2api::{LocalMap, Request, RequestCreateGame};

        let mut r_local_map = LocalMap::new();
        r_local_map.set_map_path(
            find_map(self.config.map().clone()).expect("Map not found (Config::check?)"),
        );

        let mut r_create_game = RequestCreateGame::new();
        r_create_game.set_local_map(r_local_map);
        r_create_game.set_realtime(self.config.realtime());

        let p_cfgs: Vec<_> = players.iter().map(CreateGamePlayer::to_proto).collect();
        r_create_game.set_player_setup(RepeatedField::from_vec(p_cfgs));

        let mut request = Request::new();
        request.set_create_game(r_create_game);
        request
    }

    /// Create the game using the first client
    /// Returns None if game join fails (connection close or sc2 process close)
    #[must_use]
    pub fn create_game(&mut self) -> Option<()> {
        assert!(!self.players.is_empty());

        // Craft CrateGame request
        let mut player_configs: Vec<CreateGamePlayer> = Vec::new();

        // Participant players first
        for _ in &self.players {
            player_configs.push(CreateGamePlayer::Participant);
        }

        // TODO: Human players?
        // TODO: Observers?

        // Send CreateGame request to first process
        let proto = self.proto_create_game(player_configs);
        let response = self.players[0].sc2_query(proto)?;

        assert!(response.has_create_game());
        let resp_create_game = response.get_create_game();
        if resp_create_game.has_error() {
            error!("Could not create game: {:?}", resp_create_game.get_error());
            return None;
        } else {
            debug!("Game created succesfully");
        }

        Some(())
    }

    /// Protobuf to join a game
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
    /// Returns None iff game join fails (connection close or sc2 process close)
    #[must_use]
    pub fn join_all_game(&mut self) -> Option<()> {
        let pc = PortConfig::new().expect("Unable to find free ports");

        let protos: Vec<_> = self
            .players
            .iter()
            .map(|p| self.proto_join_game_participant(pc.clone(), p.data.clone()))
            .collect();

        for (player, proto) in self.players.iter_mut().zip(protos) {
            player.sc2_request(proto)?;
        }

        for player in self.players.iter_mut() {
            let response = player.sc2_recv()?;
            assert!(response.has_join_game());
            let resp_join_game = response.get_join_game();
            if resp_join_game.has_error() {
                error!("Could not join game: {:?}", resp_join_game.get_error());
                return None;
            } else {
                debug!("Game join succesful");
            }

            // No error, pass through the response
            player.client_respond(response);
        }

        // TODO: Human players?
        // TODO: Observers?

        Some(())
    }

    /// Start the game, and send responses to join requests
    /// Returns None if game create or join fails (connection close or sc2 process close)
    /// In that case, the connections are dropped (closed).
    #[must_use]
    pub fn start(mut self) -> Option<Game> {
        self.create_game()?;
        self.join_all_game()?;
        Some(Game {
            config: self.config,
            players: self.players,
        })
    }

    /// Destroy the lobby, closing all the connections
    pub fn close(self) {}
}

/// Used to pass player setup info to CreateGame
enum CreateGamePlayer {
    Participant,
    Computer(Race, Difficulty),
    Observer,
}
impl CreateGamePlayer {
    fn to_proto(&self) -> sc2_proto::sc2api::PlayerSetup {
        use sc2_proto::sc2api::{PlayerSetup, PlayerType};
        let mut ps = PlayerSetup::new();
        match self {
            Self::Participant => {
                ps.set_field_type(PlayerType::Participant);
            }
            Self::Computer(race, difficulty) => {
                ps.set_field_type(PlayerType::Computer);
                ps.set_race(race.to_proto());
                ps.set_difficulty(difficulty.to_proto());
            }
            Self::Observer => {
                ps.set_field_type(PlayerType::Observer);
            }
        }
        ps
    }
}
