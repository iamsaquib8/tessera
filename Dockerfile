# Base images are pinned by digest so a rebuild gives the same image; Dependabot
# (docker ecosystem in .github/dependabot.yml) proposes new digests.
FROM rust:1-trixie@sha256:223cc32ddca8f3ee90c436dbc94b6b5f052ea7f9653c5201e9612f08ad7d21b8 AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY packs ./packs
RUN cargo build --release --locked --bin tessera

FROM debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2
LABEL org.opencontainers.image.source="https://github.com/iamsaquib8/tessera"
LABEL org.opencontainers.image.description="Deterministic edit safety for AI coding agents"
LABEL org.opencontainers.image.licenses="Apache-2.0"
RUN apt-get update && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/* && useradd -m -u 10001 tessera \
    && git config --system --add safe.directory '*'
COPY --from=build /src/target/release/tessera /usr/local/bin/tessera
COPY LICENSE THIRD-PARTY-NOTICES /usr/share/doc/tessera/
# A numeric user, so Kubernetes `runAsNonRoot` can check it.
USER 10001:10001
WORKDIR /work
# The index lives in the container, so a bind-mounted /work owned by any host
# user stays read-only to tessera. Set TESSERA_DB to keep it elsewhere.
ENV TESSERA_DB=/tmp/tessera/tessera.db
ENTRYPOINT ["tessera", "--root", "/work"]
CMD ["status"]
