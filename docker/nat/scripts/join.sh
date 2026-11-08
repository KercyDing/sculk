#!/bin/sh
set -eu

ip route replace 172.30.0.0/24 via 172.30.2.254
ip route replace 172.30.1.0/24 via 172.30.2.254

for _ in $(seq 1 60); do
    [ -s /shared/uri ] && break
    sleep 1
done

uri=$(cat /shared/uri)
lookup_flag=""
if [ "${ADDRESS_LOOKUP:-1}" = "0" ]; then
    lookup_flag="--no-address-lookup"
fi
sculk join "$uri" --port 30000 --delay 1 --status-file /shared/status $lookup_flag >/shared/join.log 2>&1 &
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
if [ "$expected" != "any" ]; then
    if [ "$expected" = "direct" ]; then
        want_relay=false
    else
        want_relay=true
    fi
    for _ in $(seq 1 15); do
        if [ -s /shared/status ] && grep -q "is_relay=${want_relay}" /shared/status; then
            break
        fi
        sleep 1
    done
fi

if grep -q 'is_relay=true' /shared/status 2>/dev/null; then
    echo relay >/shared/path
elif grep -q 'is_relay=false' /shared/status 2>/dev/null; then
    echo direct >/shared/path
else
    echo unknown >/shared/path
fi

initial_echo="$(cat /shared/echo 2>/dev/null || echo missing)"
initial_path="$(cat /shared/path 2>/dev/null || echo unknown)"

fault="${FAULT:-none}"
if [ "$fault" = "none" ]; then
    if [ "$initial_echo" = "hello" ] && [ "$initial_path" = "$expected" ]; then
        echo pass >/shared/result
    else
        echo "fail:echo=${initial_echo} path=${initial_path}" >/shared/result
    fi
else
    echo ready >/shared/ready
    for _ in $(seq 1 60); do
        [ -s /shared/restored ] && break
        sleep 1
    done

    reconnected=false
    for _ in $(seq 1 60); do
        if grep -q 'Reconnected to host' /shared/join.log 2>/dev/null; then
            reconnected=true
            break
        fi
        sleep 1
    done

    if printf 'hello' | timeout 10 socat - TCP:127.0.0.1:30000 >/shared/echo2; then
        recovered_echo="$(cat /shared/echo2)"
    else
        recovered_echo="failed"
    fi

    if grep -q 'is_relay=true' /shared/status 2>/dev/null; then
        echo relay >/shared/path
    elif grep -q 'is_relay=false' /shared/status 2>/dev/null; then
        echo direct >/shared/path
    fi

    if [ "$initial_echo" = "hello" ] && [ "$reconnected" = true ] && [ "$recovered_echo" = "hello" ]; then
        echo pass >/shared/result
    else
        echo "fail:echo=${initial_echo} reconnected=${reconnected} recovered=${recovered_echo}" >/shared/result
    fi
fi

kill "$join_pid" 2>/dev/null || true
wait "$join_pid" 2>/dev/null || true
