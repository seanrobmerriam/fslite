#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
entrypoint="$script_dir/../docker-entrypoint.sh"
temporary_directory=$(mktemp -d "${TMPDIR:-/tmp}/fslite-showcase-entrypoint.XXXXXX")
trap 'rm -rf "$temporary_directory"' EXIT HUP INT TERM

test_token='test-token-must-not-appear-in-entrypoint-output'
token_file="$temporary_directory/token"
printf '%s\n' "$test_token" > "$token_file"

assert_accepts() {
  url="$1"
  output_file="$temporary_directory/accept.out"

  if ! FSLITE_SERVER_URL="$url" FSLITE_TOKEN_FILE="$token_file" \
    "$entrypoint" true >"$output_file" 2>&1; then
    echo "entrypoint rejected a valid URL" >&2
    exit 1
  fi

  if grep -F "$test_token" "$output_file" >/dev/null; then
    echo "entrypoint exposed the token" >&2
    exit 1
  fi
}

assert_rejects() {
  url="$1"
  output_file="$temporary_directory/reject.out"

  if FSLITE_SERVER_URL="$url" FSLITE_TOKEN_FILE="$token_file" \
    "$entrypoint" true >"$output_file" 2>&1; then
    echo "entrypoint accepted invalid URL input" >&2
    exit 1
  fi

  if grep -F "$test_token" "$output_file" >/dev/null; then
    echo "entrypoint exposed the token" >&2
    exit 1
  fi
}

assert_accepts 'http://fslite-server:8080'
assert_accepts 'https://upstream.example.test'
assert_rejects 'http://'
assert_rejects 'https://'
assert_rejects 'http://?missing-host'
assert_rejects 'ftp://upstream.example.test'

missing_url_output="$temporary_directory/missing-url.out"
if FSLITE_TOKEN_FILE="$token_file" "$entrypoint" true >"$missing_url_output" 2>&1; then
  echo "entrypoint accepted a missing URL" >&2
  exit 1
fi

if grep -F "$test_token" "$missing_url_output" >/dev/null; then
  echo "entrypoint exposed the token" >&2
  exit 1
fi

printf 'docker entrypoint contract passed\n'
