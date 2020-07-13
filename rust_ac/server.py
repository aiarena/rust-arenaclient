from . import PServer
from multiprocessing import Process
from .supervisor import Supervisor


class Server:
    """
    Set up and run the Proxy server.
    """
    def __init__(self, ip_addr: str):
        self.ip_address: str = ip_addr
        self._server = PServer(ip_addr)
        self.process: Process = ...

    def run(self):
        self.process = Process(target=self._server.run)
        self.process.daemon = True
        self.process.start()

    def kill(self):
        self.process.kill()

    async def create_supervisor(self) -> Supervisor:
        supervisor = Supervisor(self.ip_address)
        await supervisor.connect()
        return supervisor

