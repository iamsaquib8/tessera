FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin tessera

FROM debian:bookworm-slim
LABEL org.opencontainers.image.source="https://github.com/iamsaquib8/tessera"
LABEL org.opencontainers.image.description="Deterministic edit safety for AI coding agents"
LABEL org.opencontainers.image.licenses="Apache-2.0"
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/* && useradd -m tessera \
    && git config --system --add safe.directory '*'
COPY --from=build /src/target/release/tessera /usr/local/bin/tessera
USER tessera
WORKDIR /work
# The index lives in the container, so a bind-mounted /work owned by any host
# user stays read-only to tessera. Set TESSERA_DB to keep it elsewhere.
ENV TESSERA_DB=/tmp/tessera/tessera.db
ENTRYPOINT ["tessera", "--root", "/work"]
CMD ["status"]
