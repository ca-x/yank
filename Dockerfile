FROM rust:1-bookworm AS builder

WORKDIR /app
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
RUN git config --global url."https://github.com/".insteadOf "ssh://git@github.com/" \
    && git config --global url."https://github.com/".insteadOf "git@github.com:"
COPY . .

RUN cargo build --locked --release -p yank-server

FROM debian:bookworm-slim

ARG VERSION=dev
ARG BUILD_TIME=unknown
ARG GIT_COMMIT=unknown

LABEL org.opencontainers.image.title="yank-server"
LABEL org.opencontainers.image.description="Self-hosted yank clipboard sync server"
LABEL org.opencontainers.image.version="${VERSION}"
LABEL org.opencontainers.image.created="${BUILD_TIME}"
LABEL org.opencontainers.image.revision="${GIT_COMMIT}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/yank-server /usr/local/bin/yank-server

ENV YANK_BIND=0.0.0.0:7219
ENV YANK_DB=/data/yank-server.sqlite3

VOLUME ["/data"]
EXPOSE 7219

ENTRYPOINT ["yank-server"]
