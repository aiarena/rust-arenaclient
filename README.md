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

BOTS_DIRECTORY = "c:/location/of/bots"
BOTS = ["names of", "bots"] # Only two bots at a time
HOST = "127.0.0.1"
PORT = "8642"

def start_bot(bot_name):
    bot_path = os.path.join(BOTS_DIRECTORY, bot_name)
    bot_file = "run.py" # The bot's start file
    cmd_line = [
        "python",
        bot_file,
        "--GamePort",
        PORT,
        "--StartPort",
        PORT,
        "--LadderServer",
        HOST
    ] 

    process = subprocess.Popen(
        " ".join(cmd_line),
        cwd=(str(bot_path)),
        shell=False
    )
    return process


async def main():
    # Supervisor
    session = aiohttp.ClientSession()
    # Needs "supervisor" header so the proxy knows it's not a bot
    ws = await session.ws_connect(f"ws://{HOST}:{PORT}/sc2api", headers={"supervisor":"True"})
    # Sends the handler config after connecting to proxy
    await ws.send_json({
                "Map": "AbiogenesisLE", # Map to play on 
                "MaxGameTime": 60846, # Max time a handler can run before result changes to tie. 
                                      # Measured in game_loops, which are handler seconds / 22.4
                "Player1": "BasicBot", # Bot 1 name
                "Player2": "LoserBot", # Bot 2 name
                "ReplayPath": r"c:/data/something.SC2Replay", # Path to save replay
                "MatchID": 123, # Used internally for ai-arena. Can be left out
                "DisableDebug": True, # Whether to allow debug commands or filter them out
                "MaxFrameTime": 1000, # Max time in ms a bot can take on one step
                "Strikes": 10, # How many times a bot can exceed MaxFrameTime before being kicked
                "RealTime": False, # Run handler in realtime
                "Visualize": False # Unused currently
            }
        )

    # bots
    b = []
    for bot in BOTS:
        b.append(start_bot(bot))
        msg = await ws.receive() # Waits for confirmation that bot connected
        continue
    
    result = await ws.receive() # Receives result from proxy after handler finishes

if __name__ == "__main__":
    server = Server(f"{HOST}:{PORT}")
    server.run()
    asyncio.get_event_loop().run_until_complete(main())
```

## Contributing
Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

Please make sure to update tests as appropriate.

## License
[GNU GPLv3](https://choosealicense.com/licenses/gpl-3.0/)