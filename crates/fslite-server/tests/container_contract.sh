#!/bin/sh
set -eu

repo_root="$(CDPATH= cd -- "$(dirname "$0")/../../.." && pwd)"
entrypoint="$repo_root/crates/fslite-server/docker-entrypoint.sh"
smoke_script="$repo_root/crates/fslite-server/tests/container_smoke.sh"
test_dir="$(mktemp -d)"

cleanup() {
    rm -rf "$test_dir"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$test_dir/data" "$test_dir/config" "$test_dir/bin" "$test_dir/state"
startup_output="$(FSLITE_DB="$test_dir/data/fslite.db" \
    FSLITE_CONFIG="$test_dir/config/server.json" \
    "$entrypoint" sh -c 'exit 0')"
if ! printf '%s\n' "$startup_output" | grep -F "FSLITE_DB=$test_dir/data/fslite.db" >/dev/null; then
    echo "entrypoint did not report its configured FSLITE_DB path" >&2
    exit 1
fi
if ! printf '%s\n' "$startup_output" | grep -F "FSLITE_CONFIG=$test_dir/config/server.json" >/dev/null; then
    echo "entrypoint did not report its configured FSLITE_CONFIG path" >&2
    exit 1
fi

cat > "$test_dir/bin/docker" <<'EOF'
#!/bin/sh
set -eu

case "$1" in
    container)
        [ "$2" = inspect ]
        exit 1
        ;;
    volume)
        case "$2" in
            inspect)
                exit 1
                ;;
            create)
                : > "$CONTAINER_CONTRACT_STATE/volume"
                ;;
            rm)
                rm -f "$CONTAINER_CONTRACT_STATE/volume"
                ;;
        esac
        ;;
    run)
        : > "$CONTAINER_CONTRACT_STATE/container"
        exit 1
        ;;
    rm)
        rm -f "$CONTAINER_CONTRACT_STATE/container"
        ;;
esac
EOF
chmod +x "$test_dir/bin/docker"

set +e
PATH="$test_dir/bin:$PATH" \
    CONTAINER_CONTRACT_STATE="$test_dir/state" \
    sh "$smoke_script" fslite-server:contract-test >/dev/null 2>&1
smoke_status=$?
set -e

if [ "$smoke_status" -eq 0 ]; then
    echo "smoke script unexpectedly succeeded after docker run failure" >&2
    exit 1
fi
if [ -e "$test_dir/state/container" ]; then
    echo "smoke cleanup did not remove a container created by failed docker run" >&2
    exit 1
fi
if [ -e "$test_dir/state/volume" ]; then
    echo "smoke cleanup did not remove its created volume" >&2
    exit 1
fi
