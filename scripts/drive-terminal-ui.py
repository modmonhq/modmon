#!/usr/bin/env python3
"""Drives the terminal UI through a pseudo-terminal and saves what it shows.

Starts a command (normally `mm`) on a pseudo-terminal, runs a list of steps
against it once the first frame is drawn, then quits the UI and prints one
JSON object:

    scripts/drive-terminal-ui.py --step 'mark=3/3 running' --step key=3 \\
        --step 'snap=services.screen' -- mm --root PROJECT

    {"first_frame_ms": 41.2, "steps": [...], "exit_code": 0, "exit_ms": 2310.4}

Steps run in order:

  mark=TEXT     wait until TEXT is on screen (--timeout seconds at most)
  gone=TEXT     wait until TEXT is no longer on screen
  key=KEYS      type KEYS; {enter} {esc} {tab} {up} {down} {left} {right}
                {pgup} {pgdn} {space} {backspace} and {ctrl-X} name keys,
                and \\r, \\x1b and the like are understood
  wait=SECONDS  keep reading the screen this long
  snap=FILE     write the screen as text to FILE
  say=TEXT      record TEXT in the report, with the time

A mark that never shows, or text that never goes, is reported with
"found": false and makes the script exit with status 1; the remaining steps
still run. The screen is a grid of --rows by --columns that replays the
output as a terminal would (cursor moves, erases, the alternate screen). At
the end the --quit keys are typed and the UI is given 10 s to exit. The
command runs with the environment of the caller. Python 3.9 or later, no
packages: it runs on a test node or in a container as it is.
"""

import argparse
import codecs
import fcntl
import json
import os
import pty
import re
import select
import signal
import struct
import sys
import termios
import time

ALTERNATE_SCREEN = "\x1b[?1049h"
SEQUENCE = re.compile(r"\x1b\[([0-9;?]*)[ -/]*([@-~])|\x1b[()][0-9A-Za-z]|\x1b[=>78]")
NAMED_KEYS = {
    "enter": "\r",
    "esc": "\x1b",
    "tab": "\t",
    "backtab": "\x1b[Z",
    "space": " ",
    "backspace": "\x7f",
    "up": "\x1b[A",
    "down": "\x1b[B",
    "right": "\x1b[C",
    "left": "\x1b[D",
    "home": "\x1b[H",
    "end": "\x1b[F",
    "pgup": "\x1b[5~",
    "pgdn": "\x1b[6~",
}


class Screen:
    """What a terminal shows after the output so far: enough of one to read
    a full-screen UI that positions its cursor and prints."""

    def __init__(self, rows, columns):
        self.rows, self.columns = rows, columns
        self.cells = [[" "] * columns for _ in range(rows)]
        self.row = self.column = 0
        self.pending = ""

    def feed(self, text):
        text = self.pending + text
        self.pending = ""
        position = 0
        while position < len(text):
            character = text[position]
            if character == "\x1b":
                match = SEQUENCE.match(text, position)
                if match is None:
                    # The rest of the sequence comes with the next read.
                    if len(text) - position < 32:
                        self.pending = text[position:]
                        return
                    position += 1
                    continue
                self.control(match.group(1) or "", match.group(2))
                position = match.end()
            elif character == "\r":
                self.column = 0
                position += 1
            elif character == "\n":
                self.row = min(self.row + 1, self.rows - 1)
                position += 1
            elif character < " ":
                position += 1
            else:
                if self.column < self.columns:
                    self.cells[self.row][self.column] = character
                self.column += 1
                position += 1

    def control(self, parameters, final):
        if final is None:
            return
        numbers = [int(part) if part.isdigit() else 0 for part in parameters.split(";")] if parameters else []
        if final in "Hf":
            row = (numbers[0] if numbers else 1) or 1
            column = (numbers[1] if len(numbers) > 1 else 1) or 1
            self.row = min(max(row - 1, 0), self.rows - 1)
            self.column = min(max(column - 1, 0), self.columns)
        elif final == "J" and (numbers or [0])[0] in (2, 3):
            self.cells = [[" "] * self.columns for _ in range(self.rows)]
        elif final == "K":
            for column in range(self.column, self.columns):
                self.cells[self.row][column] = " "
        elif final == "h" and parameters == "?1049":
            self.cells = [[" "] * self.columns for _ in range(self.rows)]
            self.row = self.column = 0

    def lines(self):
        return ["".join(row).rstrip() for row in self.cells]

    def text(self):
        return "\n".join(self.lines())


