#!/usr/bin/env python3
"""A stand-in for systemd's file descriptor store, for scripts/live-cluster.sh.

The test nodes run no systemd, so this runs mmd the way mmd.service does as
far as kept sockets go: with NOTIFY_SOCKET set, it keeps every descriptor
mmd sends with FDSTORE=1 (one copy of each socket, named by FDNAME=), drops
the ones named by FDSTOREREMOVE=1, and on SIGHUP stops mmd (SIGTERM), waits
--gap seconds, and starts it again with the kept descriptors from 3 on and
LISTEN_FDS, LISTEN_PID, and LISTEN_FDNAMES set, as systemd does for
`systemctl restart`. When mmd exits by itself, the kept sockets close and
this exits too, as `systemctl stop` leaves a host.

Usage: live-fd-store.py [--gap SECONDS] NOTIFY_SOCKET STATUS_FILE COMMAND...
STATUS_FILE gets the names of the kept descriptors as JSON after each change.
"""

import fcntl
import json
import os
import select
import signal
import socket
import subprocess
import sys
import time


def main():
    arguments = sys.argv[1:]
    gap = 0.0
    if arguments[:1] == ["--gap"]:
        gap = float(arguments[1])
        arguments = arguments[2:]
    notify, status, command = arguments[0], arguments[1], arguments[2:]
    if os.path.exists(notify):
        os.unlink(notify)
    listener = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
    listener.bind(notify)
    kept = []  # (name, descriptor, identity), in the order they arrived
    wake_read, wake_write = os.pipe()
    os.set_blocking(wake_write, False)
    signal.set_wakeup_fd(wake_write)
    signal.signal(signal.SIGHUP, lambda *_: None)
    signal.signal(signal.SIGTERM, lambda *_: None)

    def write_status():
        names = [name for name, _, _ in kept]
        with open(status + ".new", "w", encoding="utf-8") as out:
            json.dump({"kept": len(kept), "names": names}, out)
        os.replace(status + ".new", status)

    def start():
        descriptors = [descriptor for _, descriptor, _ in kept]
        names = ":".join(name for name, _, _ in kept)
        environment = dict(os.environ, NOTIFY_SOCKET=notify)
        environment.pop("LISTEN_FDS", None)
        if descriptors:
            environment.update(LISTEN_FDS=str(len(descriptors)), LISTEN_FDNAMES=names)

        def place():
            # Out of the way first, then at 3, 4, ... in order.
            high = [fcntl.fcntl(descriptor, fcntl.F_DUPFD, 1000) for descriptor in descriptors]
            for offset, descriptor in enumerate(high):
                os.dup2(descriptor, 3 + offset)

        wrapped = ["sh", "-c", 'LISTEN_PID=$$ exec "$@"', "sh"] if descriptors else []
        return subprocess.Popen(
            wrapped + command,
            env=environment,
            preexec_fn=place,
            pass_fds=list(range(3, 3 + len(descriptors))),
        )

    def receive():
        message, descriptors, _, _ = socket.recv_fds(listener, 65536, 64)
        fields = dict(
            line.split("=", 1) for line in message.decode(errors="replace").splitlines() if "=" in line
        )
        changed = False
        if fields.get("FDSTOREREMOVE") == "1":
            name = fields.get("FDNAME", "stored")
            for entry in [entry for entry in kept if entry[0] == name]:
                os.close(entry[1])
                kept.remove(entry)
                changed = True
        for descriptor in descriptors:
            stat = os.fstat(descriptor)
            identity = (stat.st_dev, stat.st_ino)
            if fields.get("FDSTORE") != "1" or any(entry[2] == identity for entry in kept):
                os.close(descriptor)
                continue
            kept.append((fields.get("FDNAME", "stored"), descriptor, identity))
            changed = True
        if changed:
            write_status()

    write_status()
    child = start()
    while True:
        ready, _, _ = select.select([listener, wake_read], [], [], 0.2)
        if listener in ready:
            receive()
        if wake_read in ready:
            signals = os.read(wake_read, 64)
            if signal.SIGTERM in signals:
                child.terminate()
                child.wait()
                return 0
            if signal.SIGHUP in signals:
                child.terminate()
                child.wait()
                # What the old one sent before it ended is still queued.
                while select.select([listener], [], [], 0)[0]:
                    receive()
                time.sleep(gap)
                child = start()
                continue
        code = child.poll()
        if code is not None:
            return code


if __name__ == "__main__":
    sys.exit(main())
