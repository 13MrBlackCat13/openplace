# openplace — Panduan Pengaturan macOS

Panduan ini akan membantu Anda menyiapkan **macOS** untuk menjalankan **openplace**.

---

## Langkah 1: Instal Persyaratan

Pastikan Anda telah menginstal hal-hal berikut pada sistem Anda:
-   **Homebrew**
-   **Git**
-   **Rust** (via rustup)

```bash
brew install git
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

-   **PostgreSQL 15+** (disarankan 17) — install secara lokal:

```bash
brew install postgresql@17
brew services start postgresql@17
createdb openplace
```

    atau jalankan lewat Docker:

```bash
docker run -d --name openplace-pg -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine
```

---

## Langkah 2: Clone Repositorynya

```bash
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

Opsi `--recurse-submodules` penting: frontend Nuxt disertakan sebagai submodule git.

---

## Langkah 3: Atur environment

```bash
cp .env.example .env
```

Edit `.env` dan atur minimal:
-   `DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"` (sesuaikan dengan pengaturan PostgreSQL Anda)
-   `JWT_SECRET` ke string acak yang panjang

> [PERINGATAN ⚠️]
> Gunakan password yang kuat. Jika password mengandung karakter khusus, ganti karakter tersebut sesuai tabel ini: [Percent-Encoding](https://developer.mozilla.org/en-US/docs/Glossary/Percent-encoding)

---

## Langkah 4: Build dan siapkan database

Semua perintah berikut dijalankan dari folder `backend-rs`:

```bash
cd backend-rs
cargo run --release -- setup
```

Perintah `setup` menerapkan migrasi dan membuat pengguna sistem. Jalankan sekali — aman untuk dijalankan ulang.

Impor data wilayah GeoNames (unduh salah satu dari `cities500.zip`, `cities1000.zip`, `cities5000.zip` atau `allCountries.zip`):

```bash
cargo run --release -- import-geonames cities1000.zip
```

---

## Langkah 5: Jalankan servernya

```bash
export FRONTEND_DIR="../frontend"
export DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
cargo run --release -- serve
```

> [CATATAN]
> `FRONTEND_DIR` (bawaan `./frontend`) diselesaikan **relatif terhadap direktori kerja tempat binary dijalankan**. Karena Anda menjalankan dari `backend-rs/`, atur `FRONTEND_DIR=../frontend`.

API mendengarkan di `BACKEND_PORT` (bawaan `3000`); frontend Nuxt di `127.0.0.1:3001`.

---

## Catatan untuk SSL

Untuk produksi, konfigurasikan sertifikat SSL dan reverse proxy (misalnya Caddy) — openplace harus dihosting melalui HTTPS.
Untuk pengujian lokal, buka:

```
http://localhost:3000
```

---

## Memperbarui Aplikasi

Tarik perubahan terbaru dan jalankan kembali:

```bash
git pull --recurse-submodules
cd backend-rs
cargo run --release -- setup   # menerapkan migrasi baru
cargo run --release -- serve
```
