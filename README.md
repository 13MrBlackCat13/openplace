# ![openplace](banner.png "openplace banner")

<p align="center">
  <a href="https://github.com/13MrBlackCat13/openplace/actions/workflows/ci.yml"><img src="https://github.com/13MrBlackCat13/openplace/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/13MrBlackCat13/openplace/actions/workflows/release.yml"><img src="https://github.com/13MrBlackCat13/openplace/actions/workflows/release.yml/badge.svg" alt="Release"></a>
  <img src="https://img.shields.io/badge/rust-stable-DEA584?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/PostgreSQL-17-4169E1?logo=postgresql&logoColor=white" alt="PostgreSQL 17">
  <a href="LICENSE.md"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center"><strong>Translations</strong> v7.0</p>
<p align="center">
    <a href="translations/de/LIESMICH.md"><img src="https://flagcdn.com/256x192/de.png" width="48" alt="German Flag"></a>
    <a href="translations/fr/LISEZMOI.md"><img src="https://flagcdn.com/256x192/fr.png" width="48" alt="French Flag"></a>
    <a href="translations/id/README.md"><img src="https://flagcdn.com/256x192/id.png" width="48" alt="Indonesia Flag"></a>
    <a href="translations/uk/ПРОЧИТАЙМЕНЕ.md"><img src="https://flagcdn.com/256x192/ua.png" width="48" alt="Ukraine Flag"></a>
	<a href="translations/vi/README.md"><img src="https://flagcdn.com/256x192/vn.png" width="48" alt="Vietnamese Flag"></a>

## 

