#!/bin/sh
set -eu

: "${FSLITE_SERVER_URL:?FSLITE_SERVER_URL must be set}"
: "${FSLITE_TOKEN_FILE:?FSLITE_TOKEN_FILE must be set}"

if ! node -e '
try {
    const serverUrl = new URL(process.env.FSLITE_SERVER_URL);
    if (
        !["http:", "https:"].includes(serverUrl.protocol) ||
        !serverUrl.hostname
    ) {
        process.exit(1);
    }
} catch {
    process.exit(1);
}
'; then
        echo "FSLITE_SERVER_URL must be an absolute HTTP(S) URL with a host" >&2
        exit 1
fi

if [ ! -r "$FSLITE_TOKEN_FILE" ]; then
    echo "FSLITE_TOKEN_FILE must name a readable file" >&2
    exit 1
fi

if ! tr -d '[:space:]' < "$FSLITE_TOKEN_FILE" | grep -q .; then
    echo "FSLITE_TOKEN_FILE must not be empty" >&2
    exit 1
fi

exec "$@"
