"""Local maintenance over the existing Orbic shell (installer util orbic-start-telnet).

Python standard library only. Uploads refuse to replace an existing remote file.
Credentials are deliberately not handled or saved by this utility.
"""
import argparse
import base64
import hashlib
from pathlib import Path
import re
import shlex
import socket
import sys
import threading
import time
import uuid


def run(host, command, timeout=45):
    marker = "RH_" + uuid.uuid4().hex
    start, end = (marker + "_START").encode(), (marker + "_END:").encode()
    wire = f"echo {marker}_START; ( {command} ); result=$?; echo {marker}_END:$result\n"
    with socket.create_connection((host, 24), timeout=5) as connection:
        connection.settimeout(timeout)
        connection.sendall(wire.encode())
        data = bytearray()
        while True:
            chunk = connection.recv(65536)
            if not chunk:
                raise RuntimeError("Maintenance shell disconnected before completion")
            data.extend(chunk)
            match = re.search(re.escape(end) + rb"(\d+)\r?\n", data)
            if match:
                begin = data.find(start + b"\n")
                if begin < 0:
                    begin = data.find(start + b"\r\n")
                if begin < 0:
                    raise RuntimeError("Missing command-start marker")
                output = bytes(data[begin + len(start):match.start()]).lstrip(b"\r\n")
                code = int(match.group(1))
                if code:
                    raise RuntimeError(f"Device command failed ({code}): {output.decode(errors='replace')}")
                return output


def put(host, source, destination):
    data = Path(source).read_bytes()
    quoted = shlex.quote(destination)
    run(host, f"test ! -e {quoted}")
    errors = []

    def receiver():
        try:
            run(host, f"umask 077; nc -l -p 8081 > {quoted}", timeout=90)
        except Exception as error:
            errors.append(error)

    thread = threading.Thread(target=receiver)
    thread.start()
    connection = None
    try:
        for _ in range(20):
            time.sleep(0.2)
            try:
                connection = socket.create_connection((host, 8081), timeout=2)
                break
            except OSError:
                if errors:
                    raise errors[0]
        if connection is None:
            raise RuntimeError("Device upload receiver unavailable")
        with connection:
            connection.settimeout(90)
            connection.sendall(data)
            time.sleep(1)
            connection.shutdown(socket.SHUT_WR)
    finally:
        thread.join(timeout=95)
    if thread.is_alive():
        raise RuntimeError("Upload receiver did not complete")
    if errors:
        raise errors[0]
    expected = hashlib.sha256(data).hexdigest()
    observed = run(host, f"sha256sum {quoted}").decode().split()[0]
    if observed != expected:
        raise RuntimeError("Upload SHA-256 mismatch; staged file retained for inspection")
    print(f"Verified {len(data)} bytes, SHA-256 {expected}")


def main():
    # Windows legacy consoles cannot encode every character in device logs.
    sys.stdout.reconfigure(errors="backslashreplace")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="192.168.1.1")
    sub = parser.add_subparsers(dest="action", required=True)
    command = sub.add_parser("exec")
    command.add_argument("command")
    command.add_argument("--output", type=Path)
    upload = sub.add_parser("put")
    upload.add_argument("source", type=Path)
    upload.add_argument("destination")
    download = sub.add_parser("get")
    download.add_argument("source")
    download.add_argument("destination", type=Path)
    args = parser.parse_args()
    if args.action == "put":
        put(args.host, args.source, args.destination)
    elif args.action == "get":
        data = run(args.host, "base64 " + shlex.quote(args.source), timeout=90)
        decoded = base64.b64decode(data)
        with args.destination.open("xb") as file:
            file.write(decoded)
        print(f"Saved {len(decoded)} bytes; SHA-256 {hashlib.sha256(decoded).hexdigest()}")
    else:
        data = run(args.host, args.command)
        if args.output:
            with args.output.open("xb") as file:
                file.write(data)
        else:
            print(data.decode(errors="replace"), end="")


if __name__ == "__main__":
    main()
