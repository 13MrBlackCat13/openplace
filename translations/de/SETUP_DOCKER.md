# openplace — Installationsanweisungen unter Docker

Diese Anweisungen helfen Ihnen, **openplace** mit Docker zu betreiben.


## Installationsvoraussetzungen

- **Docker** mit **Compose v2** (Befehl `docker compose`)
- **Git** (zum Klonen inklusive Submodul)

### Docker installieren

-   **Windows**: Docker Desktop über [docker.com](https://www.docker.com/products/docker-desktop/) herunterladen
-   **macOS**: Docker Desktop über [docker.com](https://www.docker.com/products/docker-desktop/) herunterladen
-   **Linux**: Die Installationsanweisungen auf [docs.docker.com](https://docs.docker.com/engine/install/) für die eigene Distribution folgen

## 1. Die Repository klonen

```bash
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

> [HINWEIS]
> Wurde die Repository bereits ohne Submodule geklont, holen Sie das `frontend`-Submodul mit `git submodule update --init --recursive` nach.

## 2. Die Umgebung konfigurieren

```bash
cp .env.example .env
```

Die `.env`-Datei bearbeiten und mindestens Folgendes einstellen:

-   `JWT_SECRET` auf eine lange, zufällige Zeichenkette setzen (**erforderlich**)
-   bei Bedarf weitere Variablen anpassen (siehe Kommentare in `.env.example`)

## 3. Die Container starten

```bash
docker compose up -d --build
```

Gestartet werden:

-   **PostgreSQL 17** (Datenbank, mit Healthcheck)
-   **openplace-backend** (Rust-Backend, API auf Port 3000)
-   **Caddy** (Reverse Proxy auf den Ports 80/443)
-   das **Nuxt-Frontend** (erreichbar unter `127.0.0.1:3001`)

## 4. Die Datenbank initialisieren und Regionen importieren

Einmalig nach dem ersten Start ausführen:

```bash
# Tabellen anlegen + Systemnutzer anlegen
docker compose exec app openplace-backend setup

# GeoNames-Städtedaten importieren (Pfad zur heruntergeladenen Datei angeben)
docker compose exec app openplace-backend import-geonames /pfad/zu/cities1000.zip
```

Die Regionsdaten stammen aus dem [GeoNames](https://download.geonames.org/export/dump/)-Dump. Laden Sie eine der Dateien `cities500.zip` (am kleinsten), `cities1000.zip`, `cities5000.zip` oder `allCountries.zip` (am größten) herunter und geben Sie den Pfad zur Datei an. Der Importer akzeptiert sowohl die `.zip` als auch die rohe TSV-Datei.

> [HINWEIS]
> Ohne Regionsdaten läuft das Backend ebenfalls, aber jedes Pixel wird dann der Fallback-Region zugeordnet.

## 5. Auf die Anwendung zugreifen

-   API: `http://localhost:3000` (hinter Caddy: `http://localhost` bzw. `https://localhost` auf den Ports 80/443)
-   Nuxt-Frontend: `http://127.0.0.1:3001`

Für den Produktionseinsatz ein SSL-Zertifikat konfigurieren — Caddy besorgt TLS normalerweise automatisch (Let's Encrypt), sobald eine öffentliche Domain hinterlegt ist.

## Nützliche Befehle

```bash
docker compose logs -f app   # Logs des Backends verfolgen
docker compose down          # Alle Container stoppen
```

Eine vollständige Übersicht aller Unterbefehle (`import-ip-list`, `redraw-tiles`, `migrate-from-mysql` usw.) finden Sie in der [Kommandozeilen-Referenz](LIESMICH.md#kommandozeilen-referenz) des READMEs.
