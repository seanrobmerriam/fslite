#!/bin/sh
set -eu

check_writable_parent() {
    path="$1"
    parent="$(dirname "$path")"

    if [ ! -d "$parent" ]; then
        echo "configured path has no parent directory: $path" >&2
        exit 1
    fi
    if [ ! -w "$parent" ]; then
        echo "configured path parent is not writable: $path" >&2
        exit 1
    fi
}

: "${FSLITE_DB:?FSLITE_DB must be set}"
: "${FSLITE_CONFIG:?FSLITE_CONFIG must be set}"
check_writable_parent "$FSLITE_DB"
check_writable_parent "$FSLITE_CONFIG"

exec "$@"
