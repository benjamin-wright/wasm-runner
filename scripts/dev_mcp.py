#!/usr/bin/env python3
"""Small stdio MCP server for this checkout's fixed development commands."""

import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parent.parent
COMMANDS = {
    "cargo_fmt": ("cargo", "fmt", "--all"),
    "cargo_deps": ("cargo", "fetch", "--locked"),
    "cargo_build": ("make", "build"),
    "cargo_clippy": (
        "cargo", "clippy", "--locked", "--workspace", "--all-targets",
        "--", "-D", "warnings",
    ),
    "cargo_test": ("cargo", "test", "--locked", "--workspace"),
    "cargo_check": ("make", "check"),
}


def call_tool(params):
    name = params.get("name")
    if not isinstance(name, str) or name not in COMMANDS:
        return {"error": {"code": -32602, "message": "Unknown tool"}}
    if params.get("arguments", {}) != {}:
        return {"error": {"code": -32602, "message": "Tools take no arguments"}}
    command = COMMANDS[name]
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
        output = f"{' '.join(command)} exited {completed.returncode}\n{completed.stdout}"
        failed = completed.returncode != 0
    except OSError as error:
        output = f"Could not run {' '.join(command)}: {error}"
        failed = True
    return {
        "result": {
            "content": [{"type": "text", "text": output}],
            "isError": failed,
        }
    }


def respond(message):
    if (
        not isinstance(message, dict)
        or message.get("jsonrpc") != "2.0"
        or not isinstance(message.get("method"), str)
    ):
        return {"error": {"code": -32600, "message": "Invalid request"}}
    # Notifications, including initialized and cancellation, receive no reply.
    if "id" not in message:
        return None
    params = message.get("params", {})
    if not isinstance(params, dict):
        return {"error": {"code": -32602, "message": "Expected object params"}}
    method = message["method"]
    if method == "initialize":
        return {
            "result": {
                "protocolVersion": "2025-11-25",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "wasm-runner-dev", "version": "0.1.0"},
            }
        }
    if method == "ping":
        return {"result": {}}
    if method == "tools/list":
        return {
            "result": {
                "tools": [
                    {
                        "name": name,
                        "description": f"Run `{' '.join(command)}` in the wasm-runner checkout.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {},
                            "additionalProperties": False,
                        },
                    }
                    for name, command in COMMANDS.items()
                ]
            }
        }
    if method == "tools/call":
        return call_tool(params)
    return {"error": {"code": -32601, "message": "Method not found"}}


def main():
    for line in sys.stdin:
        request_id = None
        try:
            message = json.loads(line)
            if isinstance(message, dict):
                request_id = message.get("id")
            response = respond(message)
        except json.JSONDecodeError:
            response = {"error": {"code": -32700, "message": "Parse error"}}
        if response is not None:
            print(json.dumps({"jsonrpc": "2.0", "id": request_id, **response}), flush=True)


if __name__ == "__main__":
    main()
