from pathlib import Path
from typing import Optional
from os import path
import os
import platform
import sys
import re
from subprocess import Popen, STDOUT, call
import subprocess


class BotTypeError(Exception):
    pass


class BotFolderError(Exception):
    pass


class BotStartError(Exception):
    pass


def convert_to_wsl(_path: Path) -> str:
    return re.sub(r'([A-Za-z])(:)', lambda x: '/mnt/' + x.group(1).lower(), _path.as_posix()).replace(' ', r'\ ')


def wsl_installed():
    try:
        return call('wsl exec echo ""') == 0
    except FileNotFoundError:
        return False


class Bot:
    def __init__(self, name: str, directory: Path, bot_type: Optional[str] = None):
        self.name = name
        self.directory = directory
        if bot_type:
            self.type = bot_type
        else:
            self.type = self.deduce_bot_type()
        self.process: Optional[Popen] = None

    @property
    def type_mapping(self):
        bot_name = self.name
        return {
            'run.py': 'Python',
            f'{bot_name}.exe': 'cppwin32',
            f"{bot_name}": 'cpplinux',
            f"{bot_name}.dll": "dotnetcore",
            f"{bot_name}.jar": "java"
        }

    @property
    def type_mapping_swopped(self):
        return {value: key for key, value in self.type_mapping.items()}

    def deduce_bot_type(self) -> str:
        bot_name = self.name
        type_mapping = {
            'run.py': 'Python',
            f'{bot_name}.exe': 'cppwin32',
            f"{bot_name}": 'cpplinux',
            f"{bot_name}.dll": "dotnetcore",
            f"{bot_name}.jar": "java"
        }
        bot_folder = self.directory.joinpath(self.name)
        if not bot_folder.exists():
            raise BotFolderError(f"{bot_folder} does not exist. Please check the path.")
        for file in bot_folder.iterdir():
            if file.is_file() and file.name in type_mapping:
                return type_mapping.get(file.name)
        else:
            raise BotTypeError(f"Could not automatically deduce bot type for {bot_name}. "
                               f"Please specify type in the GameConfig folder")

    def kill(self):
        self.process.kill()

    def start(self, opponent_id: str, port: int, host: str = '127.0.0.1'):
        bot_type = self.type
        bot_folder = self.directory.joinpath(self.name)
        bot_file = self.type_mapping_swopped.get(bot_type)

        cmd_line = [
            bot_file,
            "--GamePort",
            str(port),
            "--StartPort",
            str(port),
            "--LadderServer",
            host,
            "--OpponentId",
            str(opponent_id),
        ]
        if bot_type.lower() == "python":
            cmd_line.insert(0, sys.executable)
        elif bot_type.lower() == "cppwin32" and platform.system() == "Linux":
            cmd_line.pop(0)
            cmd_line.insert(0, path.join(bot_folder.as_posix(), bot_file))
            cmd_line.insert(0, "wine")
        elif bot_type.lower() == "dotnetcore":
            cmd_line.insert(0, "dotnet")
        elif (bot_type.lower() == "cpplinux" and platform.system() == "Linux") \
                or (bot_type.lower() == "cppwin32" and platform.system() == "Windows"):
            cmd_line.pop(0)
            cmd_line.insert(0, path.join(bot_folder.as_posix(), bot_file))
        elif bot_type.lower() == "java":
            cmd_line.insert(0, "java")
            cmd_line.insert(1, "-jar")
        elif bot_type.lower() == "cpplinux" and platform.system() == "Windows" and wsl_installed():
            raise BotTypeError("Launching Linux bots in WSL is not yet supported")
            # print(bot_folder.joinpath(bot_file))
            # cmd_line.pop(0)
            # cmd_line.insert(0, convert_to_wsl(bot_folder.joinpath(bot_file)))
            # cmd_line.insert(0, 'wsl')
        else:
            raise BotTypeError(f"Could not find a way to launch bot {self.name} "
                               f"with type {self.type} on {platform.system()}")

        try:
            is_linux = platform.system() == "Linux"
            with open(bot_folder.joinpath("data").joinpath("stderr.log").as_posix(), "w+") as out:
                process = Popen(
                    " ".join(cmd_line),
                    stdout=out,
                    stderr=STDOUT,
                    cwd=(str(bot_folder.as_posix())),
                    shell=True if is_linux else False,
                    preexec_fn=os.setpgrp if is_linux else None,
                    creationflags=None if is_linux else subprocess.CREATE_NEW_PROCESS_GROUP,
                )
                self.process = process

        except Exception as exception:
            raise BotStartError(exception)


if __name__ == "__main__":
    b = Bot("loser_bot", Path("D:\\desktop backup\\aiarenaclient\\aiarena-client\\aiarena-test-bots"))
    b.start('123', 123)
    b.kill()
