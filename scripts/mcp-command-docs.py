"""Generate the command reference from a built HwpCraft MCP server's discovery data."""
import json
from pathlib import Path
import subprocess
import sys

exe = Path(sys.argv[1]).resolve()
messages = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-06-18", "capabilities": {},
        "clientInfo": {"name": "hwpcraft-docs", "version": "1"}}},
    {"jsonrpc": "2.0", "method": "notifications/initialized"},
    {"jsonrpc": "2.0", "id": 2, "method": "resources/read",
     "params": {"uri": "hwpcraft://commands"}},
]
completed = subprocess.run([str(exe), "mcp"],
    input="\n".join(json.dumps(m, ensure_ascii=False) for m in messages)+"\n",
    capture_output=True, encoding="utf-8", timeout=30, check=True)
replies = [json.loads(line) for line in completed.stdout.splitlines()]
reply = next(r for r in replies if r.get("id") == 2)
commands = json.loads(reply["result"]["contents"][0]["text"])
lines = ["# MCP 명령 인자 참고", "",
    "`scripts/mcp-command-docs.py <hwpcraft-cli.exe>`로 실제 MCP discovery에서 생성합니다.",
    "인자 예제는 형식 안내입니다. 현재 선택·기존 개체·파일 등 실행 전제 조건을 먼저 준비하세요.",
    "`execute`에 `command`와 아래 객체를 `params`로 전달합니다. 추가 선택 인자는 설명을 참고하세요.",
    "이 문서는 모든 세부 서식 필드의 JSON Schema를 대체하지 않습니다. `*.get` 조회와 엔진 검증을 함께 사용하세요.",
    "", f"명령 수: **{len(commands)}**", ""]
for c in commands:
    lines.extend([f"## `{c['id']}` — {c['label']}", "", c["paramsHelp"], "",
        "```json", json.dumps(c["paramsExample"], ensure_ascii=False, indent=2), "```", ""])
path = Path(__file__).resolve().parents[1]/"docs"/"mcp-commands.md"
path.write_text("\n".join(lines), encoding="utf-8")
print(f"Wrote {len(commands)} command contracts to {path}")
