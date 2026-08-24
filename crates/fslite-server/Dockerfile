FROM rust:1.85-bookworm AS builder

WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --locked --release --package fslite-server

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install --no-install-recommends -y ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 fslite \
    && useradd --uid 10001 --gid fslite --no-create-home fslite \
    && install --directory --owner=10001 --group=10001 /data

COPY --from=builder /workspace/target/release/fslite-server /usr/local/bin/fslite-server
COPY crates/fslite-server/docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh

RUN chmod 0755 /usr/local/bin/fslite-server /usr/local/bin/docker-entrypoint.sh

ENV FSLITE_DB=/data/fslite.db \
    FSLITE_CONFIG=/data/server.json \
    FSLITE_BIND=0.0.0.0:8080

VOLUME ["/data"]
EXPOSE 8080
USER 10001:10001

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["/usr/local/bin/fslite-server"]

HEALTHCHECK --interval=5s --timeout=3s --start-period=10s --retries=12 \
  CMD curl --fail --silent http://127.0.0.1:8080/readyz || exit 1
