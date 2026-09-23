# openplace — Гайд налаштування Windows

Цей гайд допоможе вам підготувати пристрій з **Windows** до запуску **openplace** з вихідного коду.

---

## 1. Встановіть необхідні компоненти

-   **Rust** 1.85+ — встановіть через [rustup](https://rustup.rs/) (PowerShell):

```powershell
winget install Rustlang.Rustup
```

Або завантажте `rustup-init.exe` зі сторінки [rustup.rs](https://rustup.rs/) і запустіть його.

-   **PostgreSQL** 15+ (рекомендовано 17): [postgresql.org/download/windows](https://www.postgresql.org/download/windows/) — під час встановлення задайте пароль користувача `postgres`
-   **Git**: [git-scm.com](https://git-scm.com/download/win)

Альтернатива локальному PostgreSQL — Docker:

```powershell
docker run -d --name openplace-pg -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine
```

---

## 2. Клонуйте репозиторій

```powershell
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

---

## 3. Налаштуйте середовище

Скопіюйте `.env.example` під назвою `.env`:

```powershell
Copy-Item .env.example .env
```

Відредагуйте `.env` та задайте `DATABASE_URL` і `JWT_SECRET` (довгий випадковий рядок). Або задайте змінні в поточній сесії PowerShell:

```powershell
$env:DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
$env:JWT_SECRET="довгий-випадковий-рядок"
```

> [ПОПЕРЕДЖЕННЯ ⚠️]
> Якщо ви вказуєте пароль у `DATABASE_URL`, екрануйте спеціальні символи, наведені в цій таблиці: [Percent-Encoding](https://developer.mozilla.org/en-US/docs/Glossary/Percent-encoding)

---

## 4. Ініціалізуйте базу даних і запустіть сервер

```powershell
cd backend-rs
cargo run --release -- setup                # міграції + системні користувачі
cargo run --release -- import-geonames cities1000.zip
cargo run --release -- serve                # HTTP API на BACKEND_PORT (за замовчуванням 3000)
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