def keys_of(text):
    """The bytes to type for a key step."""
    parts = re.split(r"(\{[a-z0-9-]+\})", text)
    typed = ""
    for part in parts:
        name = part[1:-1] if part.startswith("{") and part.endswith("}") else None
        if name in NAMED_KEYS:
            typed += NAMED_KEYS[name]
        elif name and name.startswith("ctrl-") and len(name) == 6:
            typed += chr(ord(name[5].lower()) - ord("a") + 1)
        else:
            typed += codecs.decode(part, "unicode_escape")
    return typed.encode()


class Session:
    """The command on its pseudo-terminal and the screen it draws."""

    def __init__(self, command, rows, columns):
        self.started = time.monotonic()
        self.child, self.terminal = pty.fork()
        if self.child == 0:
            try:
                os.execvp(command[0], command)
            finally:
                os._exit(127)
        fcntl.ioctl(self.terminal, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
        self.screen = Screen(rows, columns)
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.seen = ""
        self.alternate = False
        self.ended = False

    def elapsed(self):
        return round((time.monotonic() - self.started) * 1000, 1)

    def pump(self, seconds):
        """Reads what the command prints for this long."""
        until = time.monotonic() + seconds
        while not self.ended:
            left = until - time.monotonic()
            if left <= 0:
                break
            readable, _, _ = select.select([self.terminal], [], [], min(left, 0.02))
            if not readable:
                continue
            try:
                chunk = os.read(self.terminal, 65536)
            except OSError:
                chunk = b""
            if not chunk:
                self.ended = True
                break
            text = self.decoder.decode(chunk)
            if self.alternate:
                self.screen.feed(text)
                continue
            self.seen += text
            if ALTERNATE_SCREEN in self.seen:
                self.screen.feed(self.seen[self.seen.index(ALTERNATE_SCREEN):])
                self.seen = ""
                self.alternate = True

    def until(self, test, limit):
        """Reads until `test` holds for the screen text, at most `limit` seconds."""
        deadline = time.monotonic() + limit
        while True:
            if test(self.screen.text()):
                return True
            if self.ended or time.monotonic() >= deadline:
                return test(self.screen.text())
            self.pump(0.03)

    def type(self, data):
        if not self.ended:
            os.write(self.terminal, data)
        self.pump(0.15)

    def finish(self, quit_keys):
        for key in keys_of(quit_keys).decode():
            if self.ended:
                break
            os.write(self.terminal, key.encode())
            self.pump(0.4)
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            finished, status = os.waitpid(self.child, os.WNOHANG)
            if finished:
                return os.waitstatus_to_exitcode(status)
            self.pump(0.1)
        os.kill(self.child, signal.SIGTERM)
        _, status = os.waitpid(self.child, 0)
        return os.waitstatus_to_exitcode(status)


def arguments():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--step", action="append", default=[], help="a step; repeatable")
    parser.add_argument("--timeout", type=float, default=30.0, help="seconds a mark or gone step waits")
    parser.add_argument("--first-frame-timeout", type=float, default=20.0)
    parser.add_argument("--columns", type=int, default=160)
    parser.add_argument("--rows", type=int, default=48)
    parser.add_argument("--quit", default="{esc}{esc}q", help="keys that leave the UI at the end")
    parser.add_argument("command", nargs="+")
    return parser.parse_args()


def main():
    options = arguments()
    session = Session(options.command, options.rows, options.columns)
    session.until(lambda text: session.alternate and text.strip(), options.first_frame_timeout)
    first_frame = session.elapsed() if session.alternate and session.screen.text().strip() else None
    steps = []
    for step in options.step:
        kind, _, value = step.partition("=")
        if kind == "mark":
            found = session.until(lambda text: value in text, options.timeout)
            steps.append({"mark": value, "found": found, "at_ms": session.elapsed()})
        elif kind == "gone":
            found = session.until(lambda text: value not in text, options.timeout)
            steps.append({"gone": value, "found": found, "at_ms": session.elapsed()})
        elif kind == "key":
            session.type(keys_of(value))
        elif kind == "wait":
            session.pump(float(value))
        elif kind == "snap":
            with open(value, "w", encoding="utf-8") as file:
                file.write(session.screen.text() + "\n")
            steps.append({"snap": os.path.basename(value), "at_ms": session.elapsed()})
        elif kind == "say":
            steps.append({"say": value, "at_ms": session.elapsed()})
        else:
            sys.exit(f"unknown step {step!r}")
    exit_code = session.finish(options.quit)
    print(json.dumps({
        "first_frame_ms": first_frame,
        "steps": steps,
        "exit_code": exit_code,
        "exit_ms": session.elapsed(),
    }))
    missing = [item for item in steps if item.get("found") is False]
    return 0 if first_frame is not None and not missing else 1


if __name__ == "__main__":
    sys.exit(main())
