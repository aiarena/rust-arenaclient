from . import PServer
from argparse import ArgumentParser
from multiprocessing import Process


if __name__ == "__main__":
    parser = ArgumentParser()
    parser.add_argument("-a", "--address", help="Start server with address", default="127.0.0.1:8642", required=True)

    args, unknown = parser.parse_known_args()

    addr = args.address
    print(f"Starting server on {addr}")

    server = PServer(addr)
    process = Process(target=server.run)
