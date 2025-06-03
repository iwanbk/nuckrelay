# nuckrelay

[![Multi-Platform Build](https://github.com/iwanbk/nuckrelay/actions/workflows/multi-platform.yml/badge.svg)](https://github.com/iwanbk/nuckrelay/actions/workflows/multi-platform.yml)

nuckrelay is a [Netbird](https://github.com/netbirdio/netbird) relay server written in Rust.

## Status

It works for my personal use

Features:
- [x] WebSocket
- [ ] TLS
- [ ] Let's Encrypt
- [ ] QUIC
- [ ] Proper healthcheck
- [ ] Metrics

## Usage

It is currently only tested on Docker-based installations provided by 
the [NetBird self-hosting quickstart guide](https://docs.netbird.io/selfhosted/selfhosted-quickstart).

Assuming that the NetBird installation directory is `$HOME`:

```bash
git clone https://github.com/iwanbk/nuckrelay.git
cd nuckrelay
make alpine-release
cp target/x86_64-unknown-linux-musl/release/nuckrelay $HOME/netbird-relay
```
Modify the `relay` section in the `docker-compose.yml` file:

```yml
relay:
    image: alpine:latest
    restart: unless-stopped
    networks: [netbird]
    volumes:
      # Mount current directory to /app in the container
      - ./:/app
    entrypoint: ["/bin/sh", "-c"]
    command:
      - |
        cd /app
        set -a
        source ./relay.env
        set +a
        chmod +x ./netbird-relay
        ./netbird-relay
    logging:
      driver: "json-file"
      options:
        max-size: "500m"
        max-file: "2"
```

Stop the old containers and start the new ones:
```bash
sudo docker compose down
sudo docker compose up -d caddy zitadel
sudo docker compose up -d
```
