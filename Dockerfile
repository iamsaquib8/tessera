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
    && rm -rf /var/lib/apt/lists/* && useradd -m tessera
COPY --from=build /src/target/release/tessera /usr/local/bin/tessera
USER tessera
WORKDIR /work
ENTRYPOINT ["tessera", "--root", "/work"]
CMD ["status"]
