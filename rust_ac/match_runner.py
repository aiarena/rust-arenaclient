from . import Result, Server, Supervisor, GameConfig, Bot

from pathlib import Path
from asyncio import get_event_loop, gather
from typing import List, Union
import portpicker
from os import cpu_count
from atexit import register


class MatchRunner:
    def __init__(self, bot_directory, proxy_host: str = '127.0.0.1'):
        _bot_directory = Path(bot_directory)
        try:
            if not _bot_directory.exists():
                raise NotADirectoryError(f"{bot_directory} does not exist. Please check the path")
        except OSError:
            raise OSError("The directory name is incorrect. Please check the path")
        self.bot_directory = _bot_directory
        self.proxy_host = proxy_host
        self._processes = set()
        register(self._cleanup)

    async def _run_game(self, game: GameConfig, port: int, host: str = '127.0.0.1'):
        s = Server(f"{host}:{port}")
        s.run()
        self._add_to_cleanup(s)
        sup = Supervisor(f"127.0.0.1:{port}", config=game)
        bots = [Bot(game.player1, self.bot_directory), Bot(game.player2, self.bot_directory)]
        await sup.start_game()  # Sends config to proxy
        for bot in bots:
            self._add_to_cleanup(bot)
            bot.start("123", port=port)
            if await sup.wait_for_bot(timeout=400):
                continue
            else:
                bot.kill()
                return 'error'
        game_result = await sup.wait_for_result()
        for bot in bots:
            bot.kill()
        s.kill()
        return game_result

    def run_games_multiple(self, games: List[GameConfig], instances: int = int(cpu_count() / 2)) -> List[Result]:
        ports = [portpicker.pick_unused_port() for _ in range(len(games))]
        results = []
        while len(games) > 0:
            used_ports = ports[:instances]
            new_games = [self._run_game(game, port) for game, port in zip(games[:instances], used_ports)]

            del games[:instances]
            del ports[:instances]

            results += get_event_loop().run_until_complete(gather(*new_games))
            for port in used_ports:
                portpicker.return_port(port)
        return results

    def run_game(self, game: GameConfig) -> Result:
        port = portpicker.pick_unused_port()
        return get_event_loop().run_until_complete(self._run_game(game, port))

    def _add_to_cleanup(self, process: Union[Server, Bot]):
        self._processes.add(process)

    def _cleanup(self):
        print("Cleanup called")
        for process in self._processes:
            process.kill()


if __name__ == "__main__":
    m = MatchRunner(r"D:\desktop backup\aiarenaclient\aiarena-client\aiarena-test-bots")
    print(m.run_game(GameConfig('AutomatonLE', 'loser_bot', 'basic_bot')))
