from json import dumps


class GameConfig:
    def __init__(self,
                 map_name=None,
                 player1=None,
                 player2=None,
                 disable_debug=True,
                 replay_name="",
                 real_time=False, max_game_time=60486):
        self.map_name = map_name
        self.player1 = player1
        self.player2 = player2
        self.disable_debug = disable_debug
        self.real_time = real_time
        self.replay_name = replay_name
        self.max_game_time = max_game_time

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
        })
