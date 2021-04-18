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
impl Difficulty {
    pub fn to_proto(self) -> sc2_proto::sc2api::Difficulty {
        use sc2_proto::sc2api::Difficulty;
        match self {
            Self::VeryEasy => Difficulty::VeryEasy,
            Self::Easy => Difficulty::Easy,
            Self::Medium => Difficulty::Medium,
            Self::MediumHard => Difficulty::MediumHard,
            Self::Hard => Difficulty::Hard,
            Self::Harder => Difficulty::Harder,
            Self::VeryHard => Difficulty::VeryHard,
            Self::CheatVision => Difficulty::CheatVision,
            Self::CheatMoney => Difficulty::CheatMoney,
            Self::CheatInsane => Difficulty::CheatInsane,
        }
    }
}
impl Default for Difficulty {
    fn default() -> Self {
        Difficulty::Hard
    }
}

#[allow(clippy::upper_case_acronyms)]
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
    #[allow(clippy::upper_case_acronyms)]
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

    pub fn to_proto(self) -> sc2_proto::sc2api::Result {
        use sc2_proto::sc2api::Result;
        match self {
            Self::Victory => Result::Victory,
            Self::Defeat => Result::Defeat,
            Self::Tie => Result::Tie,
            Self::Crash => Result::Defeat,
            Self::Timeout => Result::Defeat,
            Self::SC2Crash => Result::Undecided,
        }
    }
}
impl fmt::Display for PlayerResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
