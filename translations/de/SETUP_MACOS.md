# openplace — Installation aus dem Quellcode (macOS)

Diese Anweisungen helfen Ihnen, **openplace** unter macOS direkt aus dem Quellcode zu bauen und zu starten.

---

## 1. Installationsvoraussetzungen

-   **Homebrew** ([brew.sh](https://brew.sh))
-   **Rust** 1.85 oder neuer — über [rustup.rs](https://rustup.rs/) installieren:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

-   **PostgreSQL** 15 oder neuer (17 empfohlen):

```bash
brew install postgresql@17
brew services start postgresql@17
```

oder alternativ Docker (siehe Schritt 3)

> [HINWEIS]
> Nach der Rust-Installation ein **neues** Terminal öffnen (oder `source "$HOME/.cargo/env"` ausführen), damit `cargo` gefunden wird.

---

## 2. Die Repository klonen

```bash
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

---

## 3. Die Datenbank vorbereiten

Entweder eine lokale Datenbank anlegen:

```bash
createdb openplace
```

oder PostgreSQL über Docker starten:

```bash
docker run -d --name openplace-pg \
  -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine
```

---

## 4. Die Umgebung konfigurieren

```bash
cp .env.example .env
```

Die `.env`-Datei bearbeiten und mindestens setzen:

-   `DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"`
-   `JWT_SECRET` auf eine lange, zufällige Zeichenkette

Alternativ (oder zusätzlich) lassen sich Variablen für die aktuelle Terminal-Sitzung direkt setzen:

```bash
export DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
export JWT_SECRET="bitte-lange-zufaellige-zeichenkette-einsetzen"
export FRONTEND_DIR="../frontend"
```

> [WICHTIG]
> `export` gilt nur für die aktuelle Sitzung. Dauerhaft gehört die Konfiguration in die `.env`-Datei.

---

## 5. Bauen und starten

```bash
cd backend-rs

cargo run --release -- setup                            # Migrationen + Systemnutzer anlegen
cargo run --release -- import-geonames cities1000.zip   # Regionsdaten importieren (siehe README)
cargo run --release -- serve                            # HTTP-API auf BACKEND_PORT (Standard 3000)
```

> [HINWEIS]
> Der Server liefert das statische Frontend aus `FRONTEND_DIR` aus. Wird er aus `backend-rs/` gestartet, muss `FRONTEND_DIR=../frontend` gesetzt sein (siehe Schritt 4).

---

## 6. Auf die Anwendung zugreifen

-   API: `http://localhost:3000`
-   Frontend: über denselben Port ausgeliefert (statisch aus `frontend/`)

Für den Produktionseinsatz Caddy als Reverse Proxy mit TLS vorschalten (siehe Haupt-README, Abschnitt „Schnellstart (Docker)“).
