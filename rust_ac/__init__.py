from .rust_ac import PServer
from .server import Server
from .supervisor import Supervisor
from .game_config import GameConfig
from .result import Result
from .bot import Bot


def set_debug_level(level: int):
    """
    Sets the debugging level for the Rust Library.
    0 = trace
    1 = debug
    2 = info
    3 = warning
    4 = error
    5 = critical
    :return:
    """
    import os
    if not 0 <= level <= 5:
        return
    elif level == 0:
        level = 'trace'
    elif level == 1:
        level = 'debug'
    elif level == 2:
        level = 'info'
    elif level == 3:
        level = 'warning'
    elif level == 4:
        level = 'error'
    elif level == 5:
        level = 'critical'

    os.environ["RUST_LOG"] = level


set_debug_level(2)
