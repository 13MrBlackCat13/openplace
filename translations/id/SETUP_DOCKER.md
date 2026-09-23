# openplace — Panduan Pengaturan Docker

Panduan ini akan membantu Anda menjalankan **openplace** dengan Docker.

## Persyaratan

Anda memerlukan **Docker** dengan **Compose v2** yang terinstal di sistem Anda.

### Instal Docker

-   **Windows**: Unduh Docker Desktop dari [docker.com](https://www.docker.com/products/docker-desktop/)
-   **macOS**: Unduh Docker Desktop dari [docker.com](https://www.docker.com/products/docker-desktop/)
-   **Linux**: Ikuti panduan instalasi untuk distro Anda di [docs.docker.com](https://docs.docker.com/engine/install/)

Compose v2 sudah termasuk dalam Docker Desktop; di Linux, instal plugin `docker-compose-plugin`. Perintahnya kini `docker compose` (tanpa tanda hubung), bukan `docker-compose`.

## 1. Clone repositorynya

```bash
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

Opsi `--recurse-submodules` penting: frontend Nuxt disertakan sebagai submodule git.

## 2. Atur environment

1. Salin `.env.example` ke `.env`:

```bash
cp .env.example .env
```

2. Edit file `.env` dan atur pengaturan Anda:
    - Atur `JWT_SECRET` ke string acak yang panjang (wajib)
    - Di dalam Compose, database berjalan sebagai layanan `db`, jadi `DATABASE_URL` dari `.env.example` sudah mengarah ke sana — sesuaikan password-nya bila perlu
    - Sesuaikan variabel lain sesuai kebutuhan (lihat komentar di `.env.example`)

> [PERINGATAN ⚠️]
> Gunakan password yang kuat. Jika password mengandung karakter khusus, ganti karakter tersebut sesuai tabel ini: [Percent-Encoding](https://developer.mozilla.org/en-US/docs/Glossary/Percent-encoding)

## 3. Jalankan seluruh stack

```bash
docker compose up -d --build
```

Ini akan membangun dan memulai:

-   **PostgreSQL 17** (dengan healthcheck)
-   **Backend Rust** (`openplace-backend`, satu binary dengan healthcheck bawaan)
-   **Caddy reverse proxy** di port 80/443, menunggu hingga aplikasi sehat

## 4. Inisialisasi database dan impor data wilayah

```bash
# buat tabel + pengguna sistem
docker compose exec app openplace-backend setup

# impor data kota GeoNames (unduh salah satu dari cities500.zip / cities1000.zip / cities5000.zip / allCountries.zip)
docker compose exec app openplace-backend import-geonames /path/to/cities1000.zip
```

> [TIPS]
> Tanpa data wilayah, backend tetap berfungsi, tetapi setiap piksel akan dipetakan ke wilayah fallback.

## 5. Akses aplikasinya

Setelah semua servis berjalan, Anda bisa mengakses openplace di:

```
http://localhost
https://localhost
```

-   API backend mendengarkan di `:3000` (di belakang Caddy pada `:80`/`:443`)
-   Frontend Nuxt tersedia di `127.0.0.1:3001`

> [PERINGATAN ⚠️]
> Untuk penggunaan produksi, konfigurasikan sertifikat SSL. openplace hanya dihosting melalui HTTPS — memuat situs melalui HTTP akan menghasilkan error HTTP 400.

## Perintah berguna

```bash
# melihat log
docker compose logs -f app

# menghentikan stack
docker compose down

# memperbarui (tarik kode terbaru lalu bangun ulang)
git pull --recurse-submodules
docker compose up -d --build
```
