from . import PServer
from multiprocessing import Process


class Server:
    def __init__(self, ip_addr: str):
        self.server = PServer(ip_addr)
        self.process: Process = None

    def run(self):
        self.process = Process(target=self.server.run)
        self.process.start()

    def kill(self):
        self.process.kill()
