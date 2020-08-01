from json import dumps


class GameConfig:
    def __init__(self,
                 map_name=None,
                 player1=None,
                 player2=None,
                 archon=False,
                 disable_debug=True,
                 replay_name="",
                 real_time=False,
                 max_game_time=60486,
                 light_mode=False,
                 validate_race=False,
                 player1_race: str = None,
                 player2_race: str = None,
                 ):
        self.map_name = map_name
        self.player1 = player1
        self.player2 = player2
        self.disable_debug = disable_debug
        self.real_time = real_time
        self.replay_name = replay_name
        self.max_game_time = max_game_time
        self.light_mode = light_mode
        self.player1_race = player1_race
        self.player2_race = player2_race
        self.archon = archon
        if validate_race and not player1_race and not player2_race:
            self.validate_race = False
        else:
            self.validate_race = validate_race

    def to_json(self):
        return dumps({
            "Map": self.map_name,
            "MaxGameTime": self.max_game_time,
            "Player1": self.player1,
            "Player2": self.player2,
            "ReplayPath": self.replay_name,
            "MatchID": 0,
            "DisableDebug": self.disable_debug,
            "RealTime": self.real_time,
            "LightMode": self.light_mode,
            "ValidateRace": self.validate_race,
            "Player1Race": self.player1_race,
            "Player2Race": self.player2_race,
            "Archon": self.archon
        })
