use crate::sc2::Race;

#[derive(PartialOrd, PartialEq, Debug)]
pub enum BotRace {
    NoRace = 0,
    Terran = 1,
    Zerg = 2,
    Protoss = 3,
    Random = 4,
}
impl BotRace {
    pub fn from_str(race: &str) -> Self {
        match &race.to_lowercase()[..] {
            "p" | "protoss" | "race.protoss" | "3" => Self::Protoss,
            "t" | "terran" | "race.terran" | "1" => Self::Terran,
            "r" | "random" | "race.random" | "4" => Self::Random,
            "z" | "zerg" | "race.zerg" | "2" => Self::Zerg,
            _ => Self::NoRace,
        }
    }
    pub fn to_race(&self) -> Race {
        match self {
            BotRace::Terran => Race::Terran,
            BotRace::Zerg => Race::Zerg,
            BotRace::Protoss => Race::Protoss,
            BotRace::Random => Race::Random,
            BotRace::NoRace => Race::Random,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    pub fn test_from_str_random() {
        let race = "R";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Random);
        let race = "Random";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Random);
        let race = "Race.Random";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Random);
        let race = "4";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Random);
    }
    #[test]
    pub fn test_from_str_protoss() {
        let race = "P";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Protoss);
        let race = "Protoss";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Protoss);
        let race = "Race.Protoss";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Protoss);
        let race = "3";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Protoss);
    }
    #[test]
    pub fn test_from_str_terran() {
        let race = "T";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Terran);
        let race = "Terran";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Terran);
        let race = "Race.Terran";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Terran);
        let race = "1";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Terran);
    }
    #[test]
    pub fn test_from_str_zerg() {
        let race = "Z";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Zerg);
        let race = "Zerg";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Zerg);
        let race = "Race.Zerg";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Zerg);
        let race = "2";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::Zerg);
    }
    #[test]
    pub fn test_from_str_unknown() {
        let race = "whatever";
        let bot_race = BotRace::from_str(race);
        assert_eq!(bot_race, BotRace::NoRace);
    }
    #[test]
    pub fn test_conversion() {
        assert_eq!(Race::Random, BotRace::Random.to_race());
        assert_eq!(Race::Random, BotRace::NoRace.to_race());
        assert_eq!(Race::Terran, BotRace::Terran.to_race());
        assert_eq!(Race::Zerg, BotRace::Zerg.to_race());
        assert_eq!(Race::Protoss, BotRace::Protoss.to_race());
    }
}
