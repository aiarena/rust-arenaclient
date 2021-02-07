//! SC2 data and types

#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum Race {
    Protoss,
    Terran,
    Zerg,
    Random,
}
impl Race {
    pub fn from_proto(race: sc2_proto::common::Race) -> Self {
        use sc2_proto::common::Race;
        match race {
            Race::Protoss => Self::Protoss,
            Race::Terran => Self::Terran,
            Race::Zerg => Self::Zerg,
            Race::Random => Self::Random,
            Race::NoRace => panic!("NoRace not allowed"),
        }
    }

    pub fn to_proto(self) -> sc2_proto::common::Race {
        use sc2_proto::common::Race;
        match self {
            Self::Protoss => Race::Protoss,
            Self::Terran => Race::Terran,
            Self::Zerg => Race::Zerg,
            Self::Random => Race::Random,
        }
    }
}
impl Default for Race {
    fn default() -> Self {
        Race::Random
    }
}

/// Builtin AI difficulty level
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum Difficulty {
    VeryEasy,
    Easy,
    Medium,
    MediumHard,
    Hard,
    Harder,
    VeryHard,
    CheatVision,
    CheatMoney,
    CheatInsane,
}
impl Default for Difficulty {
    fn default() -> Self {
        Difficulty::Hard
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinAI {
    pub race: Race,
    pub difficulty: Difficulty,
}

/// Result of a player
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlayerResult {
    Victory,
    Defeat,
    Tie,
    Crash,
    SC2Crash,
    Timeout,
}
impl PlayerResult {
    pub fn from_proto(race: sc2_proto::sc2api::Result) -> Self {
        use sc2_proto::sc2api::Result;
        match race {
            Result::Victory => Self::Victory,
            Result::Defeat => Self::Defeat,
            Result::Tie => Self::Tie,
            Result::Undecided => panic!("Undecided result not allowed"),
        }
    }
}
impl fmt::Display for PlayerResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
