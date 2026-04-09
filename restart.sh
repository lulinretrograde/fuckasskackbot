#!/usr/bin/env bash
# Build, stop PM2 cleanly, kill any strays, then start fresh.
# Usage: ./restart.sh
set -euo pipefail

PM2_NAME="idf-soldat"

echo "==> Building release binary..."
~/.cargo/bin/cargo build --release

echo "==> Stopping PM2 job..."
pm2 stop "$PM2_NAME" 2>/dev/null || true

# Wait until PM2 confirms the process is stopped (not just signalled)
for i in $(seq 1 20); do
    STATUS=$(pm2 jlist 2>/dev/null | python3 -c "
import sys, json
procs = json.load(sys.stdin)
for p in procs:
    if p.get('name') == '$PM2_NAME':
        print(p.get('pm2_env', {}).get('status', 'unknown'))
        break
" 2>/dev/null || echo "stopped")
    [ "$STATUS" = "stopped" ] && break
    sleep 0.5
done

echo "==> Killing any remaining processes..."
pkill -9 -x idf-soldat 2>/dev/null || true

# Wait until fully clear
for i in $(seq 1 20); do
    pgrep -x idf-soldat > /dev/null 2>&1 || break
    sleep 0.5
done
pkill -9 -x idf-soldat 2>/dev/null || true
sleep 0.5

echo "==> Starting PM2 job..."
pm2 start "$PM2_NAME"

sleep 2
echo "==> Running instances:"
ps aux | grep idf-soldat | grep -v grep | grep -v bash
