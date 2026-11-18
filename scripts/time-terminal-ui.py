#!/usr/bin/env python3
"""Times the terminal UI through a pseudo-terminal.

Starts a command (normally `mm`) on a pseudo-terminal, records when the first
frame is drawn and when each given text first appears on screen, then quits
the UI with `q` and prints one JSON object:

    scripts/time-terminal-ui.py --mark '3/3 running' -- target/release/mm --root PROJECT

    {"first_frame_ms": 41.2, "marks": {"3/3 running": 612.9}, "exit_code": 0, ...}

The first frame is the first output after the UI switches to the alternate
screen. Marks are looked for on the screen as a terminal would show it (the
output is replayed onto a grid of --rows by --columns), one row at a time. A
mark that never appears within --timeout is reported as null and the script
exits with status 1. `--keys` sends keys once the first frame is up, with the
time in seconds to wait before each (`--keys 0.2:3 --keys 1:q`).
`--screen FILE` writes the screen as it was when the run ended. The command
runs with the environment of the caller.
"""

import argparse
import codecs
import fcntl
import importlib.util
import json
import os
import pty
import select
import struct
import sys
import termios
import time

# The screen model is the one of drive-terminal-ui.py next to this file,
# loaded without leaving a bytecode cache beside it.
sys.dont_write_bytecode = True
_driver = importlib.util.spec_from_file_location(
    "drive_terminal_ui", os.path.join(os.path.dirname(os.path.abspath(__file__)), "drive-terminal-ui.py")
)
_module = importlib.util.module_from_spec(_driver)
_driver.loader.exec_module(_module)
ALTERNATE_SCREEN = _module.ALTERNATE_SCREEN
Screen = _module.Screen


def arguments():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--mark", action="append", default=[], help="text to wait for; repeatable")
    parser.add_argument("--keys", action="append", default=[], help="SECONDS:KEYS to send after the first frame; repeatable")
    parser.add_argument("--timeout", type=float, default=60.0, help="seconds to wait for the marks")
    parser.add_argument("--linger", type=float, default=0.0, help="seconds to keep the UI open after the last mark")
    parser.add_argument("--columns", type=int, default=140)
    parser.add_argument("--rows", type=int, default=40)
    parser.add_argument("--screen", help="write the text the terminal showed to this file")
    parser.add_argument("command", nargs="+")
    return parser.parse_args()


def main():
    options = arguments()
    keys = []
    for item in options.keys:
        delay, _, text = item.partition(":")
        keys.append((float(delay), text.encode()))
    started = time.monotonic()
    child, terminal = pty.fork()
    if child == 0:
        os.execvp(options.command[0], options.command)
    fcntl.ioctl(terminal, termios.TIOCSWINSZ, struct.pack("HHHH", options.rows, options.columns, 0, 0))

    def elapsed():
        return round((time.monotonic() - started) * 1000, 1)

    screen = Screen(options.rows, options.columns)
    decoder = codecs.getincrementaldecoder("utf-8")("replace")
    seen = ""
    received = 0
    first_frame = None
    marks = {mark: None for mark in options.mark}
    next_key = None
    done_at = None
    quit_sent = False
    exit_code = None
    while True:
        now = time.monotonic()
        if now - started > options.timeout and not quit_sent:
            done_at = now
        if first_frame is not None and keys and next_key is None:
            next_key = now + keys[0][0]
        if next_key is not None and now >= next_key:
            os.write(terminal, keys.pop(0)[1])
            next_key = None
        if done_at is None and first_frame is not None and not keys and all(value is not None for value in marks.values()):
            done_at = now + options.linger
        if done_at is not None and now >= done_at and not quit_sent:
            os.write(terminal, b"q")
            quit_sent = True
        readable, _, _ = select.select([terminal], [], [], 0.01)
        if readable:
            try:
                chunk = os.read(terminal, 65536)
            except OSError:
                chunk = b""
            if not chunk:
                break
            received += len(chunk)
            text = decoder.decode(chunk)
            if first_frame is None:
                seen += text
                if ALTERNATE_SCREEN in seen:
                    screen.feed(seen[seen.index(ALTERNATE_SCREEN):])
                    seen = ""
                    first_frame = -1.0
            else:
                screen.feed(text)
            shown = screen.text()
            if first_frame == -1.0 and shown.strip():
                first_frame = elapsed()
            for mark, at in marks.items():
                if at is None and mark in shown:
                    marks[mark] = elapsed()
        finished, status = os.waitpid(child, os.WNOHANG)
        if finished:
            exit_code = os.waitstatus_to_exitcode(status)
            break
        if quit_sent and now - done_at > 10:
            os.kill(child, 15)
    if exit_code is None:
        _, status = os.waitpid(child, 0)
        exit_code = os.waitstatus_to_exitcode(status)
    if first_frame == -1.0:
        first_frame = None
    if options.screen:
        with open(options.screen, "w", encoding="utf-8") as file:
            file.write(screen.text() + "\n")
    print(json.dumps({
        "first_frame_ms": first_frame,
        "marks": marks,
        "exit_ms": elapsed(),
        "exit_code": exit_code,
        "bytes": received,
    }))
    return 0 if first_frame is not None and all(value is not None for value in marks.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
