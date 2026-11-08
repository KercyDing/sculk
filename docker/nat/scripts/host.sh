#!/bin/sh
set -eu

ip route replace 172.30.0.0/24 via 172.30.1.254
ip route replace 172.30.2.0/24 via 172.30.1.254

socat TCP-LISTEN:25565,reuseaddr,fork EXEC:cat &
lookup_flag=""
if [ "${ADDRESS_LOOKUP:-1}" = "0" ]; then
    lookup_flag="--no-address-lookup"
fi
sculk host --port 25565 --relay "$RELAY_URL" $lookup_flag >/shared/host.log 2>&1 &

for _ in $(seq 1 60); do
    uri=$(grep -o 'sculk://join/v1/[A-Za-z0-9_-]*' /shared/host.log 2>/dev/null | head -n 1 || true)
    if [ -n "$uri" ]; then
        printf '%s\n' "$uri" >/shared/uri
        break
    fi
    sleep 1
done

if [ ! -s /shared/uri ]; then
    echo "host-failed" >/shared/result
    tail -n 40 /shared/host.log || true
    exit 1
fi

wait
