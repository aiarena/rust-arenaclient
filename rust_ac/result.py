import os

from .game_config import GameConfig


class Result:
    def __init__(self, match_config, cfg=None):
        if isinstance(match_config, GameConfig):
            self.match_id = None
            self.bot1 = match_config.player1
            self.bot2 = match_config.player2
            self.map = match_config.map_name
            self.replay_path = match_config.replay_name
        else:
            self.match_id = match_config.id
            self.bot1 = match_config.bot1.name
            self.bot2 = match_config.bot2.name
            self.map = match_config.map_name
            self._config = cfg
            self.replay_path = None

        self.result = None
        self.game_time = 0
        self.game_time_formatted = None
        self.time_stamp = None
        self.bot1_avg_frame = 0
        self.bot2_avg_frame = 0
        self.winner = None

    def __repr__(self):
        return f"""
        Result={self.result}
        Winner={self.winner}
        GameTime={self.game_time}
        Bot1AvgStepTime={self.bot1_avg_frame}
        Bot2AvgStepTime={self.bot2_avg_frame}
        """

    def to_json(self):
        """
        Convert Result object to JSON
        """
        return {
            "MatchID": self.match_id,
            "Bot1": self.bot1,
            "Bot2": self.bot2,
            "Winner": self.winner,
            "Map": self.map,
            "Result": self.result if self.result else "Error",
            "GameTime": self.game_time,
            "GameTimeFormatted": self.game_time_formatted,
            "TimeStamp": self.time_stamp,
            "Bot1AvgFrame": self.bot1_avg_frame,
            "Bot2AvgFrame": self.bot2_avg_frame,
            'ReplayPath': self.replay_path,
        }

    def has_result(self):
        """
        Checks if there is a result already
        """
        return self.result is not None

    def parse_result(self, result=None, error=False):
        """
        Parse result messages into  object
        """
        if not result and not error:
            raise NameError("Result.parse_result requires either a result or an error boolean")
        if error:
            result = {"Result": "Error"}
        if result.get("Result", None):

            temp_results = result['Result']
            if temp_results == "Error":
                self.result = "Error"
                return

            if temp_results[self.bot1] == "SC2Crash" or temp_results[self.bot2] == "SC2Crash":
                self.result = "Error"
                return

            elif temp_results[self.bot1] == "Crash":
                self.result = "Player1Crash"
                self.winner = self.bot2

            elif temp_results[self.bot2] == "Crash":
                self.result = "Player2Crash"
                self.winner = self.bot1

            elif temp_results[self.bot1] == "Timeout":
                self.result = "Player1TimeOut"
                self.winner = self.bot2

            elif temp_results[self.bot2] == "Timeout":
                self.result = "Player2TimeOut"
                self.winner = self.bot1

            elif temp_results[self.bot1] == "Victory":
                self.result = "Player1Win"
                self.winner = self.bot1

            elif temp_results[self.bot1] == "Defeat":
                self.result = "Player2Win"
                self.winner = self.bot2

            elif temp_results[self.bot1] == "Tie":
                self.result = "Tie"
                self.winner = "Tie"

            elif temp_results[self.bot2] == "Tie":
                self.result = "Tie"
                self.winner = "Tie"

            elif temp_results[self.bot1] == 'InitializationError':
                self.result = "InitializationError"

            elif temp_results[self.bot2] == 'InitializationError':
                self.result = "InitializationError"

        if result.get("GameTime", None):
            self.game_time = result["GameTime"]
            self.game_time_formatted = result["GameTimeFormatted"]

        if result.get("AverageFrameTime", None):
            self.bot1_avg_frame = result['AverageFrameTime'].get(self.bot1, 0)
            self.bot2_avg_frame = result['AverageFrameTime'].get(self.bot2, 0)

        if result.get("TimeStamp", None):
            self.time_stamp = result["TimeStamp"]

        if self.replay_path is None:
            self.replay_path = os.path.join(
                self._config.REPLAYS_DIRECTORY, f'{self.match_id}_{self.bot1}_vs_{self.bot2}.SC2Replay')
