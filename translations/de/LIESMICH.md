# openplace

<p align="center">
  <a href="https://github.com/13MrBlackCat13/openplace/actions/workflows/ci.yml"><img src="https://github.com/13MrBlackCat13/openplace/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/13MrBlackCat13/openplace/actions/workflows/release.yml"><img src="https://github.com/13MrBlackCat13/openplace/actions/workflows/release.yml/badge.svg" alt="Release"></a>
  <img src="https://img.shields.io/badge/rust-stable-DEA584?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/PostgreSQL-17-4169E1?logo=postgresql&logoColor=white" alt="PostgreSQL 17">
  <a href="https://github.com/13MrBlackCat13/openplace/blob/main/LICENSE.md"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center"><strong>Translations</strong> v7.0</p>
<p align="center">
    <a href="../../README.md"><img src="https://flagcdn.com/256x192/us.png" width="48" alt="United States Flag"></a>

## 

Openplace (kleingeschrieben) ist ein freies, inoffizielles Open-Source-Backend für [wplace.](https://wplace.live) — ein Rust-Fork des [originalen openplace](https://github.com/openplaceteam/openplace) (Node.js), neu geschrieben für Geschwindigkeit und gehärtet gegen Automatisierung. Wir wollen allen Nutzern die Freiheit und Flexibilität geben, ihre eigene private wplace-Erfahrung zu gestalten — für sich selbst, ihre Freunde oder sogar ihre Community — **zu Ihren Bedingungen**: eingebaute Bot-Abwehr, selbst gehostetes Fingerprinting und Community-Regeln, die Sie zur Laufzeit über das Admin-Panel ändern können.

Das Backend ist in **Rust** (Axum + Tokio + SQLx) auf Basis von **PostgreSQL** geschrieben und stellt die wplace-HTTP-API mit einem vollständig speicherresidenten Hot-Path bereit: bemalte Tiles werden als Farb-Raster im RAM gehalten und als indizierte PNGs ausgeliefert, Ladungen und Belohnungen werden in einer einzigen atomaren Anweisung angewendet, und Regionsstatistiken werden asynchron hinter der Anfrage geschrieben. Es ist ein Drop-in-Ersatz für das ursprüngliche Node.js-Backend — dieselben Routen, dieselben JSON-Verträge, dieselben Cookies.

> [WARNUNG ⚠️]
> Dieses Projekt ist noch in Arbeit. Rechnen Sie mit unvollständigen Funktionen und Fehlern. Bitte helfen Sie uns, indem Sie Probleme im Kanal #tech-support auf unserem [Discord-Server](https://discord.gg/ZRC4DnP9Z2) melden oder Pull-Requests beisteuern. Danke!

## Inhaltsverzeichnis

- [Funktionen](#funktionen)
- [Performance](#performance)
- [Bot- und Automatisierungsabwehr](#bot--und-automatisierungsabwehr-integriert-selbst-gehostet)
- [Schnellstart (Docker)](#schnellstart-docker)
- [Installation aus dem Quellcode](#installation-aus-dem-quellcode)
- [Konfiguration](#konfiguration)
- [Kommandozeilen-Referenz](#kommandozeilen-referenz)
- [Migration vom bisherigen Node.js-Backend](#migration-vom-bisherigen-nodejs-backend)
- [API-Übersicht](#api-übersicht)
- [Benchmarks: Wie wir gemessen haben](#benchmarks-wie-wir-gemessen-haben)
- [Übersetzung hinzufügen](#übersetzung-hinzufügen)
- [Lizenz](#lizenz)

## Funktionen

- 🤖 **Eingebaute Bot- und Automatisierungsabwehr** — selbst gehostetes Fingerprinting, verhaltensbasierte Analyse des Malverhaltens, Verknüpfung von Multi-Accounts und PoW-Herausforderungen. Kein SaaS, keine Daten verlassen Ihren Server
- 🦀 **Rust (Axum + Tokio + SQLx)** — mehrthreadiger HTTP-Server mit geringer Latenz
- 🖼️ **Speicherresidente Tile-Engine** — häufig angefragte Tiles liegen im RAM (je 1 MB Farb-Raster) und werden als palettenindizierte PNGs ausgeliefert, ohne DB-Roundtrip pro Anfrage
- ⚡ **Atomare Paint-Pipeline** — Ladungsregeneration, Verfügbarkeitsprüfung sowie Level- und Droplet-Belohnungen in einem einzigen `UPDATE … RETURNING`; stapelweise Pixel-Upserts; Write-Behind für Tile-Blobs und Regionsstatistiken
- 🗺️ **GeoNames-Regionen** — KD-Tree-Suche der nächsten Region mit Pixel-Memoisierung, Regions-/Länder-Bestenlisten, Autovervollständigung
- 🏆 **Bestenlisten** — Spieler-/Allianz-/Landes-/Regions-Rankings für heute / Woche / Monat / Gesamtzeit, materialisierte Views + Echtzeit-Regionsbestenlisten
- 🛡️ **Moderations-Werkzeugkasten** — Tickets mit Screenshots, Banns mit IP-Bereichs-Kaskaden, Timeouts, Admin-/Moderator-Panels, Import von gebannten IPs
- 🛒 **Shop & Fortschritt** — Droplets-Währung, Ladungen, bezahlte Palette, 251 Flaggen, Level
- 💬 **Discord-Integration** — OAuth-Verknüpfung, rollenbasierte Cooldown-Boosts, Gateway-Bot, DM-Benachrichtigungen
- 🔐 **Sessions & Authentifizierung** — JWT-Cookies, bcrypt-Passwörter, In-Memory-Session-Cache, IP-basiertes Rate-Limiting
- 🐘 **PostgreSQL 17** — `ON CONFLICT`-Upserts, `UNIQUE NULLS NOT DISTINCT`-Statistikschlüssel, parallele Aggregation

## Performance

Identischer synthetischer Datensatz (1.000 Regionen, 5.000 Nutzer, 20 Tiles × 250.000 bemalte Pixel = 5 Millionen Zeilen), beide Stacks auf derselben Maschine, Datenbanken in Docker, derselbe HTTP-Lastgenerator (aufgewärmte Keep-Alive-Verbindungen, 20-Sekunden-Läufe). Zahlen von einem Windows-11-Entwicklungsrechner — entscheidend sind die **Proportionen**:

| Szenario (Parallelität) | Node.js + MariaDB | Rust + PostgreSQL | Beschleunigung |
|---|---|---|---|
| `GET /health` — Framework-Overhead (64) | 10.315 rps · p50 5,8 ms | **135.448 rps · p50 0,43 ms** | **13×** |
| `GET /files/s0/tiles/0/0.png` — heiße Tile (32) | 434 rps · p50 71,9 ms | **9.412 rps · p50 2,95 ms** | **22×** |
| Gemischte Lese-Last (32) | 521 rps · p50 60,6 ms | **5.177 rps · p50 0,85 ms** | **10×** |
| `GET /s0/pixel/…` — Pixel-Info (32) | 3.202 rps · p50 9,4 ms | **6.445 rps · p50 4,7 ms** | 2× |
| `GET /me` — authentifiziertes Profil (50) | 2.063 rps · p50 23,7 ms | **7.224 rps · p50 6,6 ms** | 3,5× |
| `GET /leaderboard/player/all-time` (32) | 2.983 rps · p50 9,9 ms | 4.094 rps · p50 7,1 ms | 1,4× |
| `POST paint` 25 Pixel (50 parallel) | 4,5 rps · **45 % 5xx** · p50 8,9 s | **434 rps · 0 Fehler · p50 111 ms** | **ca. 97×** |
| `POST paint` 25 Pixel (8 parallel) | 4,2 rps · p50 2,0 s | **401 rps · 0 Fehler · p50 18,4 ms** | **ca. 95×** |

In Bezug auf den Paint-Durchsatz: Der alte Stack schafft unter Last etwa 105 bemalte Pixel/Sekunde, der Rust-Stack schafft **ca. 10.000 bemalte Pixel/Sekunde** ohne einen einzigen Fehler. Bei 50 gleichzeitig malenden Nutzern beginnt das Node-Backend, HTTP-500-Fehler zurückzugeben (Prisma-Schreibkonflikte auf der heißen Nutzer-Zeile), und die Latenz steigt in den Sekundenbereich — das Rust-Backend hält p99 unter 270 ms.

### Warum es strukturell besser ist — nicht nur schneller

Die Geschwindigkeit ist ein Symptom, nicht das Ziel. Die Neuentwicklung beseitigt die Klassen von Problemen, in die ein Node.js-Backend mit ORM bei Skalierung läuft:

| | Node.js-Original | openplace (Rust) |
|---|---|---|
| Paint-Transaktion | mehrstufige Prisma-Transaktion, `SELECT … FOR UPDATE`, Retry-Schleifen, unter Last sind konfigurierbare Timeouts nötig | eine einzige atomare `UPDATE … WHERE charges_left >= cost RETURNING`-Anweisung — nichts, das einen Timeout haben könnte |
| Tile-Pipeline | Decode → Canvas → sharp-Re-Quantisierung bei jedem Paint, Blob wird aus der DB neu geschrieben | das Farb-Raster im RAM ist die Single Source of Truth; das indizierte PNG wird in ca. 1–3 ms neu kodiert; eine selbstheilende Abgleich-Schleife baut jede Tile neu auf, deren Pixel-Zeilen neuer als ihr Blob sind (crash-sicher) |
| DB-Last pro Anfrage | Session und Nutzer werden bei jeder Anfrage aus der DB gelesen | TTL-Caches für Sessions, Nutzer, Regionen, Einstellungen |
| Docker | npm install + prisma generate/db push beim Containerstart, der Proxy startet, bevor die App bereit ist | eine einzige statische Binärdatei, eingebauter Healthcheck, Caddy wartet auf eine gesunde App |
| Bot-Abwehr | nicht eingebaut | selbst gehostetes Fingerprinting + Verhaltensbewertung + PoW (siehe oben), zur Laufzeit einstellbar |

### Bekannte Probleme des Upstream-Projekts — in diesem Fork behoben

Echte Issues aus dem Issue-Tracker des Original-Projekts und ihr Stand hier:

| Upstream-Issue | Status in openplace (Rust) |
|---|---|
| [„68 econnrefused on local hosting“](https://github.com/openplaceteam/openplace/issues/68) | Straffes Compose: eine Binärdatei, eingebauter Healthcheck, strenge Startreihenfolge |
| [„66 Docker configuration needs complete revamp“](https://github.com/openplaceteam/openplace/issues/66) | Von Grund auf neu geschrieben: Postgres 17 + Healthchecks, Multi-Stage-Rust-Build, **null** npm/prisma-Schritte beim Start |
| [„58 502 Bad Gateway after Docker setup“](https://github.com/openplaceteam/openplace/issues/58) | Caddy wartet nun auf `condition: service_healthy` der App — sie kann ein kaltes Backend schlicht nicht proxen |
| [„59 Backend doesn't always render tile“](https://github.com/openplaceteam/openplace/issues/59) | Deterministische Pipeline: RAM-Raster als Single Source of Truth + eine Abgleich-Schleife (alle 5 Minuten), die jede Tile neu aufbaut, deren Pixel neuer als ihr gespeicherter Blob sind |
| [„51 Prisma transaction timeout under heavy workloads“](https://github.com/openplaceteam/openplace/issues/51) | Kein Prisma, keine langen Transaktionen zum Tunen — der Hot-Path ist eine einzige Anweisung; ca. 10.000 bemalte Pixel/s ohne Fehler |
| [„67 moderator page and overlay needed“](https://github.com/openplaceteam/openplace/issues/67) | Das `/moderation`-Panel samt vollständiger Moderator-API wird ausgeliefert, läuft und ist im Browser gegen das echte Frontend getestet |
| [„45 Discord linking does not work without a server“](https://github.com/openplaceteam/openplace/issues/45) | Die Verknüpfung ist reines OAuth — kein Server nötig; die Guild-Synchronisation des Bots ist optional und degradiert gracefully |
| [„44 Discord username not removed on unlink“](https://github.com/openplaceteam/openplace/issues/44) | Behoben: Beim Trennen werden sowohl `discord` als auch `discord_user_id` gelöscht und der Cooldown zurückgesetzt |
| [„55 IDs given to /moderator/users are… I don't know“](https://github.com/openplaceteam/openplace/issues/55) | Die API-Formate sind in der API-Übersicht dieses READMEs dokumentiert; IDs sind explizit und bleiben erhalten |

Noch auf der Roadmap (ehrlich gesagt noch nicht implementiert): die Appeal-Endpunkte
(`/report/appeal`, `/me/last-appeal` — upstream #56/#57) und Canvas-Vorlagen
(upstream #61).


## Schnellstart (Docker)

Voraussetzungen: [Docker](https://docs.docker.com/get-docker/) mit Compose v2.

```sh
# 1. Inklusive Frontend-Submodul klonen
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace

# 2. Konfigurieren
cp .env.example .env
# → .env bearbeiten und JWT_SECRET (erforderlich) sowie alles Weitere nach Bedarf setzen

# 3. Starten
docker compose up -d --build
```

Danach die Datenbank initialisieren und die Regionsdaten importieren:

```sh
# Tabellen anlegen + Systemnutzer anlegen
docker compose exec app openplace-backend setup

# GeoNames-Städtedaten importieren (siehe „Regionsdaten“ unten)
docker compose exec app openplace-backend import-geonames /pfad/zu/cities1000.zip
```

Die API lauscht auf `:3000` (hinter Caddy auf `:80`/`:443`), das Nuxt-Frontend auf `127.0.0.1:3001`.

> [TIPP]
> `setup`, `import-geonames` und Konsorten sind Unterbefehle der einzigen
> Binärdatei `openplace-backend` — siehe die [Kommandozeilen-Referenz](#kommandozeilen-referenz).

## Installation aus dem Quellcode

Voraussetzungen: [Rust](https://rustup.rs/) 1.85+, [PostgreSQL](https://www.postgresql.org/) 15+ (17 empfohlen), optional [Caddy](https://caddyserver.com/) für TLS/Reverse-Proxy.

```sh
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace

# 1. Datenbank
createdb openplace            # oder Docker: docker run -d --name openplace-pg \
                              #   -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres \
                              #   -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine

# 2. Umgebung
cp .env.example .env
# → DATABASE_URL="postgres://postgres:password@localhost:5432/openplace" setzen
# → JWT_SECRET auf eine lange zufällige Zeichenkette setzen

# 3. Bauen & starten
cd backend-rs
cargo run --release -- setup          # Migrationen + Systemnutzer
cargo run --release -- import-geonames cities1000.zip
cargo run --release -- serve          # HTTP-API auf BACKEND_PORT (Standard 3000)
```

> [HINWEIS]
> Der Server stellt das Verzeichnis `frontend/` (das Git-Submodul) als statische
> Dateien bereit und löst es über `FRONTEND_DIR` (Standard `./frontend`) auf —
> **relativ zum Arbeitsverzeichnis, aus dem er gestartet wird**. Läuft die
> Binärdatei aus `backend-rs/`, muss `FRONTEND_DIR=../frontend` gesetzt werden.

### Regionsdaten

Die Regionsgrenzen/Städte stammen aus dem [GeoNames](https://download.geonames.org/export/dump/)-Dump. Laden Sie eine der Dateien `cities500.zip` (am kleinsten), `cities1000.zip`, `cities5000.zip` oder `allCountries.zip` (am größten) herunter und importieren Sie sie — der Importer akzeptiert sowohl die `.zip` als auch die rohe TSV-Datei:

```sh
openplace-backend import-geonames cities1000.zip
```

Ohne Regionsdaten funktioniert das Backend trotzdem, aber jedes Pixel wird dann der Fallback-Region zugeordnet.

## Konfiguration

Die gesamte Konfiguration erfolgt über Umgebungsvariablen (`.env`). Die vollständige kommentierte Liste steht in [.env.example](../../.env.example); die wichtigsten:

| Variable | Standard | Beschreibung |
|---|---|---|
| `DATABASE_URL` | — | PostgreSQL-Verbindungszeichenfolge (**erforderlich**) |
| `JWT_SECRET` | — | HS256-Signaturschlüssel (**erforderlich**) |
| `BACKEND_PORT` | `3000` | HTTP-Port (`PORT` wird ebenfalls berücksichtigt) |
| `EXTERNAL_URL` | — | Öffentliche Basis-URL (wird in Links zum Zurücksetzen des Passworts verwendet) |
| `COOLDOWN_MS` | `30000` | Basisintervall der Ladungsregeneration |
| `LEVEL_BASE_PIXEL` / `LEVEL_EXPONENT` | `30` / `0.65` | Level-Kurve: `(pixels/30)^0.65 + 1` |
| `ENABLE_RATE_LIMIT` | `false` | IP-basiertes Rate-Limiting einschalten |
| `*_RATE_LIMIT_ATTEMPTS` / `*_RATE_LIMIT_MS` | siehe `.env.example` | Limits für Login/Registrierung/Passwort-Reset/Paint |
| `BAN_ON_BANNED_IP` / `BLOCK_TOR` | `false` | Accounts sperren, die von gebannten IPs aus malen / Tor-Exit-Nodes blockieren |
| `DISCORD_*` | — | OAuth- + Bot-Integration (optional) |
| `DB_MAX_CONNECTIONS` | `32` | Größe des PostgreSQL-Pools |
| `SESSION_CACHE_TTL_MS` | `60000` | Wie lange Sessions aus dem RAM validiert werden |
| `USER_CACHE_TTL_MS` | `30000` | TTL des Nutzer-Zeilen-Cache |
| `TILE_CACHE_MAX_TILES` | `512` | Wie viele bemalte Tiles im RAM bleiben |
| `TILE_FLUSH_MS` / `STATS_FLUSH_MS` | `500` / `1000` | Write-Behind-Intervalle für Tile-Blobs und Statistiken |
| `FRONTEND_HOST` / `FRONTEND_PORT` | `localhost` / `3001` | Proxy-Ziel des Nuxt-Frontends |
| `FRONTEND_DIR` | `./frontend` | Statisches Frontend-Verzeichnis (relativ zum Arbeitsverzeichnis) |
| `ANTI_BOT_MODE` | `log` | `off` / `log` / `enforce` — eingebaute Bot-Abwehr |
| `ANTI_BOT_KEY` | aus `JWT_SECRET` abgeleitet | HMAC-Schlüssel für Besucher-IDs |
| `ANTI_BOT_POW_BITS` | `18` | Proof-of-Work-Schwierigkeit |
| `ANTI_BOT_ENFORCE_THRESHOLD` | `100` | Score, ab dem im `enforce`-Modus das Malen blockiert wird |

## Kommandozeilen-Referenz

`openplace-backend` ist eine einzige Binärdatei mit Unterbefehlen:

| Befehl | Beschreibung |
|---|---|
| `serve` | HTTP-Server starten (Standard-Arbeitsmodus) |
| `setup` | Migrationen anwenden und die Systemnutzer anlegen |
| `import-geonames <file.zip\|file.txt>` | GeoNames-Dump als Regionen importieren |
| `import-ip-list <file> [--reason ip-list]` | Gebannte IPs/CIDRs importieren (eine pro Zeile, `#`-Kommentare) |
| `system-notification <title> <message>` | Systembenachrichtigung an alle Nutzer senden |
| `redraw-tiles` | Alle Tile-PNGs aus den Pixel-Zeilen neu rendern |
| `init-leaderboard` | Bestenlisten-Views initialisieren |
| `migrate-from-mysql <mysql-url> [--force]` | Daten aus der bisherigen Node.js-Datenbank importieren |
| `seed-bench` | Synthetische Daten für Lasttests einspielen |

## Migration vom bisherigen Node.js-Backend

Eine bestehende Community vom Node.js-/MariaDB-Stack wegzuziehen ist ein einziger Befehl. Passwörter (bcrypt) und sogar aktive Login-Sessions werden übernommen — bleibt `JWT_SECRET` unverändert, bleiben die Nutzer eingeloggt.

```sh
# 1. Das neue Backend aufsetzen (siehe Installation aus dem Quellcode)
openplace-backend setup

# 2. Alles aus der alten MariaDB/MySQL-Datenbank importieren
DATABASE_URL="postgres://…neu…" \
  openplace-backend migrate-from-mysql "mysql://root:password@old-host:3306/openplace"

# 3. Das neue Backend starten
openplace-backend serve
```

Was migriert wird: **Nutzer** (mit Passwort-Hashes), **Pixel**, **Tile-PNG-Blobs**, **Allianzen** (Mitglieder, Einladungen, Banns), **Favoriten-Standorte**, **gebannte IPs**, **Regionen**, **Tickets** (mit Screenshots), **Nutzer-Notizen**, **Bestenlisten-Views**, **Regionsstatistiken**, **Benachrichtigungen**, **Profilbilder** und **Sessions**. IDs bleiben erhalten.

Der Importer weigert sich, in eine nicht-leere Zieldatenbank zu schreiben, sofern Sie nicht `--force` übergeben; ein erneuter Lauf ist gefahrlos (er führt Upserts aus).

## API-Übersicht

Jede Route ist mit und ohne den Präfix `/api` erreichbar. Die Authentifizierung erfolgt über einen HS256-JWT im HttpOnly-Cookie `j`.

| Gruppe | Highlights |
|---|---|
| `POST /login` `POST /register` `POST /auth/logout` `POST /auth/request-password-reset` `POST /auth/reset-password` | Kontolebenszyklus |
| `GET /me` `POST /me/update` `DELETE /me` `GET/POST /me/profile-picture*` `DELETE /me/sessions` | Profil-Verwaltung |
| `POST /s0/pixel/{tileX}/{tileY}` | Pixel malen (stapelweise, kostenpflichtig) |
| `GET /files/s0/tiles/{x}/{y}.png` | Tile-Bilder (304-fähig, `Last-Modified`) |
| `GET /s0/pixel/{tileX}/{tileY}?x=&y=` | Wer hat ein Pixel gemalt + Regionsinfo |
| `GET /leaderboard/{player,alliance,country,region}/…` | Bestenlisten |
| `POST/GET /alliance…` | Allianz: erstellen/beitreten/verlassen/einladen/bannen/Bestenliste |
| `POST /purchase` `POST /flag/equip/{id}` | Shop (Ladungen, Palette, Flaggen) |
| `GET /notification/…` | Benachrichtigungseingang (+ Systemübertragungen) |
| `POST /report-user` `POST /admin/ban-user` | Moderationsmeldungen |
| `/admin/*` `/moderator/*` | Admin- & Moderator-Panels (HTML + JSON) |
| `GET /v1/autocomplete?text=` | Regions-Autovervollständigung (GeoJSON) |
| `GET /health` `GET /checkrobots` `GET /challenge` | Hilfsrouten |

Die verbindliche Referenz ist das originale [Wplace-Protokoll](../../protocol.md).

## Bot- und Automatisierungsabwehr (integriert, selbst gehostet)

Das originale wplace setzt auf eine kostenpflichtige Fingerprinting-SaaS. openplace liefert ein
Äquivalent mit, das **ganz und gar Ihr eigenes** ist: kein Drittanbieter-Dienst, keine Daten
verlassen Ihren Server, und jede Schicht lässt sich im Admin-Panel unter `/admin/customize`
umschalten — ganz ohne Neustart.

| Schicht | Was sie tut | Funktioniert ohne JS? |
|---|---|---|
| **Fingerprint-Sammler** | Ein eigenständiges Skript (`/fp.js`) wird automatisch in jede ausgelieferte Seite injiziert — keine Frontend-Änderungen nötig. Es hasht Canvas-/WebGL-/Audio-/Font-Merkmale clientseitig; der Server leitet daraus eine stabile **Besucher-ID** ab (`HMAC-SHA256` mit einem serverseitigen Schlüssel — Clients können sie nicht fälschen). | teilweise |
| **Multi-Account-Verknüpfung** | Eine Besucher-ID, die von mehreren Accounts aus malt, wird den Admins in `/admin/users` angezeigt (`fp_accounts`, `fp_linked_users`). Sie treibt die `ALLOW_MULTI_ACCOUNT`-Regel an. | nein |
| **Verhaltensbewertung** | Jede Paint-Anfrage wird serverseitig beobachtet: maschinell regelmäßige Anfrageabstände, völlig gleichförmige Batch-Größen, Automatisierungs-User-Agents (`HeadlessChrome`, `Puppeteer`, `python-requests`, `navigator.webdriver`), fehlende Fingerabdrücke. | **ja** |
| **Proof-of-Work-Herausforderung** | Markierte Nutzer löschen ihren Score, indem sie ein SHA-256-PoW lösen (`/fp/challenge`) — echte Nutzer merken nichts; Skript-Farmen verbrennen CPU-Zeit. | ja |

### Signale, die den Bot-Score eines Nutzers erhöhen

| Signal | Gewicht |
|---|---|
| Maschinell regelmäßige Maleabstände (Variationskoeffizient < 0,08) | +40 |
| Völlig gleichförmige Batch-Größen über viele Anfragen hinweg | +25 |
| Headless-/Automatisierungs-User-Agent, `navigator.webdriver` | +60 |
| Hohes Malvolumen, ohne dass je ein Fingerabdruck erfasst wurde | +20 |

### Durchsetzung

`ANTI_BOT_MODE` — `off` / `log` (Standard: beobachten, Admins die Scores anzeigen) /
`enforce`: Nutzer über `ANTI_BOT_ENFORCE_THRESHOLD` erhalten beim Malen einen 403, bis sie
ein PoW lösen. Admins und Moderatoren sind immer ausgenommen. All das lässt sich zur Laufzeit
in `/admin/customize` ändern — auch mitten in einem Angriff auf `enforce` umschalten.
Bei `ALLOW_BOTS=true` (Community-Regeln) lassen Sie `log` aktiviert: Bot-Communitys bleiben
sichtbar, werden aber nie blockiert.

> [HINWEIS]
> Kein Fingerprinting schlägt einen entschlossenen Gegner mit Stealth-Browser-Automatisierung — die Abwehr hier erhöht die Kosten massiver Automatisierung und macht sie für Moderatoren sichtbar; genau dafür ergänzen das Ladungssystem, Meldungen und IP-Bann-Kaskaden das Bild. Gespeichert werden nur abgeleitete Hashes und grobe Felder (keine rohen Canvas-/Audio-Daten), sodass die gespeicherten Daten minimal bleiben.



### Betrieb hinter Cloudflare

Das Backend löst die Client-IP genauso auf wie das originale Node-Backend:
`cf-connecting-ip` → `x-forwarded-for` (erster Eintrag) → Socket-Adresse. IP-Banns,
Rate-Limits und Statistiken werden anhand dieser aufgelösten IP vergeben, daher funktioniert
alles ohne weitere Konfiguration hinter Cloudflare (oder jedem Reverse-Proxy, der diese
Header setzt).

> [WICHTIG]
> Diesen Headern wird bedingungslos vertraut (genau wie beim Original-Backend), daher muss der direkte Zugriff auf den Origin blockiert werden — andernfalls könnte ein Client `cf-connecting-ip` fälschen und IP-Banns/Rate-Limits umgehen. Beschränken Sie Port 3000 auf die Cloudflare-IP-Bereiche oder schalten Sie Caddy / Cloudflare Tunnel davor.

## Benchmarks: Wie wir gemessen haben

Der Lastgenerator wird mit dem Backend ausgeliefert (`backend-rs/src/bin/loadgen.rs`) — reproduzieren Sie die obige Tabelle mit:

```sh
# identische Daten in beide Stacks einspielen
DATABASE_URL="postgres://…" backend-rs/target/release/openplace-backend seed-bench \
  --regions 1000 --users 5000 --tiles 20

# einen der beiden Stacks belasten (Beispiel)
backend-rs/target/release/loadgen --url http://127.0.0.1:3900 \
  --scenario tile --tile 0,0 --conns 32 --duration 20
backend-rs/target/release/loadgen --url http://127.0.0.1:3100 \
  --scenario paint --paint 25 --logins 50 --conns 50 --duration 20
```

Fairness-Hinweise: Der Bestenlisten-Endpunkt ist durch dasselbe Aggregat-SQL begrenzt, das beide Stacks ausführen — daher die bescheidenen 1,4×. Die Paint-Zahlen beinhalten den synchronen DB-Commit (der Ladungsabzug ist in beiden Stacks dauerhaft gespeichert, bevor geantwortet wird).

## Übersetzung hinzufügen

> [WARNUNG ⚠️]
> Beiträge, die mit KI erstellt wurden, werden abgelehnt, und Sie werden **mit Sicherheit** aus dem Repository gebannt. Sie müssen die Sprache, in die Sie übersetzen, fließend beherrschen.

Um zu diesem Repository beizutragen und die `README.md` sowie weitere Installationsdateien zu übersetzen, befolgen Sie bitte diese Schritte.

### Die Versionsnummer am oberen Rand dieses READMEs ändern, um anzuzeigen, dass eine neue Sprache hinzugefügt wurde

Die Versionsnummer hat das Format `X.XX`, wobei das erste „X“ für die Anzahl der bisher offiziell übersetzten Sprachen steht. Die zweite Gruppe von „X“-Zeichen nach dem Punkt wird geändert, sobald Anpassungen an der englischen Version des READMEs vorgenommen werden.
Diese Versionsnummer hilft Übersetzern zu erkennen, wann sie ihre bereits übersetzten Inhalte aktualisieren müssen.

### Einen neuen Ordner im Verzeichnis `translations` mit dem ISO-Code Ihrer Sprache anlegen

Wenn Sie nicht sicher sind, wie Ihr ISO-Code lautet, können Sie ihn [hier](https://gist.githubusercontent.com/josantonius/b455e315bc7f790d14b136d61d9ae468/raw/416def351bc1f790d14b136d61d9ae468/language-codes.json) nachschlagen oder einfach online danach suchen. Gesucht ist ein zweibuchstabiger Code wie `"en"` für Englisch.

### Die englischen Dateien in Ihren neuen Ordner kopieren

Kopieren Sie die englischen Dateien aus dem Ordner `translations` und die Haupt-`README.md` in den soeben erstellten Ordner.
Sie sollten nun vier Dateien besitzen: `README.md` und drei Installations-Markdown-Dateien (`.md`).

### Die richtige Flagge in beiden READMEs ergänzen

Beim Anlegen einer neuen Übersetzung müssen **zwei** README-Dateien aktualisiert werden:

#### 1. **Das originale englische README**

Fügen Sie oben **nur die Flagge des Landes/der Sprache hinzu, in die Sie übersetzen**.
Diese Flagge muss auf Ihr neu übersetztes README verlinken.

Verwenden Sie diese Vorlage:

```html
<a href="translations/LANGUAGE_ISO_CODE/NAME_OF_YOUR_README.md"><img src="https://flagcdn.com/256x192/LANGUAGE_ISO_CODE.png" width="48" alt="NAME_OF_COUNTRY Flag"></a>
```

Ersetzen Sie die Platzhalter durch den ISO-Code und den Ländernamen Ihrer Übersetzung.

#### 2. **Ihr übersetztes README**

Fügen Sie oben in Ihrem übersetzten README **nur die amerikanische Flagge** hinzu, die zurück auf das englische README verlinkt.

> [WARNUNG ⚠️]
> Die Flaggen im englischen README müssen alphabetisch nach ISO-Code sortiert bleiben.

### Links im Abschnitt „Erste Schritte“ aktualisieren

Aktualisieren Sie im Abschnitt **Erste Schritte** die Links so, dass sie auf Ihre übersetzten Dateien verweisen.
Wenn Sie unsicher sind, wie das geht, orientieren Sie sich an einem anderen Sprachordner (zum Beispiel `fr`).

### Alle Dateien übersetzen

Übersetzen Sie alle Dateien vollständig und korrekt.
Wenn Sie fertig sind, erstellen Sie einen Pull-Request. Ein Beitragender oder Nutzer wird Ihre Arbeit prüfen.
**Nicht vergessen:** Die Verwendung von KI ist strengstens untersagt und führt bei Entdeckung zu einem dauerhaften Bann.

### Ihre Arbeit überprüfen

Klicken Sie auf **JEDEN** Link und jede Flagge. Jeder davon muss korrekt funktionieren und zur passenden Datei oder Website führen.
Wenn etwas nicht funktioniert, beheben Sie es, bevor Sie Ihren Pull-Request einreichen.
Sobald alles wie erwartet funktioniert, können Sie Ihren Pull-Request bedenkenlos öffnen.
Denken Sie daran: Diese Richtlinien werden bei allen Übersetzungen überprüft, um volle Einhaltung sicherzustellen.

## Lizenz

Lizenziert unter der Apache License, Version 2.0. Siehe [LICENSE.md](https://github.com/13MrBlackCat13/openplace/blob/main/LICENSE.md).

### Danksagungen

Die Regionsdaten stammen aus dem [GeoNames Gazetteer](https://download.geonames.org/export/dump/) und sind unter einer [Creative-Commons-Namensnennung-4.0-Lizenz](https://creativecommons.org/licenses/by/4.0/) lizenziert. Die Daten werden „wie besehen“ bereitgestellt, ohne Gewährleistung oder jegliche Zusicherung von Genauigkeit, Aktualität oder Vollständigkeit.