Openplace (styled lowercase) is a free unofficial open source backend for [wplace.](https://wplace.live) — a Rust fork of the [original openplace](https://github.com/openplaceteam/openplace) (Node.js), rewritten for speed and hardened against automation. We aim to give the freedom and flexibility for all users to be able to make their own private wplace experience for themselves, their friends, or even their community — **on your terms**: built-in bot defense, self-hosted fingerprinting, and community rules you can change at runtime from the admin panel.

The backend is written in **Rust** (Axum + Tokio + SQLx) on top of **PostgreSQL** and serves the wplace HTTP API with a fully in-memory hot path: painted tiles are kept as color grids in RAM and served as indexed PNGs, charges and rewards are applied in a single atomic statement, and region statistics are written behind the request. It is a drop-in replacement for the original Node.js backend — same routes, same JSON contracts, same cookies.

> [WARNING ⚠️]
> This is a work-in-progress. Expect unfinished features and bugs. Please help us by posting issues in #tech-support on our [Discord server](https://discord.gg/ZRC4DnP9Z2) or by contributing pull requests. Thanks!

## Table of contents

- [Features](#features)
- [Performance](#performance)
- [Bot & automation defense](#bot--automation-defense-built-in-self-hosted)
- [Quick start (Docker)](#quick-start-docker)
- [Installation from source](#installation-from-source)
- [Configuration](#configuration)
- [Command-line reference](#command-line-reference)
- [Migrating from the legacy Node.js backend](#migrating-from-the-legacy-nodejs-backend)
- [API overview](#api-overview)
- [Benchmarks: how we measured](#benchmarks-how-we-measured)
- [Adding a translation](#adding-a-translation)
- [License](#license)

## Features

- 🤖 **Built-in bot & automation defense** — self-hosted fingerprinting, behavioral paint analysis, multi-account linking and PoW challenges. No SaaS, no data leaving your server
- 🦀 **Rust (Axum + Tokio + SQLx)** — multi-threaded, low-latency HTTP server
- 🖼️ **In-memory tile engine** — hot tiles live in RAM (1 MB color grid each) and are served as palette-indexed PNGs, no DB round trip per request
- ⚡ **Atomic paint pipeline** — charge regen, sufficiency check, level & droplet rewards in one `UPDATE … RETURNING`; batched pixel upserts; write-behind tile blobs & region stats
- 🗺️ **GeoNames regions** — KD-tree nearest-region lookup with per-pixel memoization, region/country leaderboards, autocomplete
- 🏆 **Leaderboards** — player / alliance / country / region boards for today / week / month / all-time, materialized views + realtime region boards
- 🛡️ **Moderation toolkit** — tickets with screenshots, bans with IP-range cascades, timeouts, admin/moderator panels, banned-IP list import
- 🛒 **Store & progression** — droplets currency, charges, paid palette, 251 flags, levels
- 💬 **Discord integration** — OAuth linking, role-based cooldown boosts, gateway bot, DM notifications
- 🔐 **Sessions & auth** — JWT cookies, bcrypt passwords, in-memory session cache, per-IP rate limiting
- 🐘 **PostgreSQL 17** — `ON CONFLICT` upserts, `UNIQUE NULLS NOT DISTINCT` stats keys, parallel aggregation

## Performance

Identical synthetic dataset (1,000 regions, 5,000 users, 20 tiles × 250k painted pixels = 5M rows), both stacks on the same machine, databases in Docker, same HTTP load generator (warm keep-alive connections, 20 s runs). Numbers from a Windows 11 dev host — the **proportions** are what matter:

| Scenario (concurrency) | Node.js + MariaDB | Rust + PostgreSQL | Speedup |
|---|---|---|---|
| `GET /health` — framework overhead (64) | 10,315 rps · p50 5.8 ms | **135,448 rps · p50 0.43 ms** | **13×** |
| `GET /files/s0/tiles/0/0.png` — hot tile (32) | 434 rps · p50 71.9 ms | **9,412 rps · p50 2.95 ms** | **22×** |
| Mixed read workload (32) | 521 rps · p50 60.6 ms | **5,177 rps · p50 0.85 ms** | **10×** |
| `GET /s0/pixel/…` — pixel info (32) | 3,202 rps · p50 9.4 ms | **6,445 rps · p50 4.7 ms** | 2× |
| `GET /me` — authenticated profile (50) | 2,063 rps · p50 23.7 ms | **7,224 rps · p50 6.6 ms** | 3.5× |
| `GET /leaderboard/player/all-time` (32) | 2,983 rps · p50 9.9 ms | 4,094 rps · p50 7.1 ms | 1.4× |
| `POST paint` 25 pixels (50 concurrent) | 4.5 rps · **45% 5xx** · p50 8.9 s | **434 rps · 0 errors · p50 111 ms** | **~97×** |
| `POST paint` 25 pixels (8 concurrent) | 4.2 rps · p50 2.0 s | **401 rps · 0 errors · p50 18.4 ms** | **~95×** |

In paint throughput terms: the legacy stack sustains ~105 painted pixels/second under concurrency, the Rust stack sustains **~10,000 painted pixels/second** with zero failures. Under 50 concurrent painters the Node backend starts returning HTTP 500s (Prisma write conflicts on the hot user row) and latency degrades to seconds — the Rust backend holds p99 under 270 ms.

### Why it is structurally better — not just faster

The speed is a symptom, not the goal. The rewrite removes the classes of
problems a Node.js + ORM backend runs into at scale:

| | Node.js original | openplace (Rust) |
|---|---|---|
| Paint transaction | multi-step Prisma transaction, `SELECT … FOR UPDATE`, retry loops, configurable timeouts needed under load | one atomic `UPDATE … WHERE charges_left >= cost RETURNING` — nothing to time out |
| Tile pipeline | decode → canvas → sharp re-quantize per paint, blob rewritten from DB | in-RAM color grid is the source of truth; indexed PNG re-encoded in ~1–3 ms; self-healing reconcile loop rebuilds any tile whose pixel rows are newer than its blob (crash-safe) |
| Per-request DB load | session + user re-read from DB on every request | TTL caches for sessions, users, regions, settings |
| Docker | npm install + prisma generate/db push at container boot, proxy starts before app is ready | single static binary, built-in healthcheck probe, Caddy waits for a healthy app |
| Bot defense | none built in | self-hosted fingerprinting + behavioral scoring + PoW (see above), tunable at runtime |

### Known upstream pain points — addressed in this fork

Real issues from the original project's tracker, and where they stand here:

| Upstream issue | Status in openplace (Rust) |
|---|---|
| ["68 econnrefused on local hosting"](https://github.com/openplaceteam/openplace/issues/68) | Streamlined compose: single binary, built-in healthcheck, strict startup ordering |
| ["66 Docker configuration needs complete revamp"](https://github.com/openplaceteam/openplace/issues/66) | Rewritten from scratch: Postgres 17 + healthchecks, multi-stage Rust build, **zero** boot-time npm/prisma steps |
| ["58 502 Bad Gateway after Docker setup"](https://github.com/openplaceteam/openplace/issues/58) | Caddy now waits for `condition: service_healthy` on the app — it physically cannot proxy a cold backend |
| ["59 Backend doesn't always render tile"](https://github.com/openplaceteam/openplace/issues/59) | Deterministic pipeline: RAM grid as source of truth + a reconcile loop (every 5 min) that rebuilds any tile whose pixels are newer than its stored blob |
| ["51 Prisma transaction timeout under heavy workloads"](https://github.com/openplaceteam/openplace/issues/51) | No Prisma, no long transactions to tune — the hot path is a single statement; ~10k painted px/s sustained with zero errors |
| ["67 moderator page and overlay needed"](https://github.com/openplaceteam/openplace/issues/67) | `/moderation` panel + the full moderator API ship, serve and are browser-tested against the real frontend |
| ["45 Discord linking does not work without a server"](https://github.com/openplaceteam/openplace/issues/45) | Linking is pure OAuth — no guild required; the bot's guild sync is optional and degrades gracefully |
| ["44 Discord username not removed on unlink"](https://github.com/openplaceteam/openplace/issues/44) | Fixed: unlink clears both `discord` and `discord_user_id` and resets the cooldown |
| ["55 IDs given to /moderator/users are… I don't know"](https://github.com/openplaceteam/openplace/issues/55) | API shapes are documented in the README's API overview; IDs are explicit and preserved |

Still on the roadmap (not implemented yet, honestly): appeal endpoints
(`/report/appeal`, `/me/last-appeal` — upstream #56/#57) and canvas templates
(upstream #61).


## Quick start (Docker)

Requirements: [Docker](https://docs.docker.com/get-docker/) with Compose v2.

```sh
# 1. Clone with the frontend submodule
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace

# 2. Configure
cp .env.example .env
# → edit .env and set JWT_SECRET (required) and anything else you need

# 3. Run
docker compose up -d --build
```

Then initialize the database and import region data:

```sh
# create tables + system users
docker compose exec app openplace-backend setup

# import GeoNames city data (see "Region data" below)
docker compose exec app openplace-backend import-geonames /path/to/cities1000.zip
```

The API listens on `:3000` (behind Caddy on `:80`/`:443`), the Nuxt frontend on `127.0.0.1:3001`.

> [TIP]
> `setup`, `import-geonames` and friends are subcommands of the single
> `openplace-backend` binary — see the [command-line reference](#command-line-reference).

## Installation from source

Requirements: [Rust](https://rustup.rs/) 1.85+, [PostgreSQL](https://www.postgresql.org/) 15+ (17 recommended), optionally [Caddy](https://caddyserver.com/) for TLS/reverse proxy.

```sh
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace

# 1. Database
createdb openplace            # or use Docker: docker run -d --name openplace-pg \
                              #   -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres \
                              #   -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine

# 2. Environment
cp .env.example .env
# → set DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
# → set JWT_SECRET to a long random string

# 3. Build & run
cd backend-rs
cargo run --release -- setup          # migrations + system users
cargo run --release -- import-geonames cities1000.zip
cargo run --release -- serve          # HTTP API on BACKEND_PORT (default 3000)
```

> [NOTE]
> The server serves the `frontend/` directory (the git submodule) as static
> files, resolving it as `FRONTEND_DIR` (default `./frontend`) **relative to
> the working directory it is started from**. When running the binary from
> `backend-rs/`, set `FRONTEND_DIR=../frontend`.

### Region data

Region boundaries/cities come from the [GeoNames](https://download.geonames.org/export/dump/) dump. Download one of `cities500.zip` (smallest), `cities1000.zip`, `cities5000.zip` or `allCountries.zip` (largest) and import it — the importer accepts both the `.zip` and the raw TSV:

```sh
openplace-backend import-geonames cities1000.zip
```

Without region data the backend still works, but every pixel maps to the fallback region.

## Configuration

All configuration is environment-based (`.env`). The full annotated list lives in [.env.example](.env.example); the highlights:

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | — | PostgreSQL connection string (**required**) |
| `JWT_SECRET` | — | HS256 signing secret (**required**) |
| `BACKEND_PORT` | `3000` | HTTP port (`PORT` also honored) |
| `EXTERNAL_URL` | — | Public base URL (used in password-reset links) |
| `COOLDOWN_MS` | `30000` | Base charge recharge interval |
| `LEVEL_BASE_PIXEL` / `LEVEL_EXPONENT` | `30` / `0.65` | Level curve: `(pixels/30)^0.65 + 1` |
| `ENABLE_RATE_LIMIT` | `false` | Turn on per-IP rate limiting |
| `*_RATE_LIMIT_ATTEMPTS` / `*_RATE_LIMIT_MS` | see `.env.example` | Limits for login/signup/password-reset/paint |
| `BAN_ON_BANNED_IP` / `BLOCK_TOR` | `false` | Ban accounts painting from banned IPs / block Tor exits |
| `DISCORD_*` | — | OAuth + bot integration (optional) |
| `DB_MAX_CONNECTIONS` | `32` | PostgreSQL pool size |
| `SESSION_CACHE_TTL_MS` | `60000` | How long sessions are validated from RAM |
| `USER_CACHE_TTL_MS` | `30000` | User row cache TTL |
| `TILE_CACHE_MAX_TILES` | `512` | How many painted tiles stay in RAM |
| `TILE_FLUSH_MS` / `STATS_FLUSH_MS` | `500` / `1000` | Write-behind intervals for tile blobs & stats |
| `FRONTEND_HOST` / `FRONTEND_PORT` | `localhost` / `3001` | Nuxt frontend proxy target |
| `FRONTEND_DIR` | `./frontend` | Static frontend directory (relative to cwd) |
| `ANTI_BOT_MODE` | `log` | `off` / `log` / `enforce` — built-in bot defense |
| `ANTI_BOT_KEY` | derived from `JWT_SECRET` | HMAC key for visitor IDs |
| `ANTI_BOT_POW_BITS` | `18` | Proof-of-work difficulty |
| `ANTI_BOT_ENFORCE_THRESHOLD` | `100` | Score that blocks painting in `enforce` mode |

## Command-line reference

`openplace-backend` is a single binary with subcommands:

| Command | Description |
|---|---|
| `serve` | Run the HTTP server (default workload) |
| `setup` | Apply migrations and seed the system users |
| `import-geonames <file.zip\|file.txt>` | Import a GeoNames dump as regions |
| `import-ip-list <file> [--reason ip-list]` | Import banned IPs/CIDRs (one per line, `#` comments) |
| `system-notification <title> <message>` | Broadcast a system notification to all users |
| `redraw-tiles` | Re-render all tile PNGs from pixel rows |
| `init-leaderboard` | Initialize leaderboard views |
| `migrate-from-mysql <mysql-url> [--force]` | Import data from the legacy Node.js database |
| `seed-bench` | Seed synthetic data for load testing |

## Migrating from the legacy Node.js backend

Moving an existing community off the Node.js/MariaDB stack is a single command. Passwords (bcrypt) and even active login sessions carry over — if `JWT_SECRET` is unchanged, users stay logged in.

```sh
# 1. Set up the new backend (see Installation from source)
openplace-backend setup

# 2. Import everything from the old MariaDB/MySQL database
DATABASE_URL="postgres://…new…" \
  openplace-backend migrate-from-mysql "mysql://root:password@old-host:3306/openplace"

# 3. Start the new backend
openplace-backend serve
```

What is migrated: **users** (with password hashes), **pixels**, **tile PNG blobs**, **alliances** (members, invites, bans), **favorite locations**, **banned IPs**, **regions**, **tickets** (with screenshots), **user notes**, **leaderboard views**, **region statistics**, **notifications**, **profile pictures** and **sessions**. IDs are preserved.

The importer refuses to write into a non-empty target database unless you pass `--force`; re-running it is safe (it upserts).

## API overview

Every route is available with and without the `/api` prefix. Authentication is an HS256 JWT in the `j` HttpOnly cookie.

| Group | Highlights |
|---|---|
| `POST /login` `POST /register` `POST /auth/logout` `POST /auth/request-password-reset` `POST /auth/reset-password` | Account lifecycle |
| `GET /me` `POST /me/update` `DELETE /me` `GET/POST /me/profile-picture*` `DELETE /me/sessions` | Profile management |
| `POST /s0/pixel/{tileX}/{tileY}` | Paint pixels (batch, charged) |
| `GET /files/s0/tiles/{x}/{y}.png` | Tile images (304-aware, `Last-Modified`) |
| `GET /s0/pixel/{tileX}/{tileY}?x=&y=` | Who painted a pixel + region info |
| `GET /leaderboard/{player,alliance,country,region}/…` | Leaderboards |
| `POST/GET /alliance…` | Alliance create/join/leave/invite/ban/leaderboard |
| `POST /purchase` `POST /flag/equip/{id}` | Store (charges, palette, flags) |
| `GET /notification/…` | Notification inbox (+ system broadcasts) |
| `POST /report-user` `POST /admin/ban-user` | Moderation reports |
| `/admin/*` `/moderator/*` | Admin & moderator panels (HTML + JSON) |
| `GET /v1/autocomplete?text=` | Region autocomplete (GeoJSON) |
| `GET /health` `GET /checkrobots` `GET /challenge` | Utilities |

The authoritative reference is the original [Wplace protocol](protocol.md).

## Bot & automation defense (built-in, self-hosted)

The original wplace relies on a paid fingerprinting SaaS. openplace ships an
equivalent that is **entirely your own**: no third-party service, no data
leaving your server, and every layer is switchable from the admin panel at
`/admin/customize` — no restarts required.

| Layer | What it does | Works without JS? |
|---|---|---|
| **Fingerprint collector** | A self-contained script (`/fp.js`) is auto-injected into every served page — no frontend changes. Hashes canvas / WebGL / audio / font traits client-side; the server derives a stable **visitor ID** (`HMAC-SHA256` with a server-side key — clients cannot forge it). | partial |
| **Multi-account linking** | One visitor ID painting from several accounts is surfaced to admins in `/admin/users` (`fp_accounts`, `fp_linked_users`). Powers the `ALLOW_MULTI_ACCOUNT` rule. | no |
| **Behavioral scoring** | Every paint request is observed server-side: machine-regular request intervals, perfectly uniform batch sizes, automation UAs (`HeadlessChrome`, `Puppeteer`, `python-requests`, `navigator.webdriver`), missing fingerprints. | **yes** |
| **Proof-of-work challenge** | Flagged users clear their score by solving a SHA-256 PoW (`/fp/challenge`) — real users never notice; script farms burn CPU. | yes |

### Signals that raise a user bot score

| Signal | Weight |
|---|---|
| Machine-regular paint intervals (coefficient of variation < 0.08) | +40 |
| Perfectly uniform batch sizes across many requests | +25 |
| Headless / automation user agent, `navigator.webdriver` | +60 |
| High paint volume with no fingerprint ever collected | +20 |

### Enforcement

`ANTI_BOT_MODE` — `off` / `log` (default: observe, expose scores to admins) /
`enforce`: users above `ANTI_BOT_ENFORCE_THRESHOLD` get 403 on paint until they
solve a PoW. Admins and moderators are always exempt. All of this is editable
at runtime in `/admin/customize` — including flipping to `enforce` mid-attack.
When `ALLOW_BOTS=true` (community rules), keep `log`: bot communities stay
visible but are never blocked.

> [NOTE]
> No fingerprinting defeats a determined adversary with stealth browser
> automation — defense here raises the cost of mass automation and makes it
> visible to moderators, which is exactly what the charge system, reports and
> IP-ban cascades complete. Only derived hashes and coarse fields are stored
> (no raw canvas/audio data), keeping the stored data minimal.



### Running behind Cloudflare

The backend resolves the client IP exactly like the original Node backend:
`cf-connecting-ip` → `x-forwarded-for` (first entry) → socket address. IP bans,
rate limits and statistics are keyed by this resolved IP, so it works out of
the box behind Cloudflare (or any reverse proxy that sets these headers).

> [IMPORTANT]
> These headers are trusted unconditionally (same as the original backend),
> so direct origin access must be blocked — otherwise a client could spoof
> `cf-connecting-ip` and evade IP bans / rate limits. Restrict port 3000 to
> Cloudflare IP ranges, or put Caddy / Cloudflare Tunnel in front.

## Benchmarks: how we measured

The load generator ships with the backend (`backend-rs/src/bin/loadgen.rs`) — reproduce the table above with:

```sh
# seed identical data into both stacks
DATABASE_URL="postgres://…" backend-rs/target/release/openplace-backend seed-bench \
  --regions 1000 --users 5000 --tiles 20

# hammer either stack (example)
backend-rs/target/release/loadgen --url http://127.0.0.1:3900 \
  --scenario tile --tile 0,0 --conns 32 --duration 20
backend-rs/target/release/loadgen --url http://127.0.0.1:3100 \
  --scenario paint --paint 25 --logins 50 --conns 50 --duration 20
```

Fairness notes: the leaderboard endpoint is bounded by the same aggregate SQL both stacks run, hence the modest 1.4×; the paint numbers include the synchronous DB commit (charge deduction is durable in both stacks before responding).

## Adding a translation

> [WARNING ⚠️]
> Contributions made with AI will be rejected, and you **WILL** be banned from the repository. You must be proficient in the language you translate.

To contribute to this repository and translate the `README.md` and other installation files, please follow these steps.

### Change the version number at the top of this README to indicate a new language has been added

The version number is formatted as `X.XX`, where the first "X" represents the number of languages officially translated so far. The second set of "X"s after the period is changed whenever any modifications are made to the English version of the README.
This version number helps translators know when they need to update their existing translated content.

### Create a new folder in the `translations` directory named after your language’s ISO code

If you’re unsure what your ISO code is, you can check it [here](https://gist.githubusercontent.com/josantonius/b455e315bc7f790d14b136d61d9ae468/raw/416def351bc1f790d14b136d61d9ae468/language-codes.json) or simply search online. You are looking for a two-letter code such as `"en"` for English.

### Copy the English files into your new folder

Copy the English files from the `translations` folder and the main `README.md` into the folder you just created.
You should now have four files: `README.md` and three installation markdown (`.md`) files.

### Add the correct flag to both README

When creating a new translation, you must update **two** README files:

#### 1. **Original English README**

Add **only the flag of the country/language you are translating into** at the top.
This flag must link to your new translated README.

Use this template:

```html
<a href="translations/LANGUAGE_ISO_CODE/NAME_OF_YOUR_README.md"><img src="https://flagcdn.com/256x192/LANGUAGE_ISO_CODE.png" width="48" alt="NAME_OF_COUNTRY Flag"></a>
```

Replace the placeholders with the ISO code and country name for your translation.

#### 2. **Your Translated README**

At the top of your translated README, add **only the American flag**, linking back to the English README.

> [WARNING ⚠️]
> Flags in the English README must stay in alphabetical order by ISO code.

### Update links in the Getting Started section

In the **Getting Started** section, update the links so they point to your translated files.
If you’re unsure how to do this, refer to another language folder (for example, `fr`).

### Translate all files

Translate all the files completely and accurately.
Once you’ve finished, make a pull request. A contributor or user will verify your work.
**Do not forget:** the use of AI is strictly prohibited and will result in a permanent ban if detected.

### Verify your work

Click on **EVERY** link and flag. Each one must work correctly and lead to the appropriate file or website.
If something doesn't work out, fix it before submitting your pull request.
Once everything functions as expected, you can confidently open your pull request.
Remember: these guidelines will be reviewed for all translations to ensure full compliance.

## License

Licensed under the Apache License, version 2.0. Refer to [LICENSE.md](https://github.com/13MrBlackCat13/openplace/blob/main/LICENSE.md).

### Acknowledgements

Region data is from [GeoNames Gazetteer](https://download.geonames.org/export/dump/), and is licensed under a [Creative Commons Attribution 4.0 License](https://creativecommons.org/licenses/by/4.0/). The Data is provided “as is” without warranty or any representation of accuracy, timeliness or completeness.
