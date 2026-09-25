#!/bin/sh
set -eu
cd "$(dirname "$0")/.."

CARGO="${CARGO:-$(command -v cargo || echo "$HOME/.cargo/bin/cargo")}"
SOCKET="${ONEO_SOCKET:-/tmp/oneos-dev.sock}"

"$CARGO" build -p oneosd -p oneos

rm -f "$SOCKET"

ONEO_DEV=1 ONEO_SOCKET="$SOCKET" target/debug/oneosd &
daemon_pid=$!
trap 'kill "$daemon_pid" 2>/dev/null || true; rm -f "$SOCKET"' EXIT

i=0
while [ ! -S "$SOCKET" ] && [ "$i" -lt 50 ]; do
    if ! kill -0 "$daemon_pid" 2>/dev/null; then
        echo "oneosd exited during startup" >&2
        exit 1
    fi
    sleep 0.1
    i=$((i + 1))
done

ONEO_SOCKET="$SOCKET" target/debug/oneos "$@"
