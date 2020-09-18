#![allow(missing_docs)]
mod race;
use crate::config::race::BotRace;
use crate::sc2::Race;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub(crate) pids: Vec<u32>,
    #[serde(default)]
    pub(crate) average_frame_time: Vec<HashMap<String, f32>>,
    #[serde(default, alias = "Map")]
    pub(crate) map: String,
    #[serde(default, alias = "MaxGameTime")]
    pub(crate) max_game_time: u32,
    #[serde(default, alias = "MaxFrameTime")]
    pub(crate) max_frame_time: i32,
    #[serde(default, alias = "Strikes")]
    pub(crate) strikes: i32,
    #[serde(default)]
    pub(crate) result: Vec<HashMap<String, String>>,
    #[serde(default, alias = "Player1")]
    pub(crate) player1: String,
    #[serde(default, alias = "Player2")]
    pub(crate) player2: String,
    #[serde(default, alias = "ReplayPath")]
    pub(crate) replay_path: String,
    #[serde(default, alias = "MatchID")]
    pub(crate) match_id: i64,
    #[serde(default, alias = "ReplayName")]
    pub(crate) replay_name: String,
    #[serde(default)]
    pub(crate) game_time: f32,
    #[serde(default)]
    pub(crate) game_time_seconds: f32,
    #[serde(default)]
    pub(crate) game_time_formatted: String,
    #[serde(default, alias = "DisableDebug")]
    pub(crate) disable_debug: bool,
    #[serde(default, alias = "RealTime")]
    pub(crate) real_time: bool,
    #[serde(default, alias = "Visualize")]
    pub(crate) visualize: bool,
    #[serde(default, alias = "LightMode")]
    pub(crate) light_mode: bool,
    #[serde(default, alias = "ValidateRace")]
    pub(crate) validate_race: bool,
    #[serde(default, alias = "Player1Race")]
    pub(crate) player1_race: Option<String>,
    #[serde(default, alias = "Player2Race")]
    pub(crate) player2_race: Option<String>,
    #[serde(default, alias = "Archon")]
    pub(crate) archon: bool,
}
impl Config {
    /// New default config
    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }
    pub fn load_from_str(data: &str) -> Self {
        let p: Self = serde_json::from_str(data).expect("Could not load config from JSON");
        p
    }
    pub fn map(&self) -> &String {
        &self.map
    }
    pub fn disable_debug(&self) -> bool {
        self.disable_debug
    }
    pub fn realtime(&self) -> bool {
        self.real_time
    }
    pub fn player1(&self) -> String {
        self.player1.clone()
    }
    pub fn player2(&self) -> String {
        self.player2.clone()
    }
    pub fn max_game_time(&self) -> u32 {
        self.max_game_time
    }
    pub fn replay_path(&self) -> String {
        self.replay_path.clone()
    }
    pub fn light_mode(&self) -> bool {
        self.light_mode
    }
    pub fn validate_race(&self) -> bool {
        self.validate_race
    }
    pub fn player1_race(&self) -> Option<String> {
        self.player1_race.clone()
    }
    pub fn player1_bot_race(&self) -> Option<Race> {
        match self.player1_race.clone() {
            Some(string) => Some(BotRace::from_str(&*string).to_race()),
            None => None,
        }
    }
    pub fn player2_race(&self) -> Option<String> {
        self.player2_race.clone()
    }
    pub fn player2_bot_race(&self) -> Option<Race> {
        match self.player2_race.clone() {
            Some(string) => Some(BotRace::from_str(&*string).to_race()),
            None => None,
        }
    }
    pub fn archon(&self) -> bool {
        self.archon
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn string_config() -> &'static str {
        "{\"Map\": \"AutomatonLE\",\
        \"MaxGameTime\":10,\
        \"Player1\":\"Bot1\",\
        \"Player2\":\"Bot2\",\
        \"ReplayPath\":\"c:\\random_path\",\
        \"MatchID\":10,\
        \"DisableDebug\":true,\
        \"MaxFrameTime\":20,\
        \"Strikes\":10,\
        \"RealTime\":false,\
        \"Visualize\":false,\
        \"ValidateRace\":true,\
        \"Player1Race\":\"random\",\
        \"Player2Race\":\"t\"}"
    }
    #[test]
    fn test_load_from_str() {
        let str_config = string_config();
        let config = Config::load_from_str(&*str_config);
        assert_eq!(config.map(), "AutomatonLE");
    }
}
