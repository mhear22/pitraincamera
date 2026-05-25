# Pi Train Camera — Sound Guard

Audio monitoring system for Raspberry Pi. Listens to onboard mic, captures photos + audio clips on loud events, reports back to a dashboard server.

> **Rewrite of the original Django/Python version.** The old code lives on `main`. This branch (`rewrite`) uses Rust (client) + Node.js/Express (server) for better performance on Pi Zero.

## Architecture

- **Pi Zero (client):** Rust app — mic listener → peak detection → capture photo + audio clip → POST to server. Checks in every 60s with system stats and fetches remote settings.
- **Homelab (server):** Express + SQLite — receives events, serves dashboard with event list, metrics, Pi status, and remote config.

## Quick Start

### Server

```bash
cd server
npm install
npm start        # runs on port 3002
```

### Client (cross-compile for Pi Zero)

```bash
cd client-rust
# Install ARMv6 target
rustup target add arm-unknown-linux-gnueabihf
# Build
cargo build --release --target arm-unknown-linux-gnueabihf
# Binary at target/arm-unknown-linux-gnueabihf/release/sound-guard-client
```

Or via Docker (multi-stage build):

```bash
docker buildx build --platform linux/arm/v6 -t sound-guard-client .
```

### Docker Compose (server only)

```bash
docker compose up -d
```

## Configuration

### Client (env vars)

| Variable | Default | Description |
|---|---|---|
| `SERVER_URL` | `http://192.168.1.66:3002/api/events` | Server endpoint |
| `THRESHOLD_DB` | `-10` | Trigger threshold (dBFS, 0 = max) |
| `COOLDOWN_SEC` | `5` | Min time between triggers |
| `SAMPLE_RATE` | `44100` | Audio sample rate |
| `PRE_BUFFER_SEC` | `1.0` | Audio kept before peak |
| `POST_BUFFER_SEC` | `1.0` | Audio kept after peak |

### Remote Settings (via dashboard)

Settings can be changed from the dashboard's **⚙️ Settings** tab. The Pi syncs on its next check-in (every 60s). No restart needed.

## Dashboard

Open `http://<server>:3002` for:
- **Events tab** — paginated event list with photos, audio playback, and dB chart
- **Settings tab** — Pi status (online/offline, CPU temp, uptime, memory, disk), remote config (threshold, cooldown, sample rate, buffers, enable/disable toggle)

## API Endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/api/events` | Receive event from Pi (multipart: event JSON + audio + photo) |
| `GET` | `/api/events` | List events (paginated, filterable) |
| `GET` | `/api/events/:id` | Single event |
| `DELETE` | `/api/events/:id` | Delete event + files |
| `GET` | `/api/stats` | Summary stats |
| `GET` | `/api/metrics` | Hourly/daily aggregations |
| `POST` | `/api/checkin` | Pi check-in (stats) → returns settings |
| `GET` | `/api/checkin/latest` | Latest Pi check-in |
| `GET` | `/api/checkins` | Check-in history |
| `GET` | `/api/settings` | Current Pi settings |
| `PATCH` | `/api/settings` | Update Pi settings |

## Testing

```bash
# Server
cd server && npm test

# Client
cd client-rust && cargo test
```

## Original Version

The Django/Python version with `pyaudio` listener and Django REST Framework webapp is preserved on the `main` branch.
