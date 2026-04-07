FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev pkgconf

WORKDIR /src
COPY . .
RUN cargo build --release --bin tidal-collaborative-playlists

FROM alpine:3

RUN apk add --no-cache ca-certificates sqlite

COPY --from=builder /src/target/release/tidal-collaborative-playlists /usr/local/bin/tidal-collaborative-playlists
COPY --from=builder /src/migrations /migrations

EXPOSE 3000
VOLUME /data

WORKDIR /data

CMD ["tidal-collaborative-playlists"]
