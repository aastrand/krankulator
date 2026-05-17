# Web Dev Setup

The web frontend requires HTTPS for Gamepad API and SharedArrayBuffer (COOP/COEP headers).

## Prerequisites

```bash
sudo apt install mkcert libnss3-tools
mkcert -install
```

Restart Firefox/Chrome after `mkcert -install` to pick up the local CA.

## Generate Certs

From the repo root:

```bash
mkcert localhost 127.0.0.1
```

This creates `localhost+1.pem` and `localhost+1-key.pem` in the current directory.

## Run Trunk

```bash
cd web
trunk serve --port 8080
```

`Trunk.toml` is configured to pick up the certs from `../localhost+1.pem` and `../localhost+1-key.pem` (one level above `web/`).

If certs are elsewhere, override via CLI:

```bash
trunk serve --tls-key-path /path/to/key.pem --tls-cert-path /path/to/cert.pem
```

## Why HTTPS?

- `navigator.getGamepads()` requires a secure context
- `SharedArrayBuffer` requires COOP/COEP headers, which browsers only honor over HTTPS
- AudioWorklet also requires a secure context
