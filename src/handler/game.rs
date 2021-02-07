//! Game manages a single handler, including configuration and result gathering

use crate::config::Config;
use crate::sc2::PlayerResult;
use crossbeam::channel::{select, Receiver, Sender};
use log::{debug, error, info};
use std::thread;

use super::any_panic_to_string;
use super::messaging::{create_channels, FromSupervisor, ToGame, ToGameContent, ToSupervisor};
use super::player::Player;
use crate::handler::messaging::ToPlayer;

/// Game result data
#[derive(Debug, Clone)]
pub struct GameResult {
    pub end_reason: GameEndReason,
    pub player_results: Vec<PlayerResult>,
    pub average_frame_time: Option<[f32; 2]>,
    pub game_loops: u32,
}

/// Why this handler ended
#[derive(Debug, Clone)]
pub enum GameEndReason {
    /// Game ended naturally
    Normal,
    /// Supervisor requested handler quit
    QuitRequest,
}

/// A running handler
#[derive(Debug)]
pub struct Game {
    /// Game configuration
    pub(super) config: Config,
    /// Player participants
    pub(super) players: Vec<Player>,
}
impl Game {
    /// Process a message from player thread
    fn process_msg(
        msg: ToGame,
        player_results: &mut Vec<Option<PlayerResult>>,
        game_loops: &mut u32,
        frame_times: &mut [f32; 2],
        game_over: &mut bool,
    ) {
        let ToGame {
            player_index,
            content,
        } = msg;
        match content {
            ToGameContent::GameOver(results) => {
                for (i, item) in player_results.iter_mut().enumerate() {
                    if item.is_none() {
                        *item = Some(results.0[i])
                    }
                }
                *game_loops = results.1;
                frame_times[player_index] = results.2;
                *game_over = true;
            }
            ToGameContent::LeftGame => {
                info!("Player left handler before it was over");
                player_results[player_index] = Some(PlayerResult::Defeat);
                *game_over = true;
            }
            ToGameContent::QuitBeforeLeave => {
                info!("Client quit without leaving the handler");
                player_results[player_index] = Some(PlayerResult::Defeat);
                *game_over = true;
            }
            ToGameContent::SC2UnexpectedConnectionClose => {
                info!("SC2 process closed connection unexpectedly");
                player_results[player_index] = Some(PlayerResult::SC2Crash);
                *game_over = true;
            }
            ToGameContent::UnexpectedConnectionClose => {
                info!("Unexpected connection close");
                player_results[player_index] = Some(PlayerResult::Crash);
                *game_over = true;
                match player_index {
                    0 => player_results[1] = Some(PlayerResult::Victory),
                    _ => player_results[0] = Some(PlayerResult::Victory),
                }
            }
        }
    }

    /// Run the handler, spawns thread for each participant player
    /// Returns the non-disconnected player instances, so they can be returned to the playlist
    pub fn run(
        self,
        result_tx: Sender<GameResult>,
        from_sv: Receiver<FromSupervisor>,
        _to_sv: Sender<ToSupervisor>,
    ) -> Vec<Player> {
        let mut handles: Vec<thread::JoinHandle<Option<Player>>> = Vec::new();
        let mut game_loops = 0_u32;
        let mut frame_times: [f32; 2] = [0_f32, 0_f32];
        let (rx, to_player_channels, player_channels) = create_channels(self.players.len());
        let mut player_results: Vec<Option<PlayerResult>> = vec![None; self.players.len()];
        let mut game_over = false;
        // Run games
        for (p, c) in self.players.into_iter().zip(player_channels) {
            let thread_config: Config = self.config.clone();
            let handle = thread::spawn(move || p.run(thread_config, c));
            handles.push(handle);
        }

        while !game_over {
            select! {
                // A client ended the handler
                recv(rx) -> r => match r {
                    Ok(msg) => {
                        Self::process_msg(msg, &mut player_results, &mut game_loops, &mut frame_times, &mut game_over);
                    },
                    Err(e) => panic!("Player channel closed without sending results {:?}",e),
                },
                recv(from_sv) -> r => match r {
                    Ok(FromSupervisor::Quit) => {
                        // Game quit requested
                        debug!("Supervisor requested handler quit");

                        result_tx
                            .send(GameResult {
                                end_reason: GameEndReason::QuitRequest,
                                player_results: Vec::new(),
                                game_loops: 0,
                                average_frame_time: None
                            })
                            .expect("Could not send results to the supervisor");

                        unimplemented!(); // TODO
                    },
                    Err(e) => panic!("Supervisor channel closed unexpectedly: {}", e),
                }
            }
        }

        info!("Game ready, results collected");
        for mut c in to_player_channels {
            if let Err(e) = c.send(ToPlayer::Quit) {
                error!("{:?}", e)
            }
        }

        // Wait until the games are ready
        let mut result_players: Vec<Player> = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(Some(player)) => {
                    result_players.push(player);
                }
                Ok(None) => {}
                Err(panic_msg) => {
                    panic!(
                        "Could not join handler-client thread: {:?}",
                        any_panic_to_string(panic_msg)
                    );
                }
            }
        }
        // Send handler result to the supervisor
        result_tx
            .send(GameResult {
                end_reason: GameEndReason::Normal,
                player_results: player_results.into_iter().map(Option::unwrap).collect(),
                average_frame_time: Some(frame_times),
                game_loops,
            })
            .expect("Could not send results to the supervisor");

        result_players
    }
}
