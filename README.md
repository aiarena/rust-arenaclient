[![Quality Gate Status](https://sonar.m1nd.io/api/project_badges/measure?project=rust-arenaclient&metric=alert_status)](https://sonar.m1nd.io/dashboard?id=rust-arenaclient)
# rust-arenaclient

rust-arenaclient was created for [ai-arena](https://ai-arena.net/) to act as a proxy between 
bots and StarCraft II. It was originally written entirely in Python, 
but due to performance concerns, was rewritten using Rust + [pyo3](https://github.com/pyo3/pyo3).

It can be used as a stand-alone binary or as a Python library for convenience.

## Installation
You can install rust_arenaclient using:

```bash
pip install rust_arenaclient
```
or alternatively build a binary from source using 
```bash
cargo build --bin rust_ac_bin
```
## Usage

### Python
```python
from rust_ac import Server

server = Server("127.0.0.1:8642")
server.run()
```
### Binary
Currently the proxy server starts on `127.0.0.1:8642` when launched. Future updates will enable the user to specify 
host and port using command line arguments, after which this README will be updated.

## Running a game
rust_arenaclient was made for the purpose of being part of a bigger system
 to run StarCraft II
games. The system will interact with the rust_arenaclient through websockets and is known
 as a supervisor. rust_arenaclient only acts as a proxy between bots and StarCraft II. 
 The starting of bots and creating game config 
is the supervisor's job. Example of an extremely basic supervisor script in Python:
```python
import os, subprocess, asyncio, aiohttp
from rust_ac import Server

from rust_ac.match_runner import MatchRunner
from rust_ac import GameConfig


m = MatchRunner(bot_directory=r"D:\desktop backup\aiarenaclient\aiarena-client\aiarena-test-bots")
result = m.run_game(game=GameConfig('AutomatonLE', 'loser_bot', 'MicroMachine'))  # One game

games = [GameConfig('AutomatonLE', 'loser_bot', 'basic_bot') for _ in range(20)]

results = m.run_games_multiple(games=games, instances=3)  # Multiple games - Run 3 games at a time
```

## Contributing
Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

Please make sure to update tests as appropriate.

## License
[GNU GPLv3](https://choosealicense.com/licenses/gpl-3.0/)