#!/usr/bin/env bash
# HwpCraft build/test/run helper for macOS and Linux (the PowerShell twin is HwpCraft.ps1).
# Usage: ./hwpcraft.sh [build|test|install|start [file]|status]   (default: start)
set -euo pipefail

PROJECT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE="$(dirname "$PROJECT")"
TARGET="${CARGO_TARGET_DIR:-$WORKSPACE/.build/hwpcraft}"
LOGS="$PROJECT/logs"
RUNTIME="$PROJECT/runtime"
TOOLCHAIN="${HWPCRAFT_TOOLCHAIN:-1.98.1}"
JOBS="${HWPCRAFT_JOBS:-12}"
VERSION="$(tr -d '[:space:]' < "$PROJECT/VERSION")"
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]{2}$ ]]; then
  echo "VERSION must use x.xx format, for example 0.10." >&2
  exit 1
fi
mkdir -p "$LOGS" "$RUNTIME"

cargo_run() { # <log> <cargo args...>
  local log="$1"; shift
  if ! (cd "$PROJECT" && CARGO_TARGET_DIR="$TARGET" CARGO_BUILD_JOBS="$JOBS" cargo "+$TOOLCHAIN" "$@" >"$log" 2>&1); then
    tail -n 50 "$log" >&2
    echo "Cargo failed: $log" >&2
    exit 1
  fi
}

case "${1:-start}" in
  build)
    [[ -f "$WORKSPACE/rhwp/Cargo.toml" ]] || { echo "RHWP source must exist in ../rhwp. No automatic Git operation is performed." >&2; exit 1; }
    cargo_run "$LOGS/build.log" build --release --locked -p hwpcraft -p hwpcraft-cli
    echo "Build complete: $TARGET/release"
    ;;
  test)
    cargo_run "$LOGS/test.log" test --locked --workspace
    echo "Tests passed: $LOGS/test.log"
    ;;
  install)
    for name in hwpcraft hwpcraft-cli; do
      src="$TARGET/release/$name"
      [[ -f "$src" ]] || { echo "Build first: ./hwpcraft.sh build (missing $src)" >&2; exit 1; }
      install -m 0755 "$src" "$RUNTIME/$name"
    done
    for name in README.md LICENSE THIRD-PARTY-NOTICES.txt VERSION; do cp -f "$PROJECT/$name" "$RUNTIME/$name"; done
    cp -f "$WORKSPACE/rhwp/LICENSE" "$RUNTIME/LICENSE-RHWP.txt"
    for craft in WordCraft DeckCraft DesignCraft; do
      # License texts moved from assets/ to assets/licenses/ on 2026-10-25; accept either place.
      for dir in assets/licenses assets; do
        if [[ -f "$PROJECT/$dir/LICENSE-$craft-MIT.txt" ]]; then cp -f "$PROJECT/$dir/LICENSE-$craft-MIT.txt" "$RUNTIME/"; break; fi
      done
    done
    mkdir -p "$PROJECT/samples"
    if [[ ! -f "$PROJECT/samples/HwpCraft-시작하기.hwpx" ]]; then
      "$RUNTIME/hwpcraft-cli" demo "$PROJECT/samples"
    fi
    echo "Installed HwpCraft $VERSION. Local runtime: $RUNTIME"
    ;;
  start)
    exe="$RUNTIME/hwpcraft"
    [[ -x "$exe" ]] || { echo "Run ./hwpcraft.sh build and ./hwpcraft.sh install first." >&2; exit 1; }
    shift || true
    if [[ $# -gt 0 ]]; then
      (cd "$PROJECT" && nohup "$exe" "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")" >/dev/null 2>&1 &)
    else
      (cd "$PROJECT" && nohup "$exe" >/dev/null 2>&1 &)
    fi
    ;;
  status)
    printf 'Project=%s\nVersion=%s\nExecutable=%s\nBuilt=%s\nInstalled=%s\n' \
      "$PROJECT" "$VERSION" "$RUNTIME/hwpcraft" \
      "$([[ -f "$TARGET/release/hwpcraft" ]] && echo true || echo false)" \
      "$([[ -x "$RUNTIME/hwpcraft" ]] && echo true || echo false)"
    ;;
  *)
    echo "Usage: ./hwpcraft.sh [build|test|install|start [file]|status]" >&2
    exit 2
    ;;
esac
