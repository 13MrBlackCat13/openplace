# openplace — Гайд налаштування для macOS

Цей гайд допоможе вам підготувати пристрій з **macOS** до запуску **openplace** з вихідного коду.

---

## Крок 1: Встановіть необхідні компоненти

Перевірте, чи є на вашій системі:

-   **Homebrew** — [brew.sh](https://brew.sh/)
-   **Git** — `xcode-select --install` або [git-scm.com](https://git-scm.com/download/mac)

Встановіть **Rust** 1.85+ через [rustup](https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

---

## Крок 2: Клонуйте репозиторій

```bash
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

---

## Крок 3: Підготуйте базу даних PostgreSQL

### Варіант A: PostgreSQL через Homebrew

```bash
brew install postgresql@17
brew services start postgresql@17
createdb openplace
```

### Варіант B: PostgreSQL через Docker

```bash
docker run -d --name openplace-pg \
  -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine
```

---

## Крок 4: Налаштуйте середовище

Скопіюйте `.env.example` під назвою `.env`:

```bash
cp .env.example .env
```

Відредагуйте `.env` та задайте `DATABASE_URL` і `JWT_SECRET` (довгий випадковий рядок). Або задайте змінні в поточній сесії терміналу:

```bash
export DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
export JWT_SECRET="довгий-випадковий-рядок"
```

> [ПОПЕРЕДЖЕННЯ ⚠️]
> Якщо ви вказуєте пароль у `DATABASE_URL`, екрануйте спеціальні символи, наведені в цій таблиці: [Percent-Encoding](https://developer.mozilla.org/en-US/docs/Glossary/Percent-encoding)

---

## Крок 5: Ініціалізуйте базу даних і запустіть сервер

```bash
cd backend-rs
cargo run --release -- setup            # міграції + системні користувачі
cargo run --release -- import-geonames cities1000.zip
cargo run --release -- serve            # HTTP API на BACKEND_PORT (за замовчуванням 3000)
```

> [ПРИМІТКА]
> Сервер роздає каталог `frontend/` (git-субмодуль) як статичні файли, визначаючи його через `FRONTEND_DIR` (за замовчуванням `./frontend`) **відносно робочого каталогу, з якого його запущено**. Оскільки ви запускаєте бінарник з `backend-rs/`, задайте `FRONTEND_DIR=../frontend`.

> [ПОРАДА]
> Без даних регіонів бекенд теж працює, але кожен піксель потрапляє до резервного регіону. Дампи міст доступні на [download.geonames.org](https://download.geonames.org/export/dump/).

---

## Запуск сервера

-   Для публіки/продакшн налаштуйте SSL-сертифікат (наприклад, поставте Caddy як зворотний проксі).
-   Для локального/приватного використання відкрийте:

```
http://localhost:3000
```
