# openplace — Гайд налаштування Docker

Цей гайд допоможе вам налаштувати **openplace** за допомогою Docker.

## Вимоги

Потрібен **Docker** з **Compose v2** (команда `docker compose`, а не стара `docker-compose`).

### Встановлення Docker

-   **Windows**: встановіть Docker Desktop звідси: [docker.com](https://www.docker.com/products/docker-desktop/)
-   **macOS**: встановіть Docker Desktop звідси: [docker.com](https://www.docker.com/products/docker-desktop/)
-   **Linux**: дотримуйтеся інструкції встановлення Docker для вашого дистрибутива: [docs.docker.com](https://docs.docker.com/engine/install/)

## 1. Клонуйте репозиторій

Разом із фронтенд-субмодулем:

```bash
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

## 2. Налаштуйте середовище

1. Скопіюйте `.env.example` під назвою `.env`:

```bash
cp .env.example .env
```

2. Відкрийте `.env` і задайте потрібні значення:
    - `JWT_SECRET` — **обов'язково**; задайте довгий випадковий рядок для безпеки
    - `DATABASE_URL` — значення за замовчуванням уже коректне для Docker-стека (PostgreSQL у compose)
    - решту змінних — за потреби (повний анотований список дивіться в `.env.example`)

> [ПОПЕРЕДЖЕННЯ ⚠️]
> Якщо ви змінюєте `DATABASE_URL`, екрануйте спеціальні символи, наведені в цій таблиці: [Percent-Encoding](https://developer.mozilla.org/en-US/docs/Glossary/Percent-encoding)

## 3. Запустіть стек

Зберіть і запустіть усі сервіси:

```bash
docker compose up -d --build
```

Після старту працюватимуть:

-   **PostgreSQL 17** — база даних
-   **Бекенд на Rust** (контейнер `app`) — HTTP API на порту **3000**
-   **Caddy** — зворотний проксі на портах **80/443**
-   **Nuxt-фронтенд** — на `127.0.0.1:3001`

## 4. Ініціалізуйте базу даних

Створіть таблиці та системних користувачів:

```bash
docker compose exec app openplace-backend setup
```

## 5. Імпортуйте дані регіонів

Завантажте дамп міст GeoNames (наприклад, `cities1000.zip`) з [download.geonames.org](https://download.geonames.org/export/dump/) та імпортуйте його:

```bash
docker compose exec app openplace-backend import-geonames /path/to/cities1000.zip
```

> [ПОРАДА]
> `setup`, `import-geonames` та інші — це підкоманди єдиного бінарника `openplace-backend` всередині контейнера `app`.

## 6. Доступ до застосунку

Коли всі сервіси запустилися та працюють, openplace доступний за адресами:

```
http://localhost
https://localhost
```

-   HTTP API слухає на порту `:3000` (за Caddy на `:80`/`:443`)
-   Фронтенд (Nuxt) — на `127.0.0.1:3001`

> [ПРИМІТКА]
> Для публічного розгортання налаштуйте SSL-сертифікат: Caddy може отримувати сертифікати автоматично — вкажіть свій домен у `Caddyfile`.

## Корисні команди

```bash
docker compose logs -f app        # логи бекенду
docker compose restart app        # перезапуск бекенду
docker compose down               # зупинити стек
docker compose up -d --build      # перезібрати після оновлення коду
```
