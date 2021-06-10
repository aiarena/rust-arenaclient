import asyncio
from .game_config import GameConfig
from typing import Optional

from aiohttp import ClientSession, WSMsgType, ClientConnectorError
from .result import Result
from datetime import datetime


def valid_msg(msg):
    """
    Looks for keywords in the message so that the result can be parsed.
    @param msg:
    @return:
    """
    if 'Result' in msg:
        return True
    elif 'GameTime' in msg:
        return True
    elif 'AverageFrameTime' in msg:
        return True
    else:
        return False


def complete(msg):
    """
    Checks if msg status is complete.
    """
    return msg.get("Status", None) == "Complete"


class Supervisor:
    def __init__(self, ip_addr: str, config: Optional[GameConfig] = None):
        self.ip_address: str = ip_addr
        self._websocket = None
        self._session = None
        if not config:
            self._config: GameConfig = GameConfig()
        else:
            self._config: GameConfig = config

    async def connect(self):
        """
        Connects to address with headers
        """
        ws, session = None, None
        headers = {"Supervisor": "true"}
        addr = self._parse_url()
        for i in range(60):
            await asyncio.sleep(1)
            try:
                session = ClientSession()
                print(addr)
                ws = await session.ws_connect(addr, headers=headers)
                self._websocket = ws
                self._session = session
                return
            except ClientConnectorError:
                await session.close()
                if i > 15:
                    return None, None

    def _parse_url(self) -> str:
        addr = self.ip_address.replace("/sc2api", "")
        return "ws://" + addr + '/sc2api'

    def set_config(self, config: GameConfig):
        self._config = config

    async def _send_config(self):
        await self._websocket.send_str(self._config.to_json())

    async def wait_for_bot(self, timeout: int = 40) -> bool:
        try:
            msg = await self._websocket.receive(timeout)
            if msg.json().get("Bot", None) == "Connected":
                return True
            else:
                await self._cleanup()
                return False
        except asyncio.TimeoutError:
            await self._cleanup()
            return False

    async def start_game(self):
        await self.connect()
        if not self._websocket or not self._session:
            raise ConnectionError("Please call .connect() before starting game")

        msg = await self._websocket.receive()
        if msg.type == WSMsgType.CLOSED:
            raise ConnectionError("Server sent a CLOSED message")
        if msg.json().get("Status") == "Connected":
            print("Connected to proxy.")
        await self._send_config()

        msg = await self._websocket.receive()

        if msg.type == WSMsgType.CLOSED:
            raise ConnectionError("Server sent a CLOSED message")
        if msg.json().get("Config") == "Received":
            print("Config successfully sent. Bots can be started")

    async def _wait_for_result(self) -> Result:
        result = Result(self._config)
        async for msg in self._websocket:
            if msg.type == WSMsgType.CLOSED:
                if not result.has_result():
                    result.parse_result(error=True)
                    return result
            msg = msg.json()

            if valid_msg(msg):
                result.parse_result(msg)

            if 'Error' in msg:
                if not result.has_result():
                    result.parse_result(error=True)
                    return result

            if complete(msg):
                result.parse_result({"TimeStamp": datetime.utcnow().strftime("%d-%m-%Y %H-%M-%SUTC")})

            await self._websocket.send_str("Received")
        if not result.has_result():
            result.parse_result(error=True)
        return result

    async def _cleanup(self):
        await self._websocket.close()
        await self._session.close()

    async def wait_for_result(self) -> Result:
        result = await self._wait_for_result()
        await self._cleanup()
        return result

    async def reset(self):
        await self._websocket.send_str("Reset")
        _ = await self._websocket.receive()  # Receive confirmation













