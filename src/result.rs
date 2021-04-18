use serde::{Deserialize, Serialize};
use std::collections::HashMap;


#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub(crate) struct JsonResult {
    #[serde(default, rename = "MatchID")]
    match_id: i64,
    #[serde(default, rename = "Result")]
    result: HashMap<String, String>,
    #[serde(default, rename = "GameTime")]
    game_time: u32,
    #[serde(default, rename = "GameTimeSeconds")]
    game_time_seconds: f64,
    #[serde(default, rename = "GameTimeFormatted")]
    game_time_formatted: String,
    #[serde(default, rename = "AverageFrameTime")]
    average_frame_time: HashMap<String, f32>,
    #[serde(default, rename = "Status")]
    status: String,
    #[serde(default, rename="Bots")]
    bots: HashMap<u8, String>,
    #[serde(default, rename="Map")]
    map: String,
    #[serde(default, rename="ReplayPath")]
    replay_path: String
}
impl JsonResult {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from(
        result: Option<HashMap<String, String>>,
        game_time: Option<u32>,
        game_time_seconds: Option<f64>,
        game_time_formatted: Option<String>,
        average_frame_time: Option<HashMap<String, f32>>,
        status: Option<String>,
        bots: Option<HashMap<u8, String>>,
        map: Option<String>,
        replay_path: Option<String>,
        match_id: Option<i64>
    ) -> Self {
        Self {
            result: result.unwrap_or_default(),
            game_time: game_time.unwrap_or_default(),
            game_time_seconds: game_time_seconds.unwrap_or_default(),
            game_time_formatted: game_time_formatted.unwrap_or_default(),
            average_frame_time: average_frame_time.unwrap_or_default(),
            status: status.unwrap_or_default(),
            bots: bots.unwrap_or_default(),
            map: map.unwrap_or_default(),
            replay_path: replay_path.unwrap_or_default(),
            match_id: match_id.unwrap_or_default()
        }
    }
    pub(crate) fn serialize(&self) -> String {
        serde_json::to_string(&self).expect("Could not serialize Result")
    }
}
