# openplace — Panduan Pengaturan Windows

Panduan ini akan membantu Anda menyiapkan **Windows** untuk menjalankan **openplace**.

---

## 1. Prasyarat Instal

Anda butuh **Rust**, **Git**, dan **PostgreSQL 15+** (disarankan 17).

-   Install **Rust** melalui rustup (PowerShell):

```powershell
winget install Rustlang.Rustup
rustup default stable
```

-   Install **Git**:

```powershell
winget install Git.Git
```

-   **PostgreSQL**: install secara lokal (versi 15+, disarankan 17) atau jalankan lewat Docker:

```powershell
docker run -d --name openplace-pg -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine
```

---

## 2. Clone repositorynya

```powershell
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

Opsi `--recurse-submodules` penting: frontend Nuxt disertakan sebagai submodule git.

---

## 3. Konfigurasikan environment

1. Salin `.env.example` ke `.env`:

```powershell
Copy-Item .env.example .env
```

2. Edit `.env` dan atur minimal:
    - `DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"` (sesuaikan dengan pengaturan PostgreSQL Anda)
    - `JWT_SECRET` ke string acak yang panjang

> [PERINGATAN ⚠️]
> Gunakan password yang kuat. Jika password mengandung karakter khusus, ganti karakter tersebut sesuai tabel ini: [Percent-Encoding](https://developer.mozilla.org/en-US/docs/Glossary/Percent-encoding)

---

## 4. Build dan siapkan database

Semua perintah berikut dijalankan dari folder `backend-rs`:

```powershell
cd backend-rs
cargo run --release -- setup
```

Perintah `setup` menerapkan migrasi dan membuat pengguna sistem. Jalankan sekali — aman untuk dijalankan ulang.

Impor data wilayah GeoNames (unduh salah satu dari `cities500.zip`, `cities1000.zip`, `cities5000.zip` atau `allCountries.zip`):

```powershell
cargo run --release -- import-geonames cities1000.zip
```

---

## 5. Jalankan servernya

```powershell
$env:FRONTEND_DIR="../frontend"
$env:DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
cargo run --release -- serve
```

> [CATATAN]
> Server menyajikan frontend dari `frontend/` sebagai berkas statis, diselesaikan melalui `FRONTEND_DIR` (bawaan `./frontend`) **relatif terhadap direktori kerja**. Karena binary dijalankan dari `backend-rs/`, atur `FRONTEND_DIR=../frontend` sebelum memulai.

API mendengarkan di `BACKEND_PORT` (bawaan `3000`); frontend Nuxt di `127.0.0.1:3001`.

---

## Mengaktifkan server Anda

-   Untuk produksi, konfigurasikan sertifikat SSL dan reverse proxy (misalnya Caddy).
-   Untuk penggunaan lokal/pribadi, buka:

```
http://localhost:3000
```

> [PERINGATAN ⚠️]
> Untuk penggunaan produksi, openplace harus dihosting melalui HTTPS.

---

## Memperbarui aplikasi

Tarik perubahan terbaru dan jalankan kembali:

```powershell
git pull --recurse-submodules
cd backend-rs
cargo run --release -- setup   # menerapkan migrasi baru
cargo run --release -- serve
```
