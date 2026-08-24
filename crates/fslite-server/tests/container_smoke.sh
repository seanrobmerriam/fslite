#!/bin/sh
set -eu

image="${1:-fslite-server:local}"
container="fslite-server-smoke"
volume="fslite-server-smoke-data"
script_dir="$(CDPATH= cd -- "$(dirname "$0")" && pwd)"
smoke_dir="$(mktemp -d "$script_dir/fslite-server-smoke.XXXXXX")"
container_created=false
volume_created=false

cleanup() {
    if [ "$container_created" = true ]; then
        docker rm -f "$container" >/dev/null 2>&1 || true
    fi
    if [ "$volume_created" = true ]; then
        docker volume rm "$volume" >/dev/null 2>&1 || true
    fi
    rm -rf "$smoke_dir"
}
trap cleanup EXIT HUP INT TERM

if docker container inspect "$container" >/dev/null 2>&1; then
    echo "refusing to remove existing container: $container" >&2
    exit 1
fi
if docker volume inspect "$volume" >/dev/null 2>&1; then
    echo "refusing to remove existing volume: $volume" >&2
    exit 1
fi

umask 077
printf '%s\n' 'container-smoke-token' > "$smoke_dir/token"

docker volume create fslite-server-smoke-data >/dev/null
volume_created=true

start_server() {
    container_created=true
    docker run -d --name fslite-server-smoke \
      -p 127.0.0.1:18080:8080 \
      -v fslite-server-smoke-data:/data \
      -v "$smoke_dir/token:/run/secrets/fslite_token:ro" \
      -e FSLITE_TOKEN_FILE=/run/secrets/fslite_token \
      "$image" >/dev/null
}

wait_for_ready() {
    attempts=0
    while [ "$attempts" -lt 30 ]; do
        if curl --fail --silent --show-error http://127.0.0.1:18080/readyz >/dev/null; then
            return 0
        fi
        attempts=$((attempts + 1))
        sleep 1
    done
    echo "server did not become ready" >&2
    docker logs "$container" >&2 || true
    return 1
}

assert_startup_paths() {
    startup_log="$(docker logs "$container" 2>&1)"
    case "$startup_log" in
        *"FSLITE_DB=/data/fslite.db"*) ;;
        *)
            echo "server startup did not report FSLITE_DB" >&2
            return 1
            ;;
    esac
    case "$startup_log" in
        *"FSLITE_CONFIG=/data/server.json"*) ;;
        *)
            echo "server startup did not report FSLITE_CONFIG" >&2
            return 1
            ;;
    esac
}

start_server
wait_for_ready
assert_startup_paths

identity="$(curl --fail --silent --show-error \
    -H 'Authorization: Bearer container-smoke-token' \
    http://127.0.0.1:18080/v1/me)"
workspace_id="$(printf '%s' "$identity" | sed -n 's/.*"workspace_id":"\([^"]*\)".*/\1/p')"
if [ -z "$workspace_id" ]; then
    echo "could not parse workspace_id from /v1/me" >&2
    exit 1
fi

curl --fail --silent --show-error \
    -X PUT \
    -H 'Authorization: Bearer container-smoke-token' \
    --data-binary 'persistent' \
    "http://127.0.0.1:18080/v1/workspaces/$workspace_id/content/persist.txt" >/dev/null

docker rm -f "$container" >/dev/null
container_created=false

start_server
wait_for_ready
assert_startup_paths

content="$(curl --fail --silent --show-error \
    -H 'Authorization: Bearer container-smoke-token' \
    "http://127.0.0.1:18080/v1/workspaces/$workspace_id/content/persist.txt")"
if [ "$content" != 'persistent' ]; then
    echo "persistent content was not preserved" >&2
    exit 1
fi
