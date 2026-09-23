# openplace — Installation aus dem Quellcode (Windows)

Diese Anweisungen helfen Ihnen, **openplace** unter Windows direkt aus dem Quellcode zu bauen und zu starten.

---

## 1. Installationsvoraussetzungen

-   **Rust** 1.85 oder neuer — über [rustup.rs](https://rustup.rs/) installieren oder in PowerShell:

```powershell
winget install Rustlang.Rustup
```

-   **Git**:

```powershell
winget install Git.Git
```

-   **PostgreSQL** 15 oder neuer (17 empfohlen) — entweder lokal installieren ([postgresql.org/download/windows](https://www.postgresql.org/download/windows/)) oder als Docker-Container verwenden (siehe Schritt 3)

> [HINWEIS]
> Nach der Rust-Installation ein **neues** Terminal öffnen, damit `cargo` gefunden wird. Prüfen mit: `cargo --version`

---

## 2. Die Repository klonen

```powershell
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

---

## 3. Die Datenbank vorbereiten

Entweder eine lokale Datenbank anlegen:

```powershell
createdb openplace
```

oder PostgreSQL schnell über Docker starten:

```powershell
docker run -d --name openplace-pg -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine
```

---

## 4. Die Umgebung konfigurieren

```powershell
Copy-Item .env.example .env
```

Die `.env`-Datei bearbeiten und mindestens setzen:

-   `DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"`
-   `JWT_SECRET` auf eine lange, zufällige Zeichenkette

Alternativ (oder zusätzlich) lassen sich Variablen für die aktuelle PowerShell-Sitzung direkt setzen:

```powershell
$env:DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
$env:JWT_SECRET="bitte-lange-zufaellige-zeichenkette-einsetzen"
$env:FRONTEND_DIR="../frontend"
```

> [WICHTIG]
> `$env:...` gilt nur für die aktuelle Sitzung. Dauerhaft gehört die Konfiguration in die `.env`-Datei.

---

## 5. Bauen und starten

```powershell
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
