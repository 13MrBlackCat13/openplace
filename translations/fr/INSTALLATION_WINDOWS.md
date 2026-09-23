# openplace — Guide d’installation Windows

Ce guide vous aide à compiler et exécuter **openplace** (backend Rust + PostgreSQL) depuis les sources sous **Windows**.

---

## 1. Installer les prérequis

Vous aurez besoin de **Rust 1.85+**, **Git** et **PostgreSQL 15+** (17 recommandé).

- Avec **winget** (Windows 10/11, PowerShell en tant qu’administrateur) :

```powershell
winget install Git.Git
winget install Rustlang.Rustup
```

Vous pouvez aussi installer Rust via [rustup.rs](https://rustup.rs/) (`rustup-init.exe` ; le composant « Desktop development with C++ » de Visual Studio Build Tools est requis).

- **PostgreSQL** : installez-le depuis [postgresql.org](https://www.postgresql.org/download/windows/), ou lancez-le simplement avec Docker :

```powershell
docker run -d --name openplace-pg -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine
```

---

## 2. Cloner le dépôt

```powershell
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

---

## 3. Créer la base de données

Avec une installation native de PostgreSQL (le mot de passe défini lors de l’installation vous sera demandé) :

```powershell
createdb -U postgres openplace
```

Si vous utilisez le conteneur Docker ci-dessus, la base `openplace` est déjà créée — passez à l’étape suivante.

---

## 4. Configurer l’environnement

```powershell
Copy-Item .env.example .env
```

Modifiez le fichier `.env` et définissez au minimum :

- `DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"`
- `JWT_SECRET` — une longue chaîne aléatoire sécurisée

Vous pouvez aussi (ou en complément) définir ces variables dans la session PowerShell courante :

```powershell
$env:DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
$env:JWT_SECRET="une-longue-chaine-aleatoire-securisee"
```

> [AVERTISSEMENT ⚠️]
> N’utilisez jamais le mot de passe d’exemple `password` ni un secret faible en production.

---

## 5. Compiler, initialiser et lancer

Le backend se compile et se lance depuis le dossier `backend-rs` :

```powershell
cd backend-rs

cargo run --release -- setup                            # migrations + utilisateurs système
cargo run --release -- import-geonames cities1000.zip   # données de régions (voir le README)

$env:FRONTEND_DIR="../frontend"   # voir la remarque ci-dessous
cargo run --release -- serve      # API HTTP sur BACKEND_PORT (3000 par défaut)
```

> [REMARQUE]
> Le serveur sert le dossier `frontend/` (le sous-module git) comme fichiers statiques, résolu via `FRONTEND_DIR` (`./frontend` par défaut) **par rapport au répertoire de travail depuis lequel il est lancé**. Comme ici le binaire est lancé depuis `backend-rs/`, il faut définir `FRONTEND_DIR=../frontend`.

> [ASTUCE]
> `setup`, `import-geonames` et `serve` sont des sous-commandes du binaire unique `openplace-backend` — voir la [Référence des commandes](LISEZMOI.md#référence-des-commandes).

---

## 6. Accéder à l’application

- **API** : `http://127.0.0.1:3000`
- Le backend sert aussi les fichiers statiques du frontend (`frontend/`) sur le même port.

Pour un accès public, placez [Caddy](https://caddyserver.com/) (ou un autre proxy inverse) devant le backend pour obtenir le HTTPS.

> [AVERTISSEMENT ⚠️]
> En production, configurez un certificat SSL : openplace doit être servi en HTTPS. Pour un usage local ou privé, `http://127.0.0.1:3000` suffit.

---

## Mettre à jour la base de données

Si le schéma change après une mise à jour du dépôt, réappliquez les migrations :

```powershell
cargo run --release -- setup
```
