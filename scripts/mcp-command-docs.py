"""Export a live MCP 2026 command snapshot without overwriting the hand-written guide."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

VERSION = "2026-07-28"
REQUEST_ID = "hwpcraft-docs-command-contracts"
RESOURCE = "hwpcraft://commands"
DEFAULT_OUTPUT = (
    Path(__file__).resolve().parents[1]
    / "output/implementation/0.17/mcp-command-contracts.md"
)
MAX_RESPONSE = 32 * 1024 * 1024


def reject_constant(value: str) -> None:
    raise ValueError(f"non-JSON number: {value}")


def unique_object(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def decode_json(text: str) -> object:
    return json.loads(text, parse_constant=reject_constant, object_pairs_hook=unique_object)


def parse_commands(stdout: bytes) -> list[dict]:
    """Validate every stdout line; do not search for a JSON prefix or skip banners."""
    if not stdout or not stdout.endswith(b"\n") or len(stdout) > MAX_RESPONSE:
        raise ValueError("stdout must be nonempty, newline-terminated JSONL within 32 MiB")
    text = stdout.decode("utf-8", errors="strict")
    messages = []
    for number, line in enumerate(text.split("\n")[:-1], start=1):
        value = decode_json(line)
        if not isinstance(value, dict) or value.get("jsonrpc") != "2.0":
            raise ValueError(f"stdout line {number} is not a JSON-RPC 2.0 object")
        if value.get("id") != REQUEST_ID or type(value.get("id")) is not str:
            raise ValueError(f"stdout line {number} has an unexpected response ID")
        if "method" in value:
            raise ValueError("unexpected server request/notification in command-only export")
        if "error" in value:
            raise ValueError(f"MCP resource request failed: {json.dumps(value['error'], ensure_ascii=False)}")
        if "result" not in value:
            raise ValueError("JSON-RPC response has no result")
        messages.append(value)
    if len(messages) != 1:
        raise ValueError("expected exactly one response to the one resource request")
    result = messages[0]["result"]
    if not isinstance(result, dict) or result.get("resultType") != "complete":
        raise ValueError("resource result must be a complete MCP 2026 result")
    if result.get("cacheScope") != "public" or type(result.get("ttlMs")) is not int or result["ttlMs"] != 60_000:
        raise ValueError("command resource cache metadata does not match the public contract")
    contents = result.get("contents")
    if not isinstance(contents, list) or len(contents) != 1:
        raise ValueError("command resource must contain exactly one item")
    content = contents[0]
    if not isinstance(content, dict) or content.get("uri") != RESOURCE or content.get("mimeType") != "application/json" or not isinstance(content.get("text"), str):
        raise ValueError("command resource content has the wrong URI, MIME type or text")
    commands = decode_json(content["text"])
    if not isinstance(commands, list) or not commands:
        raise ValueError("command catalog must be a nonempty JSON array")
    seen = set()
    for command in commands:
        if not isinstance(command, dict):
            raise ValueError("command catalog entries must be objects")
        for field in ("id", "label", "shortcut", "paramsHelp"):
            if not isinstance(command.get(field), str):
                raise ValueError(f"command {field} must be a string")
        identifier = command["id"]
        if not identifier or identifier in seen or any(ord(char) < 32 for char in identifier):
            raise ValueError("command IDs must be nonempty, unique and free of control characters")
        seen.add(identifier)
        if not isinstance(command.get("paramsExample"), dict):
            raise ValueError(f"{identifier}: paramsExample must be a JSON object")
        actions = command.get("hwpActions")
        if not isinstance(actions, list) or any(not isinstance(action, str) for action in actions):
            raise ValueError(f"{identifier}: hwpActions must be a string array")
    return commands


def fetch_commands(executable: Path, timeout: float) -> list[dict]:
    request = {
        "jsonrpc": "2.0",
        "id": REQUEST_ID,
        "method": "resources/read",
        "params": {
            "uri": RESOURCE,
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": VERSION,
                "io.modelcontextprotocol/clientCapabilities": {},
            },
        },
    }
    wire = (json.dumps(request, ensure_ascii=False) + "\n").encode("utf-8")
    completed = subprocess.run(
        [str(executable), "mcp", "--transport", "stdio"],
        input=wire,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    if completed.returncode != 0:
        detail = completed.stderr.decode("utf-8", errors="replace")[:4096]
        raise ValueError(f"MCP child exited {completed.returncode}: {detail}")
    return parse_commands(completed.stdout)


def render(commands: list[dict]) -> str:
    stamp = datetime.now(timezone.utc).isoformat(timespec="seconds")
    lines = [
        "# HwpCraft MCP command contract snapshot", "",
        f"Protocol: `{VERSION}`. Generated at `{stamp}`.", "",
        "Generated from the supplied executable's live `hwpcraft://commands` resource.",
        "This snapshot is not the hand-written `docs/mcp-commands.md` guide.",
        "Examples require an explicit document_id on the enclosing execute tool and current document prerequisites.",
        "Command registration and examples do not establish complete Hancom feature parity.",
        "", f"Command count: **{len(commands)}**", "",
    ]
    for command in commands:
        lines.extend([
            f"## `{command['id']}` — {command['label']}", "",
            command["paramsHelp"], "", "```json",
            json.dumps(command["paramsExample"], ensure_ascii=False, indent=2), "```", "",
        ])
    return "\n".join(lines)


def write_snapshot(output: Path, text: str) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", newline="\n", dir=output.parent,
                                         prefix=f".{output.name}.", suffix=".tmp", delete=False) as stream:
            temporary = Path(stream.name)
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, output)
        temporary = None
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("executable", type=Path, help="built hwpcraft-cli executable (required)")
    result.add_argument("--output", type=Path, default=DEFAULT_OUTPUT,
                        help=f"snapshot destination (default: {DEFAULT_OUTPUT})")
    result.add_argument("--timeout", type=float, default=30.0, help="child deadline in seconds (default: 30)")
    return result


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    executable = args.executable.resolve()
    if not executable.is_file():
        raise ValueError(f"executable does not exist: {executable}")
    if not 0 < args.timeout <= 300:
        raise ValueError("--timeout must be greater than zero and at most 300 seconds")
    commands = fetch_commands(executable, args.timeout)
    output = args.output.resolve()
    write_snapshot(output, render(commands))
    print(f"Wrote {len(commands)} command contracts to {output}")
    return 0


if __name__ == "__main__":
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8", errors="replace")
    try:
        raise SystemExit(main())
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"mcp-command-docs: {error}", file=sys.stderr)
        raise SystemExit(1) from None
