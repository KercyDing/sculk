#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$root"

command -v docker >/dev/null 2>&1 || {
    echo "docker is required" >&2
    exit 1
}
docker compose version >/dev/null 2>&1 || {
    echo "docker compose is required" >&2
    exit 1
}
docker info >/dev/null 2>&1 || {
    echo "docker daemon is not available" >&2
    exit 1
}
for cmd in awk curl openssl; do
    command -v "$cmd" >/dev/null 2>&1 || {
        echo "$cmd is required" >&2
        exit 1
    }
done

compose="docker compose -f docker/nat/compose.yml"

relay_version="${RELAY_VERSION:-}"
if [ -z "$relay_version" ]; then
    relay_version=$(awk '
        $0 == "name = \"iroh-relay\"" { found = 1; next }
        found && $0 ~ /^version = / {
            gsub(/"/, "", $3)
            print $3
            exit
        }
    ' Cargo.lock)
fi
if [ -z "$relay_version" ]; then
    echo "cannot determine iroh-relay version from Cargo.lock; set RELAY_VERSION" >&2
    exit 1
fi

relay_bin="docker/nat/bin/iroh-relay"
relay_stamp="docker/nat/bin/iroh-relay.version"

mkdir -p docker/nat/bin docker/nat/certs docker/nat/run

echo "==> building sculk CLI inside the Docker builder"
if [ ! -x "$relay_bin" ] || [ "$(cat "$relay_stamp" 2>/dev/null)" != "$relay_version" ]; then
    echo "==> downloading iroh-relay $relay_version"
    curl -fL \
        "https://github.com/KercyDing/iroh-relay/releases/download/v${relay_version}/iroh-relay-linux-amd64" \
        -o "$relay_bin"
    chmod +x "$relay_bin"
    printf '%s\n' "$relay_version" >"$relay_stamp"
fi

if [ ! -f docker/nat/certs/cert.pem ]; then
    echo "==> generating self-signed relay certificate"
    openssl req -x509 -newkey rsa:2048 -nodes \
        -keyout docker/nat/certs/key.pem \
        -out docker/nat/certs/cert.pem \
        -days 1 \
        -subj "/CN=172.30.0.10" \
        -addext "subjectAltName=IP:172.30.0.10" >/dev/null 2>&1
fi

echo "==> building Docker image"
$compose build
$compose down -v --remove-orphans >/dev/null 2>&1 || true

cleanup() {
    $compose down -v --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT INT TERM

run_case() {
    mode="$1"
    block_direct="$2"
    expected_path="$3"

    rm -rf docker/nat/run
    mkdir -p docker/nat/run

    echo
    echo "==> ${mode}: BLOCK_DIRECT=${block_direct}, expected path=${expected_path}"
    EXPECTED_PATH="$expected_path" BLOCK_DIRECT="$block_direct" $compose up -d --force-recreate

    for _ in $(seq 1 90); do
        [ -s docker/nat/run/result ] && break
        sleep 1
    done

    result=$(cat docker/nat/run/result 2>/dev/null || echo timeout)
    path=$(cat docker/nat/run/path 2>/dev/null || echo unknown)

    echo "result=${result} path=${path}"
    echo "--- host log ---"
    tail -n 20 docker/nat/run/host.log 2>/dev/null || true
    echo "--- join log ---"
    tail -n 30 docker/nat/run/join.log 2>/dev/null || true
    echo "--- relay / nat logs ---"
    $compose logs --no-color --tail=40 relay nat-a nat-b 2>/dev/null || true

    if [ "$result" != "pass" ]; then
        echo "FAIL: ${mode} did not forward TCP (result=${result})"
        return 1
    fi
    if [ "$path" != "$expected_path" ]; then
        echo "FAIL: ${mode} expected path=${expected_path}, got ${path}"
        return 1
    fi

    echo "PASS: ${mode}"
}

status=0
run_case "relay-only" 1 relay || status=1
run_case "holepunch" 0 direct || status=1
exit "$status"
