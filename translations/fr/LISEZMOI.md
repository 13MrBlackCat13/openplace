# ![openplace](../../banner.png "openplace banner")

<p align="center">
  <a href="https://github.com/13MrBlackCat13/openplace/actions/workflows/ci.yml"><img src="https://github.com/13MrBlackCat13/openplace/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/13MrBlackCat13/openplace/actions/workflows/release.yml"><img src="https://github.com/13MrBlackCat13/openplace/actions/workflows/release.yml/badge.svg" alt="Release"></a>
  <img src="https://img.shields.io/badge/rust-stable-DEA584?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/PostgreSQL-17-4169E1?logo=postgresql&logoColor=white" alt="PostgreSQL 17">
  <a href="../../LICENSE.md"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center"><strong>Translations</strong> v7.0</p>
<p align="center">
    <a href="../../README.md"><img src="https://flagcdn.com/256x192/us.png" width="48" alt="United States Flag"></a>

## 

Openplace (stylisé en minuscules) est un backend libre, non officiel et open source pour [wplace.](https://wplace.live) — un fork Rust de [l’openplace d’origine](https://github.com/openplaceteam/openplace) (Node.js), réécrit pour la rapidité et durci contre l’automatisation. Notre objectif est d’offrir à tous les utilisateurs la liberté et la flexibilité nécessaires pour créer leur propre expérience wplace privée — pour eux-mêmes, leurs amis ou même leur communauté — **selon vos termes** : défense intégrée contre les bots, fingerprinting auto-hébergé et règles communautaires modifiables à l’exécution depuis le panneau d’administration.

Le backend est écrit en **Rust** (Axum + Tokio + SQLx) au-dessus de **PostgreSQL** et sert l’API HTTP de wplace avec un chemin critique entièrement en mémoire : les Tuiles peintes sont conservées sous forme de grilles de couleurs en RAM et servies sous forme de PNG indexés, les charges et les récompenses sont appliquées en une seule instruction atomique, et les statistiques de régions sont écrites en arrière-plan de la requête. Il s’agit d’un remplaçant direct de l’ancien backend Node.js — mêmes routes, mêmes contrats JSON, mêmes cookies.

> [AVERTISSEMENT ⚠️]
> Ce projet est en cours de développement. Attendez-vous à des fonctionnalités incomplètes et à des bugs. Merci de nous aider en signalant les problèmes dans le canal #tech-support sur notre [serveur Discord](https://discord.gg/ZRC4DnP9Z2) ou en proposant des *pull requests*. Merci !

## Sommaire

- [Fonctionnalités](#fonctionnalités)
- [Performances](#performances)
- [Défense contre les bots et l’automatisation (intégrée, auto-hébergée)](#défense-contre-les-bots-et-lautomatisation-intégrée-auto-hébergée)
- [Démarrage rapide (Docker)](#démarrage-rapide-docker)
- [Installation à partir des sources](#installation-à-partir-des-sources)
- [Configuration](#configuration)
- [Référence des commandes](#référence-des-commandes)
- [Migration depuis l’ancien backend Node.js](#migration-depuis-lancien-backend-nodejs)
- [Aperçu de l’API](#aperçu-de-lapi)
- [Benchmarks : notre méthodologie](#benchmarks--notre-méthodologie)
- [Ajouter une traduction](#ajouter-une-traduction)
- [Licence](#licence)

## Fonctionnalités

- 🤖 **Défense intégrée contre les bots et l’automatisation** — fingerprinting auto-hébergé, analyse comportementale des peintures, liaison multi-comptes et défis PoW. Pas de SaaS, aucune donnée ne quitte votre serveur
- 🦀 **Rust (Axum + Tokio + SQLx)** — serveur HTTP multithread à faible latence
- 🖼️ **Moteur de Tuiles en mémoire** — les Tuiles actives vivent en RAM (grille de couleurs de 1 Mo chacune) et sont servies sous forme de PNG indexés par palette, sans aller-retour en base à chaque requête
- ⚡ **Pipeline de peinture atomique** — régénération des charges, vérification de solde, récompenses de niveau et de Droplets en un seul `UPDATE … RETURNING` ; upserts de pixels par lots ; blobs de Tuiles et statistiques de régions écrits en arrière-plan
- 🗺️ **Régions GeoNames** — recherche de la région la plus proche par KD-tree avec mémoïsation par pixel, classements par région/pays, autocomplétion
- 🏆 **Classements** — tableaux joueur / alliance / pays / région pour aujourd’hui / semaine / mois / tout temps, vues matérialisées + classements de régions en temps réel
- 🛡️ **Boîte à outils de modération** — tickets avec captures d’écran, bannissements avec cascades de plages d’IP, timeouts, panneaux admin/modérateur, import de listes d’IP bannies
- 🛒 **Boutique et progression** — monnaie en Droplets, charges, palette payante, 251 drapeaux, niveaux
- 💬 **Intégration Discord** — liaison OAuth, bonus de cooldown selon les rôles, bot gateway, notifications en message privé
- 🔐 **Sessions et authentification** — cookies JWT, mots de passe bcrypt, cache de sessions en mémoire, limitation de débit par IP
- 🐘 **PostgreSQL 17** — upserts `ON CONFLICT`, clés de statistiques `UNIQUE NULLS NOT DISTINCT`, agrégation parallèle

## Performances

Jeu de données synthétique identique (1 000 régions, 5 000 utilisateurs, 20 Tuiles × 250 000 pixels peints = 5 millions de lignes), les deux piles sur la même machine, bases de données dans Docker, même générateur de charge HTTP (connexions keep-alive préchauffées, exécutions de 20 s). Chiffres issus d’une machine de développement sous Windows 11 — ce sont les **proportions** qui comptent :

| Scénario (concurrence) | Node.js + MariaDB | Rust + PostgreSQL | Gain |
|---|---|---|---|
| `GET /health` — overhead du framework (64) | 10 315 rps · p50 5,8 ms | **135 448 rps · p50 0,43 ms** | **13×** |
| `GET /files/s0/tiles/0/0.png` — Tuile chaude (32) | 434 rps · p50 71,9 ms | **9 412 rps · p50 2,95 ms** | **22×** |
| Charge de lecture mixte (32) | 521 rps · p50 60,6 ms | **5 177 rps · p50 0,85 ms** | **10×** |
| `GET /s0/pixel/…` — infos pixel (32) | 3 202 rps · p50 9,4 ms | **6 445 rps · p50 4,7 ms** | 2× |
| `GET /me` — profil authentifié (50) | 2 063 rps · p50 23,7 ms | **7 224 rps · p50 6,6 ms** | 3,5× |
| `GET /leaderboard/player/all-time` (32) | 2 983 rps · p50 9,9 ms | 4 094 rps · p50 7,1 ms | 1,4× |
| `POST paint` 25 pixels (50 concurrents) | 4,5 rps · **45 % de 5xx** · p50 8,9 s | **434 rps · 0 erreur · p50 111 ms** | **~97×** |
| `POST paint` 25 pixels (8 concurrents) | 4,2 rps · p50 2,0 s | **401 rps · 0 erreur · p50 18,4 ms** | **~95×** |

En termes de débit de peinture : l’ancienne pile tient environ 105 pixels peints/seconde sous concurrence, tandis que la pile Rust maintient **environ 10 000 pixels peints/seconde** sans aucun échec. Avec 50 peintres concurrents, le backend Node commence à renvoyer des erreurs HTTP 500 (conflits d’écriture Prisma sur la ligne utilisateur « chaude ») et la latence se dégrade en secondes — le backend Rust maintient un p99 sous 270 ms.

### Pourquoi c’est structurellement meilleur — et pas seulement plus rapide

La vitesse est un symptôme, pas un objectif. La réécriture supprime les catégories de problèmes qu’un backend Node.js + ORM rencontre à grande échelle :

| | Node.js d’origine | openplace (Rust) |
|---|---|---|
| Transaction de peinture | transaction Prisma en plusieurs étapes, `SELECT … FOR UPDATE`, boucles de réessai, timeouts configurables indispensables sous charge | un seul `UPDATE … WHERE charges_left >= cost RETURNING` atomique — rien ne peut expirer |
| Pipeline de Tuiles | décodage → canvas → re-quantification sharp à chaque peinture, blob réécrit depuis la base | la grille de couleurs en RAM est la source de vérité ; PNG indexé ré-encodé en ~1–3 ms ; une boucle de réconciliation auto-réparatrice reconstruit toute Tuile dont les lignes de pixels sont plus récentes que son blob (résistant aux crashs) |
| Charge en base par requête | session + utilisateur relus en base à chaque requête | caches TTL pour les sessions, les utilisateurs, les régions et les réglages |
| Docker | npm install + prisma generate/db push au démarrage du conteneur, le proxy démarre avant que l’application soit prête | un seul binaire statique, sonde de healthcheck intégrée, Caddy attend une application en bonne santé |
| Défense anti-bots | aucune intégrée | fingerprinting auto-hébergé + score comportemental + PoW (voir ci-dessus), ajustable à l’exécution |

### Problèmes connus du projet d’origine — traités dans ce fork

Vrais problèmes issus du tracker du projet d’origine, et leur statut ici :

| Problème en amont | Statut dans openplace (Rust) |
|---|---|
| [« 68 econnrefused on local hosting »](https://github.com/openplaceteam/openplace/issues/68) | Compose rationalisé : un seul binaire, healthcheck intégré, ordre de démarrage strict |
| [« 66 Docker configuration needs complete revamp »](https://github.com/openplaceteam/openplace/issues/66) | Réécrit de zéro : Postgres 17 + healthchecks, build Rust multi-étapes, **zéro** étape npm/prisma au démarrage |
| [« 58 502 Bad Gateway after Docker setup »](https://github.com/openplaceteam/openplace/issues/58) | Caddy attend désormais `condition: service_healthy` sur `app` — il ne peut physiquement pas proxifier un backend froid |
| [« 59 Backend doesn't always render tile »](https://github.com/openplaceteam/openplace/issues/59) | Pipeline déterministe : grille en RAM comme source de vérité + une boucle de réconciliation (toutes les 5 min) qui reconstruit toute Tuile dont les pixels sont plus récents que le blob stocké |
| [« 51 Prisma transaction timeout under heavy workloads »](https://github.com/openplaceteam/openplace/issues/51) | Pas de Prisma, pas de longues transactions à régler — le chemin critique est une seule instruction ; ~10k pixels peints/s soutenus sans aucune erreur |
| [« 67 moderator page and overlay needed »](https://github.com/openplaceteam/openplace/issues/67) | Le panneau `/moderation` + l’API complète de modérateur sont livrés, servis et testés dans le navigateur avec le vrai frontend |
| [« 45 Discord linking does not work without a server »](https://github.com/openplaceteam/openplace/issues/45) | La liaison est du OAuth pur — aucun serveur Discord requis ; la synchronisation de serveur du bot est optionnelle et se dégrade proprement |
| [« 44 Discord username not removed on unlink »](https://github.com/openplaceteam/openplace/issues/44) | Corrigé : la déliaison efface à la fois `discord` et `discord_user_id` et réinitialise le cooldown |
| [« 55 IDs given to /moderator/users are… I don't know »](https://github.com/openplaceteam/openplace/issues/55) | Les formes de l’API sont documentées dans l’[Aperçu de l’API](#aperçu-de-lapi) de ce README ; les ID sont explicites et préservés |

Toujours sur la feuille de route (pas encore implémenté, soyons honnêtes) : les endpoints d’appel (`/report/appeal`, `/me/last-appeal` — en amont #56/#57) et les modèles de canvas (en amont #61).


## Démarrage rapide (Docker)

Guide détaillé : [Guide d’installation pour Docker](INSTALLATION_DOCKER.md).

Prérequis : [Docker](https://docs.docker.com/get-docker/) avec Compose v2.

```sh
# 1. Cloner avec le sous-module du frontend
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace

# 2. Configurer
cp .env.example .env
# → modifiez .env et définissez JWT_SECRET (obligatoire) ainsi que tout ce dont vous avez besoin

# 3. Lancer
docker compose up -d --build
```

Initialisez ensuite la base de données et importez les données de régions :

```sh
# créer les tables + les utilisateurs système
docker compose exec app openplace-backend setup

# importer les données de villes GeoNames (voir « Données de régions » plus bas)
docker compose exec app openplace-backend import-geonames /chemin/vers/cities1000.zip
```

L’API écoute sur `:3000` (derrière Caddy sur `:80`/`:443`), le frontend Nuxt (`frontend2`) sur `127.0.0.1:3001`.

> [ASTUCE]
> `setup`, `import-geonames` et leurs semblables sont des sous-commandes du
> binaire unique `openplace-backend` — voir la [référence des commandes](#référence-des-commandes).

## Installation à partir des sources

Guides détaillés : [Windows](INSTALLATION_WINDOWS.md) · [macOS](INSTALLATION_MACOS.md).

Prérequis : [Rust](https://rustup.rs/) 1.85+, [PostgreSQL](https://www.postgresql.org/) 15+ (17 recommandé), et optionnellement [Caddy](https://caddyserver.com/) pour TLS/proxy inverse.

```sh
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace

# 1. Base de données
createdb openplace            # ou via Docker : docker run -d --name openplace-pg \
                              #   -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres \
                              #   -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine

# 2. Environnement
cp .env.example .env
# → définissez DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
# → définissez JWT_SECRET avec une longue chaîne aléatoire

# 3. Compiler et lancer
cd backend-rs
cargo run --release -- setup          # migrations + utilisateurs système
cargo run --release -- import-geonames cities1000.zip
cargo run --release -- serve          # API HTTP sur BACKEND_PORT (3000 par défaut)
```

> [REMARQUE]
> Le serveur sert le dossier `frontend/` (le sous-module git) comme fichiers
> statiques, résolu via `FRONTEND_DIR` (`./frontend` par défaut) **par rapport
> au répertoire de travail depuis lequel il est lancé**. Quand vous exécutez
> le binaire depuis `backend-rs/`, définissez `FRONTEND_DIR=../frontend`.

### Données de régions

Les limites/villes des régions proviennent du dump [GeoNames](https://download.geonames.org/export/dump/). Téléchargez l’un des fichiers `cities500.zip` (le plus petit), `cities1000.zip`, `cities5000.zip` ou `allCountries.zip` (le plus grand) et importez-le — l’importeur accepte aussi bien le `.zip` que le TSV brut :

```sh
openplace-backend import-geonames cities1000.zip
```

Sans données de régions, le backend fonctionne quand même, mais chaque pixel est rattaché à la région de repli.

## Configuration

Toute la configuration passe par des variables d’environnement (`.env`). La liste complète et annotée se trouve dans [.env.example](../../.env.example) ; les points essentiels :

| Variable | Défaut | Description |
|---|---|---|
| `DATABASE_URL` | — | Chaîne de connexion PostgreSQL (**obligatoire**) |
| `JWT_SECRET` | — | Secret de signature HS256 (**obligatoire**) |
| `BACKEND_PORT` | `3000` | Port HTTP (`PORT` également accepté) |
| `EXTERNAL_URL` | — | URL publique de base (utilisée dans les liens de réinitialisation de mot de passe) |
| `COOLDOWN_MS` | `30000` | Intervalle de base de recharge des charges |
| `LEVEL_BASE_PIXEL` / `LEVEL_EXPONENT` | `30` / `0.65` | Courbe des niveaux : `(pixels/30)^0.65 + 1` |
| `ENABLE_RATE_LIMIT` | `false` | Active la limitation de débit par IP |
| `*_RATE_LIMIT_ATTEMPTS` / `*_RATE_LIMIT_MS` | voir `.env.example` | Limites pour connexion/inscription/réinitialisation de mot de passe/peinture |
| `BAN_ON_BANNED_IP` / `BLOCK_TOR` | `false` | Bannir les comptes peignant depuis des IP bannies / bloquer les nœuds de sortie Tor |
| `DISCORD_*` | — | Intégration OAuth + bot (optionnelle) |
| `DB_MAX_CONNECTIONS` | `32` | Taille du pool PostgreSQL |
| `SESSION_CACHE_TTL_MS` | `60000` | Durée pendant laquelle les sessions sont validées depuis la RAM |
| `USER_CACHE_TTL_MS` | `30000` | TTL du cache des lignes utilisateur |
| `TILE_CACHE_MAX_TILES` | `512` | Nombre de Tuiles peintes conservées en RAM |
| `TILE_FLUSH_MS` / `STATS_FLUSH_MS` | `500` / `1000` | Intervalles d’écriture en arrière-plan des blobs de Tuiles et des statistiques |
| `FRONTEND_HOST` / `FRONTEND_PORT` | `localhost` / `3001` | Cible du proxy pour le frontend Nuxt |
| `FRONTEND_DIR` | `./frontend` | Dossier statique du frontend (relatif au répertoire courant) |
| `ANTI_BOT_MODE` | `log` | `off` / `log` / `enforce` — défense intégrée contre les bots |
| `ANTI_BOT_KEY` | dérivé de `JWT_SECRET` | Clé HMAC pour les ID de visiteurs |
| `ANTI_BOT_POW_BITS` | `18` | Difficulté de la preuve de travail |
| `ANTI_BOT_ENFORCE_THRESHOLD` | `100` | Score qui bloque la peinture en mode `enforce` |

## Référence des commandes

`openplace-backend` est un binaire unique avec des sous-commandes :

| Commande | Description |
|---|---|
| `serve` | Lance le serveur HTTP (charge de travail par défaut) |
| `setup` | Applique les migrations et crée les utilisateurs système |
| `import-geonames <file.zip\|file.txt>` | Importe un dump GeoNames comme régions |
| `import-ip-list <file> [--reason ip-list]` | Importe des IP/CIDR bannis (un par ligne, commentaires avec `#`) |
| `system-notification <title> <message>` | Diffuse une notification système à tous les utilisateurs |
| `redraw-tiles` | Régénère tous les PNG de Tuiles à partir des lignes de pixels |
| `init-leaderboard` | Initialise les vues de classement |
| `migrate-from-mysql <mysql-url> [--force]` | Importe les données de l’ancienne base Node.js |
| `seed-bench` | Insère des données synthétiques pour les tests de charge |

## Migration depuis l’ancien backend Node.js

Migrer une communauté existante hors de la pile Node.js/MariaDB tient en une seule commande. Les mots de passe (bcrypt) et même les sessions de connexion actives sont conservés — si `JWT_SECRET` ne change pas, les utilisateurs restent connectés.

```sh
# 1. Mettre en place le nouveau backend (voir « Installation à partir des sources »)
openplace-backend setup

# 2. Tout importer depuis l’ancienne base MariaDB/MySQL
DATABASE_URL="postgres://…nouvelle…" \
  openplace-backend migrate-from-mysql "mysql://root:password@old-host:3306/openplace"

# 3. Démarrer le nouveau backend
openplace-backend serve
```

Ce qui est migré : les **utilisateurs** (avec les hachages de mots de passe), les **pixels**, les **blobs PNG de Tuiles**, les **alliances** (membres, invitations, bannissements), les **lieux favoris**, les **IP bannies**, les **régions**, les **tickets** (avec captures d’écran), les **notes utilisateur**, les **vues de classement**, les **statistiques de régions**, les **notifications**, les **photos de profil** et les **sessions**. Les ID sont préservés.

L’importeur refuse d’écrire dans une base cible non vide, sauf si vous passez `--force` ; le relancer est sans danger (il fait des upserts).

## Aperçu de l’API

Chaque route est disponible avec et sans le préfixe `/api`. L’authentification repose sur un JWT HS256 dans le cookie HttpOnly `j`.

| Groupe | Points marquants |
|---|---|
| `POST /login` `POST /register` `POST /auth/logout` `POST /auth/request-password-reset` `POST /auth/reset-password` | Cycle de vie du compte |
| `GET /me` `POST /me/update` `DELETE /me` `GET/POST /me/profile-picture*` `DELETE /me/sessions` | Gestion du profil |
| `POST /s0/pixel/{tileX}/{tileY}` | Peindre des pixels (par lot, payant en charges) |
| `GET /files/s0/tiles/{x}/{y}.png` | Images de Tuiles (gestion des 304, `Last-Modified`) |
| `GET /s0/pixel/{tileX}/{tileY}?x=&y=` | Qui a peint un pixel + infos de région |
| `GET /leaderboard/{player,alliance,country,region}/…` | Classements |
| `POST/GET /alliance…` | Alliance : créer/rejoindre/quitter/inviter/bannir/classement |
| `POST /purchase` `POST /flag/equip/{id}` | Boutique (charges, palette, drapeaux) |
| `GET /notification/…` | Boîte de réception des notifications (+ diffusions système) |
| `POST /report-user` `POST /admin/ban-user` | Signalements de modération |
| `/admin/*` `/moderator/*` | Panneaux admin et modérateur (HTML + JSON) |
| `GET /v1/autocomplete?text=` | Autocomplétion de régions (GeoJSON) |
| `GET /health` `GET /checkrobots` `GET /challenge` | Utilitaires |

La référence faisant autorité est le [protocole Wplace](../../protocol.md) d’origine.

## Défense contre les bots et l’automatisation (intégrée, auto-hébergée)

Le wplace d’origine s’appuie sur un SaaS de fingerprinting payant. openplace embarque un équivalent qui vous appartient **entièrement** : aucun service tiers, aucune donnée qui quitte votre serveur, et chaque couche est activable depuis le panneau d’administration sur `/admin/customize` — sans redémarrage.

| Couche | Ce qu’elle fait | Fonctionne sans JS ? |
|---|---|---|
| **Collecteur d’empreintes** | Un script autonome (`/fp.js`) est injecté automatiquement dans chaque page servie — aucune modification du frontend. Hache les empreintes canvas / WebGL / audio / polices côté client ; le serveur dérive un **ID de visiteur** stable (`HMAC-SHA256` avec une clé côté serveur — les clients ne peuvent pas le falsifier). | partielle |
| **Liaison multi-comptes** | Un ID de visiteur peignant depuis plusieurs comptes est signalé aux admins dans `/admin/users` (`fp_accounts`, `fp_linked_users`). Alimente la règle `ALLOW_MULTI_ACCOUNT`. | non |
| **Score comportemental** | Chaque requête de peinture est observée côté serveur : intervalles de requêtes d’une régularité mécanique, tailles de lots parfaitement uniformes, user-agents d’automatisation (`HeadlessChrome`, `Puppeteer`, `python-requests`, `navigator.webdriver`), empreintes manquantes. | **oui** |
| **Défi de preuve de travail** | Les utilisateurs signalés effacent leur score en résolvant un PoW SHA-256 (`/fp/challenge`) — les vrais utilisateurs ne s’en aperçoivent jamais ; les fermes de scripts brûlent du CPU. | oui |

### Signaux qui augmentent le score de bot d’un utilisateur

| Signal | Poids |
|---|---|
| Intervalles de peinture d’une régularité mécanique (coefficient de variation < 0,08) | +40 |
| Tailles de lots parfaitement uniformes sur de nombreuses requêtes | +25 |
| User-agent headless / d’automatisation, `navigator.webdriver` | +60 |
| Volume de peinture élevé sans qu’aucune empreinte n’ait jamais été collectée | +20 |

### Application des mesures

`ANTI_BOT_MODE` — `off` / `log` (défaut : observer, exposer les scores aux admins) / `enforce` : les utilisateurs au-dessus de `ANTI_BOT_ENFORCE_THRESHOLD` reçoivent un 403 à la peinture jusqu’à ce qu’ils résolvent un PoW. Les admins et les modérateurs sont toujours exemptés. Tout ceci est modifiable à l’exécution dans `/admin/customize` — y compris basculer en `enforce` en pleine attaque. Quand `ALLOW_BOTS=true` (règles communautaires), gardez `log` : les communautés de bots restent visibles mais ne sont jamais bloquées.

> [REMARQUE]
> Aucun fingerprinting ne vient à bout d’un adversaire déterminé muni d’une automatisation de navigateur furtive — la défense relève ici le coût de l’automatisation de masse et la rend visible pour les modérateurs, ce que complètent justement le système de charges, les signalements et les cascades de bannissements d’IP. Seuls les hachés dérivés et les champs grossiers sont stockés (aucune donnée canvas/audio brute), ce qui ramène les données conservées au strict minimum.



### Fonctionnement derrière Cloudflare

Le backend résout l’IP du client exactement comme l’ancien backend Node : `cf-connecting-ip` → `x-forwarded-for` (première entrée) → adresse du socket. Les bannissements d’IP, les limites de débit et les statistiques sont indexés sur cette IP résolue, donc tout fonctionne immédiatement derrière Cloudflare (ou tout proxy inverse qui définit ces en-têtes).

> [IMPORTANT]
> Ces en-têtes sont approuvés sans condition (comme dans le backend d’origine), donc l’accès direct à l’origine doit être bloqué — sinon un client pourrait falsifier `cf-connecting-ip` et contourner les bannissements d’IP / les limites de débit. Restreignez le port 3000 aux plages d’IP Cloudflare, ou placez Caddy / Cloudflare Tunnel devant.

## Benchmarks : notre méthodologie

Le générateur de charge est livré avec le backend (`backend-rs/src/bin/loadgen.rs`) — reproduisez le tableau ci-dessus avec :

```sh
# insérer des données identiques dans les deux piles
DATABASE_URL="postgres://…" backend-rs/target/release/openplace-backend seed-bench \
  --regions 1000 --users 5000 --tiles 20

# marteler l’une des piles (exemple)
backend-rs/target/release/loadgen --url http://127.0.0.1:3900 \
  --scenario tile --tile 0,0 --conns 32 --duration 20
backend-rs/target/release/loadgen --url http://127.0.0.1:3100 \
  --scenario paint --paint 25 --logins 50 --conns 50 --duration 20
```

Notes d’équité : l’endpoint de classement est borné par le même SQL d’agrégation que les deux piles exécutent, d’où le modeste 1,4× ; les chiffres de peinture incluent le commit synchrone en base (la déduction des charges est durable dans les deux piles avant de répondre).

## Ajouter une traduction

> [AVERTISSEMENT ⚠️]
> Les contributions réalisées à l’aide de l’IA seront rejetées, et vous **SEREZ** banni du dépôt. Vous devez maîtriser la langue que vous traduisez.

Pour contribuer à ce dépôt et traduire le `README.md` ainsi que les autres fichiers d’installation, veuillez suivre les étapes ci-dessous.

### Modifier le numéro de version en haut de ce README pour indiquer qu’une nouvelle langue a été ajoutée

Le numéro de version est au format `X.XX`, où le premier « X » représente le nombre de langues officiellement traduites à ce jour. Le second ensemble de « X » après le point change chaque fois qu’une modification est apportée à la version anglaise du README.
Ce numéro de version aide les traducteurs à savoir quand ils doivent mettre à jour leur contenu traduit existant.

### Créer un nouveau dossier dans le répertoire `translations` nommé d’après le code ISO de votre langue

Si vous ne connaissez pas votre code ISO, vous pouvez le vérifier [ici](https://gist.githubusercontent.com/josantonius/b455e315bc7f790d14b136d61d9ae468/raw/416def351bc1f790d14b136d61d9ae468/language-codes.json) ou simplement le rechercher en ligne. Vous cherchez un code à deux lettres, comme `"en"` pour l’anglais.

### Copier les fichiers anglais dans votre nouveau dossier

Copiez les fichiers anglais du dossier `translations` ainsi que le `README.md` principal dans le dossier que vous venez de créer.
Vous devriez maintenant avoir quatre fichiers : le `README.md` et trois fichiers d’installation au format markdown (`.md`).

### Ajouter le drapeau approprié aux deux README

Lors de la création d’une nouvelle traduction, vous devez mettre à jour **deux** fichiers README :

#### 1. **README original en anglais**

Ajoutez **uniquement le drapeau du pays/de la langue vers lequel vous traduisez** en haut du fichier.
Ce drapeau doit pointer vers votre nouveau README traduit.

Utilisez ce modèle :

```html
<a href="translations/LANGUAGE_ISO_CODE/NAME_OF_YOUR_README.md"><img src="https://flagcdn.com/256x192/LANGUAGE_ISO_CODE.png" width="48" alt="Drapeau de NAME_OF_COUNTRY"></a>
```

Remplacez les champs réservés par le code ISO et le nom du pays de votre traduction.

#### 2. **Votre README traduit**

En haut de votre README traduit, ajoutez **uniquement le drapeau américain**, qui renvoie vers le README anglais.

> [AVERTISSEMENT ⚠️]
> Les drapeaux dans le README anglais doivent rester classés par ordre alphabétique selon leur code ISO.

### Mettre à jour les liens dans la section Démarrage

Dans la section **Démarrage**, mettez à jour les liens afin qu’ils pointent vers vos fichiers traduits.
Si vous ne savez pas comment faire, consultez un autre dossier de langue (par exemple, `fr`).

### Traduire tous les fichiers

Traduisez tous les fichiers complètement et avec précision.
Une fois terminé, créez une pull request. Un contributeur ou un utilisateur vérifiera votre travail.
**N’oubliez pas :** l’utilisation de l’IA est strictement interdite et entraînera un bannissement permanent si elle est détectée.

### Vérifier votre travail

Cliquez sur **TOUS** les liens et drapeaux. Chacun doit fonctionner correctement et mener au fichier ou au site approprié.
Si quelque chose ne fonctionne pas, corrigez-le avant de soumettre votre pull request.
Une fois que tout fonctionne comme prévu, vous pouvez ouvrir votre pull request en toute confiance.
Souvenez-vous : ces directives seront vérifiées pour toutes les traductions afin d’assurer une conformité totale.

## Licence

Sous licence **Apache License, version 2.0**. Voir [LICENSE.md](https://github.com/13MrBlackCat13/openplace/blob/main/LICENSE.md).

### Remerciements

Les données de régions proviennent du [GeoNames Gazetteer](https://download.geonames.org/export/dump/), sous licence [Creative Commons Attribution 4.0](https://creativecommons.org/licenses/by/4.0/).
Les données sont fournies « telles quelles », sans garantie ni déclaration quant à leur exactitude, leur actualité ou leur exhaustivité.
