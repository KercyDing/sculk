#!/bin/sh
set -eu

ip route replace 172.30.0.0/24 via 172.30.2.254

for _ in $(seq 1 60); do
    [ -s /shared/uri ] && break
    sleep 1
done

uri=$(cat /shared/uri)
sculk join "$uri" --port 30000 --delay 1 >/shared/join.log 2>&1 &
join_pid=$!

for _ in $(seq 1 30); do
    nc -z 127.0.0.1 30000 2>/dev/null && break
    sleep 1
done

if printf 'hello' | timeout 10 socat - TCP:127.0.0.1:30000 >/shared/echo; then
    echo "echo-ok" >/shared/echo-status
else
    echo "echo-failed" >/shared/echo-status
fi

expected="${EXPECTED_PATH:-relay}"
for _ in $(seq 1 15); do
    grep -q "path: ${expected}" /shared/join.log 2>/dev/null && break
    sleep 1
done

grep -o 'path: [a-z]*' /shared/join.log 2>/dev/null | tail -n 1 | awk '{print $2}' >/shared/path || true

if [ "$(cat /shared/echo)" = "hello" ] && [ "$(cat /shared/path)" = "$expected" ]; then
    echo pass >/shared/result
else
    echo "fail:echo=$(cat /shared/echo 2>/dev/null) path=$(cat /shared/path 2>/dev/null)" >/shared/result
fi

kill "$join_pid" 2>/dev/null || true
wait "$join_pid" 2>/dev/null || true
