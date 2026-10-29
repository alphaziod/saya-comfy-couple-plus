#!/usr/bin/env python3
"""Saya Studio Native Messaging host (`com.saya.studio`).

Reads the Civitai token from the application data folder and returns it
only to the extension authorised by the host manifest. The token is
never written to stdout outside a JSON frame nor logged.
"""
from __future__ import annotations

import json
import os
import struct
import sys
from pathlib import Path

HOST_NAME = "com.saya.studio"
MAX_MESSAGE = 1024 * 1024


def data_dir() -> Path:
    override = os.environ.get("SAYA_CIVITAI_TOKEN_FILE", "").strip()
    if override:
        return Path(override)
    base = os.environ.get("XDG_DATA_HOME", "").strip()
    root = Path(base) if base else Path.home() / ".local" / "share"
    return root / "studio.saya.desktop" / "civitai-token"


def read_message(stream) -> dict | None:
    header = stream.read(4)
    if len(header) != 4:
        return None
    (length,) = struct.unpack("<I", header)
    if length == 0 or length > MAX_MESSAGE:
        raise ValueError("taille de message invalide")
    body = stream.read(length)
    if len(body) != length:
        return None
    message = json.loads(body.decode("utf-8"))
    if not isinstance(message, dict):
        raise ValueError("message JSON invalide")
    return message


def write_message(stream, payload: dict) -> None:
    data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    stream.write(struct.pack("<I", len(data)))
    stream.write(data)
    stream.flush()


def read_token() -> str:
    try:
        return data_dir().read_text(encoding="utf-8").strip()
    except OSError:
        return ""


def handle(message: dict) -> dict:
    kind = str(message.get("type") or message.get("action") or "").strip()
    token = read_token()
    if kind in ("status", "ping"):
        return {"ok": True, "host": HOST_NAME, "tokenAvailable": bool(token)}
    if kind in ("get_token", "token"):
        if not token:
            return {"ok": False, "error": "jeton Saya Studio introuvable"}
        return {"ok": True, "token": token, "host": HOST_NAME}
    return {"ok": False, "error": "action inconnue"}


def main() -> int:
    stream_in = sys.stdin.buffer
    stream_out = sys.stdout.buffer
    while True:
        try:
            message = read_message(stream_in)
        except (ValueError, json.JSONDecodeError):
            return 1
        if message is None:
            return 0
        try:
            write_message(stream_out, handle(message))
        except BrokenPipeError:
            return 0


if __name__ == "__main__":
    raise SystemExit(main())
