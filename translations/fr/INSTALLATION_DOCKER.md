# openplace — Guide d’installation avec Docker

Ce guide vous aide à exécuter **openplace** (backend Rust + PostgreSQL 17) avec Docker.

## Prérequis

Vous devez avoir **Docker** avec **Compose v2** (la commande `docker compose`) installé sur votre système.

- **Windows / macOS** : téléchargez Docker Desktop depuis [docker.com](https://www.docker.com/products/docker-desktop/) (Compose v2 est inclus)
- **Linux** : installez le moteur Docker depuis [docs.docker.com](https://docs.docker.com/engine/install/), puis le plugin Compose depuis [docs.docker.com/compose/install](https://docs.docker.com/compose/install/linux/)

Vérifiez votre installation :

```bash
docker compose version
```

## 1. Cloner le dépôt

Clonez le dépôt **avec le sous-module du frontend** :

```bash
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

## 2. Configurer l’environnement

Copiez le fichier d’exemple :

```bash
cp .env.example .env
```

Modifiez ensuite le fichier `.env` :

- définissez `JWT_SECRET` avec une longue chaîne aléatoire sécurisée (**obligatoire**) ;
- `DATABASE_URL` est déjà fourni par le `docker-compose.yml` (service `db`, utilisateur `postgres`, mot de passe `password` — changez-le dans le compose si besoin) ;
- ajustez les autres variables selon vos besoins (voir la section [Configuration](LISEZMOI.md#configuration) du README).

## 3. Démarrer la pile

Lancez l’ensemble des services avec Compose v2 :

```bash
docker compose up -d --build
```

Cela démarre quatre services :

- **db** — PostgreSQL 17, avec healthcheck ;
- **app** — le backend openplace (binaire Rust unique, healthcheck intégré) ;
- **caddy** — proxy inverse et TLS sur les ports **80** et **443** ; il attend que `app` soit en bonne santé (`condition: service_healthy`) avant de proxifier ;
- **frontend2** — le frontend Nuxt, publié sur `127.0.0.1:3001`.

## 4. Initialiser la base de données

Une fois les conteneurs démarrés, créez les tables et les utilisateurs système :

```bash
docker compose exec app openplace-backend setup
```

Importez ensuite les données de régions GeoNames (téléchargez par exemple `cities1000.zip` sur [download.geonames.org](https://download.geonames.org/export/dump/)) :

```bash
docker compose exec app openplace-backend import-geonames /chemin/vers/cities1000.zip
```

## 5. Accéder à l’application

| Service | Adresse |
|---|---|
| API (backend) | port `3000` (derrière Caddy sur `:80`/`:443`) |
| Proxy inverse Caddy | `http://localhost` / `https://localhost` |
| Frontend Nuxt (`frontend2`) | `http://127.0.0.1:3001` |

> [AVERTISSEMENT ⚠️]
> En production, openplace doit être servi en HTTPS. Dès qu’un nom de domaine pointe vers votre serveur, Caddy obtient et renouvelle automatiquement les certificats ; en local, `https://localhost` fonctionne aussi (Caddy émet un certificat local).

## Mettre à jour

```bash
git pull --recurse-submodules
docker compose up -d --build
docker compose exec app openplace-backend setup   # applique les nouvelles migrations
```
