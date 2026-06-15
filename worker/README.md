# krankulator-cors

Cloudflare Worker that proxies ROM downloads and adds CORS headers so the web frontend can fetch them.

## First-time setup

1. Install Node.js and wrangler:
   ```bash
   sudo apt-get install -y nodejs
   npm install -g wrangler
   ```

2. Log in to Cloudflare:
   ```bash
   wrangler login
   ```
   This opens a browser for OAuth. Make sure your Cloudflare email is verified.

3. Register a workers.dev subdomain (first time only — must be interactive):
   ```bash
   cd worker
   wrangler deploy
   ```
   It will prompt you to pick a subdomain like `krankulator`. Say yes.
   The worker URL will be `https://krankulator-cors.<your-subdomain>.workers.dev`.

## Deploying updates

```bash
cd worker
wrangler deploy
```

## How it works

The frontend fetches ROMs via:
```
https://krankulator-cors.<subdomain>.workers.dev/?url=https://archive.org/download/roms_nes/...
```

The worker fetches the ROM server-side and returns it with `Access-Control-Allow-Origin` set.

Only requests from allowed origins (krankulator.teknodromen.se, localhost:8080) to allowed hosts (*.archive.org) are proxied.
