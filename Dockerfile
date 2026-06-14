# Multi-stage build: compile the static-ish CLI, ship a slim runtime.
FROM rust:1.82-bookworm AS build
WORKDIR /src
# Leverage layer caching: deps first, then sources.
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY tests ./tests
COPY examples ./examples
RUN cargo build --release --bin tessera

FROM debian:bookworm-slim
LABEL org.opencontainers.image.source="https://github.com/iamsaquib8/tessera"
LABEL org.opencontainers.image.description="Local, deterministic semantic code graph + MCP server for AI coding agents"
LABEL org.opencontainers.image.licenses="Apache-2.0"
RUN useradd -m tessera
COPY --from=build /src/target/release/tessera /usr/local/bin/tessera
USER tessera
WORKDIR /work
ENTRYPOINT ["tessera"]
CMD ["--help"]
