# openplace

<p align="center">
  <a href="https://github.com/13MrBlackCat13/openplace/actions/workflows/ci.yml"><img src="https://github.com/13MrBlackCat13/openplace/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/13MrBlackCat13/openplace/actions/workflows/release.yml"><img src="https://github.com/13MrBlackCat13/openplace/actions/workflows/release.yml/badge.svg" alt="Release"></a>
  <img src="https://img.shields.io/badge/rust-stable-DEA584?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/PostgreSQL-17-4169E1?logo=postgresql&logoColor=white" alt="PostgreSQL 17">
  <a href="../../LICENSE.md"><img src="https://img.shields.io/badge/license-Apache--2.0-blue.svg" alt="License"></a>
</p>

<p align="center"><strong>Terjemahan</strong> v7.1</p>
<p align="center">
    <a href="../../README.md"><img src="https://flagcdn.com/256x192/us.png" width="48" alt="United States Flag"></a>

## 

Openplace (ditulis dengan huruf kecil) adalah backend open source tidak resmi yang gratis untuk [wplace.](https://wplace.live) — fork Rust dari [openplace](https://github.com/openplaceteam/openplace) asli (Node.js), ditulis ulang demi kecepatan dan diperkuat untuk melawan otomatisasi. Kami bertujuan untuk memberikan kebebasan dan fleksibilitas bagi semua pengguna agar dapat membangun pengalaman wplace pribadi mereka sendiri, baik untuk diri mereka sendiri, teman-teman mereka, bahkan komunitas mereka — **sesuai ketentuan Anda sendiri**: pertahanan bot bawaan, fingerprinting yang di-hosting sendiri, dan aturan komunitas yang bisa Anda ubah saat runtime dari panel admin.

Backend ditulis dalam **Rust** (Axum + Tokio + SQLx) di atas **PostgreSQL** dan melayani HTTP API wplace dengan hot path yang sepenuhnya in-memory: tile yang digambar disimpan sebagai grid warna di RAM dan disajikan sebagai PNG terindeks, charges dan reward diterapkan dalam satu pernyataan atomik, dan statistik wilayah ditulis secara asinkron (write-behind) di belakang permintaan. Ini adalah pengganti langsung (drop-in replacement) untuk backend Node.js asli — rute yang sama, kontrak JSON yang sama, cookie yang sama.

> [PERINGATAN ⚠️]
> Ini adalah proyek yang masih dalam pengembangan. Bersiaplah menemui fitur yang belum selesai dan bug. Bantu kami dengan melaporkan masalah di #tech-support pada [server Discord](https://discord.gg/ZRC4DnP9Z2) kami atau dengan berkontribusi melalui pull request. Terima kasih!

## Daftar isi

- [Memulai](#memulai)
- [Fitur](#fitur)
- [Performa](#performa)
- [Pertahanan bot & otomatisasi (bawaan, self-hosted)](#pertahanan-bot--otomatisasi-bawaan-self-hosted)
- [Mulai cepat (Docker)](#mulai-cepat-docker)
- [Instalasi dari sumber](#instalasi-dari-sumber)
- [Konfigurasi](#konfigurasi)
- [Referensi baris perintah](#referensi-baris-perintah)
- [Migrasi dari backend Node.js lama](#migrasi-dari-backend-nodejs-lama)
- [Ringkasan API](#ringkasan-api)
- [Benchmark: bagaimana kami mengukurnya](#benchmark-bagaimana-kami-mengukurnya)
- [Menambahkan terjemahan](#menambahkan-terjemahan)
- [Lisensi](#lisensi)

## Memulai

### Windows

- [Panduan Instalasi untuk Windows](SETUP_WINDOWS.md)

### macOS

- [Panduan Instalasi untuk macOS](SETUP_MACOS.md)

### Docker

- [Panduan Instalasi untuk Docker](SETUP_DOCKER.md)

## Fitur

- 🤖 **Pertahanan bot & otomatisasi bawaan** — fingerprinting yang di-hosting sendiri, analisis perilaku penggambaran, pengaitan multi-akun, dan tantangan PoW. Tanpa SaaS, tanpa data yang keluar dari server Anda
- 🦀 **Rust (Axum + Tokio + SQLx)** — server HTTP multi-thread berlatensi rendah
- 🖼️ **Mesin tile in-memory** — tile yang sering diakses tinggal di RAM (masing-masing 1 MB grid warna) dan disajikan sebagai PNG terindeks palet, tanpa bolak-balik ke DB pada setiap permintaan
- ⚡ **Pipeline penggambaran atomik** — regen charge, pemeriksaan kecukupan, reward level & droplet dalam satu `UPDATE … RETURNING`; upsert piksel secara batch; penulisan blob tile & statistik wilayah secara write-behind
- 🗺️ **Wilayah GeoNames** — pencarian wilayah terdekat dengan KD-tree dan memoization per piksel, papan peringkat wilayah/negara, autocomplete
- 🏆 **Papan peringkat** — papan pemain / aliansi / negara / wilayah untuk hari ini / minggu / bulan / sepanjang masa, materialized views + papan wilayah realtime
- 🛡️ **Perlengkapan moderasi** — tiket dengan tangkapan layar, ban dengan cascading rentang IP, timeout, panel admin/moderator, impor daftar IP yang di-ban
- 🛒 **Toko & progresi** — mata uang droplets, charges, palet berbayar, 251 bendera, level
- 💬 **Integrasi Discord** — pengaitan OAuth, peningkatan cooldown berbasis role, bot gateway, notifikasi DM
- 🔐 **Sesi & autentikasi** — cookie JWT, kata sandi bcrypt, cache sesi in-memory, rate limiting per-IP
- 🐘 **PostgreSQL 17** — upsert `ON CONFLICT`, kunci statistik `UNIQUE NULLS NOT DISTINCT`, agregasi paralel

## Performa

Dataset sintetis yang identik (1,000 wilayah, 5,000 pengguna, 20 tile × 250k piksel yang digambar = 5 juta baris), kedua stack di mesin yang sama, database di Docker, generator beban HTTP yang sama (koneksi keep-alive yang sudah hangat, durasi uji 20 detik). Angka-angka berasal dari mesin pengembangan Windows 11 — yang penting adalah **proporsinya**:

| Skenario (konkurensi) | Node.js + MariaDB | Rust + PostgreSQL | Percepatan |
|---|---|---|---|
| `GET /health` — overhead framework (64) | 10,315 rps · p50 5.8 ms | **135,448 rps · p50 0.43 ms** | **13×** |
| `GET /files/s0/tiles/0/0.png` — hot tile (32) | 434 rps · p50 71.9 ms | **9,412 rps · p50 2.95 ms** | **22×** |
| Beban kerja baca campuran (32) | 521 rps · p50 60.6 ms | **5,177 rps · p50 0.85 ms** | **10×** |
| `GET /s0/pixel/…` — info piksel (32) | 3,202 rps · p50 9.4 ms | **6,445 rps · p50 4.7 ms** | 2× |
| `GET /me` — profil terautentikasi (50) | 2,063 rps · p50 23.7 ms | **7,224 rps · p50 6.6 ms** | 3.5× |
| `GET /leaderboard/player/all-time` (32) | 2,983 rps · p50 9.9 ms | 4,094 rps · p50 7.1 ms | 1.4× |
| `POST paint` 25 piksel (50 konkuren) | 4.5 rps · **45% 5xx** · p50 8.9 s | **434 rps · 0 error · p50 111 ms** | **~97×** |
| `POST paint` 25 piksel (8 konkuren) | 4.2 rps · p50 2.0 s | **401 rps · 0 error · p50 18.4 ms** | **~95×** |

Dalam istilah throughput penggambaran: stack lama hanya bertahan pada ~105 piksel yang digambar per detik di bawah konkurensi, sedangkan stack Rust bertahan pada **~10,000 piksel yang digambar per detik** tanpa kegagalan. Dengan 50 penggambar konkuren, backend Node mulai mengembalikan HTTP 500 (konflik tulis Prisma pada baris pengguna yang panas) dan latensinya memburuk hingga hitungan detik — backend Rust menjaga p99 di bawah 270 ms.

### Mengapa ini secara struktural lebih baik — bukan sekadar lebih cepat

Kecepatan hanyalah gejala, bukan tujuannya. Penulisan ulang ini menghilangkan kelas-kelas
masalah yang biasa dialami backend Node.js + ORM pada skala besar:

| | Node.js asli | openplace (Rust) |
|---|---|---|
| Transaksi penggambaran | transaksi Prisma multi-langkah, `SELECT … FOR UPDATE`, loop retry, perlu timeout yang dikonfigurasi saat beban tinggi | satu `UPDATE … WHERE charges_left >= cost RETURNING` yang atomik — tidak ada yang bisa timeout |
| Pipeline tile | decode → canvas → re-kuantisasi sharp pada setiap penggambaran, blob ditulis ulang dari DB | grid warna di RAM adalah sumber kebenaran; PNG terindeks dienkode ulang dalam ~1–3 ms; loop rekonsiliasi yang menyembuhkan diri membangun ulang tile mana pun yang baris pikselnya lebih baru daripada blob-nya (aman dari crash) |
| Beban DB per permintaan | sesi + pengguna dibaca ulang dari DB pada setiap permintaan | cache TTL untuk sesi, pengguna, wilayah, pengaturan |
| Docker | npm install + prisma generate/db push saat container boot, proxy mulai sebelum aplikasi siap | satu binary statis, probe healthcheck bawaan, Caddy menunggu hingga aplikasi sehat |
| Pertahanan bot | tidak ada bawaan | fingerprinting self-hosted + skor perilaku + PoW (lihat di atas), dapat disetel saat runtime |

### Masalah upstream yang diketahui — ditangani di fork ini

Masalah nyata dari pelacak proyek asli, beserta statusnya di sini:

| Masalah upstream | Status di openplace (Rust) |
|---|---|
| ["68 econnrefused on local hosting"](https://github.com/openplaceteam/openplace/issues/68) | Compose yang ramping: satu binary, healthcheck bawaan, urutan startup yang ketat |
| ["66 Docker configuration needs complete revamp"](https://github.com/openplaceteam/openplace/issues/66) | Ditulis ulang dari nol: Postgres 17 + healthcheck, build Rust multi-tahap, **nol** langkah npm/prisma saat boot |
| ["58 502 Bad Gateway after Docker setup"](https://github.com/openplaceteam/openplace/issues/58) | Caddy kini menunggu `condition: service_healthy` pada aplikasi — secara fisik tidak mungkin mem-proxy backend yang belum siap |
| ["59 Backend doesn't always render tile"](https://github.com/openplaceteam/openplace/issues/59) | Pipeline deterministik: grid RAM sebagai sumber kebenaran + loop rekonsiliasi (setiap 5 menit) yang membangun ulang tile mana pun yang pikselnya lebih baru daripada blob yang tersimpan |
| ["51 Prisma transaction timeout under heavy workloads"](https://github.com/openplaceteam/openplace/issues/51) | Tanpa Prisma, tanpa transaksi panjang untuk disetel — hot path hanya satu pernyataan; ~10rb piksel digambar/detik secara berkelanjutan tanpa error |
| ["67 moderator page and overlay needed"](https://github.com/openplaceteam/openplace/issues/67) | Panel `/moderation` + API moderator lengkap ikut dikirim, disajikan, dan diuji di browser terhadap frontend asli |
| ["45 Discord linking does not work without a server"](https://github.com/openplaceteam/openplace/issues/45) | Pengaitan murni OAuth — tidak perlu guild; sinkronisasi guild milik bot bersifat opsional dan menurun dengan baik |
| ["44 Discord username not removed on unlink"](https://github.com/openplaceteam/openplace/issues/44) | Diperbaiki: unlink menghapus baik `discord` maupun `discord_user_id` dan mereset cooldown |
| ["55 IDs given to /moderator/users are… I don't know"](https://github.com/openplaceteam/openplace/issues/55) | Bentuk API didokumentasikan di ringkasan API README; ID bersifat eksplisit dan dipertahankan |

Masih ada di roadmap (belum diimplementasikan, jujur saja): endpoint banding
(`/report/appeal`, `/me/last-appeal` — upstream #56/#57) dan template kanvas
(upstream #61).


## Mulai cepat (Docker)

Persyaratan: [Docker](https://docs.docker.com/get-docker/) dengan Compose v2.

```sh
# 1. Clone beserta submodule frontend
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace

# 2. Konfigurasi
cp .env.example .env
# → edit .env dan atur JWT_SECRET (wajib) serta hal lain yang Anda perlukan

# 3. Jalankan
docker compose up -d --build
```

Kemudian inisialisasi database dan impor data wilayah:

```sh
# buat tabel + pengguna sistem
docker compose exec app openplace-backend setup

# impor data kota GeoNames (lihat "Data wilayah" di bawah)
docker compose exec app openplace-backend import-geonames /path/to/cities1000.zip
```

API mendengarkan di `:3000` (di belakang Caddy pada `:80`/`:443`), frontend Nuxt di `127.0.0.1:3001`.

> [TIPS]
> `setup`, `import-geonames` dan lainnya adalah subperintah dari satu
> binary `openplace-backend` — lihat [referensi baris perintah](#referensi-baris-perintah).

## Instalasi dari sumber

Persyaratan: [Rust](https://rustup.rs/) 1.85+, [PostgreSQL](https://www.postgresql.org/) 15+ (disarankan 17), opsional [Caddy](https://caddyserver.com/) untuk TLS/reverse proxy.

```sh
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace

# 1. Database
createdb openplace            # atau gunakan Docker: docker run -d --name openplace-pg \
                              #   -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres \
                              #   -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine

# 2. Environment
cp .env.example .env
# → atur DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
# → atur JWT_SECRET ke string acak yang panjang

# 3. Build & jalankan
cd backend-rs
cargo run --release -- setup          # migrasi + pengguna sistem
cargo run --release -- import-geonames cities1000.zip
cargo run --release -- serve          # HTTP API di BACKEND_PORT (bawaan 3000)
```

> [CATATAN]
> Server menyajikan direktori `frontend/` (submodule git) sebagai berkas
> statis, menyelesaikannya melalui `FRONTEND_DIR` (bawaan `./frontend`) **relatif
> terhadap direktori kerja tempat ia dijalankan**. Saat menjalankan binary dari
> `backend-rs/`, atur `FRONTEND_DIR=../frontend`.

### Data wilayah

Batas/kota wilayah berasal dari dump [GeoNames](https://download.geonames.org/export/dump/). Unduh salah satu dari `cities500.zip` (terkecil), `cities1000.zip`, `cities5000.zip` atau `allCountries.zip` (terbesar) dan imporkan — importer menerima baik `.zip` maupun TSV mentah:

```sh
openplace-backend import-geonames cities1000.zip
```

Tanpa data wilayah, backend tetap berfungsi, tetapi setiap piksel akan dipetakan ke wilayah fallback.

## Konfigurasi

Seluruh konfigurasi berbasis environment (`.env`). Daftar lengkap yang diberi anotasi ada di [.env.example](../../.env.example); berikut yang paling penting:

| Variabel | Bawaan | Deskripsi |
|---|---|---|
| `DATABASE_URL` | — | Connection string PostgreSQL (**wajib**) |
| `JWT_SECRET` | — | Rahasia penandatanganan HS256 (**wajib**) |
| `BACKEND_PORT` | `3000` | Port HTTP (`PORT` juga dihormati) |
| `EXTERNAL_URL` | — | URL dasar publik (digunakan pada tautan reset kata sandi) |
| `COOLDOWN_MS` | `30000` | Interval pengisian ulang charge dasar |
| `LEVEL_BASE_PIXEL` / `LEVEL_EXPONENT` | `30` / `0.65` | Kurva level: `(pixels/30)^0.65 + 1` |
| `ENABLE_RATE_LIMIT` | `false` | Mengaktifkan rate limiting per-IP |
| `*_RATE_LIMIT_ATTEMPTS` / `*_RATE_LIMIT_MS` | lihat `.env.example` | Batas untuk login/signup/reset kata sandi/paint |
| `BAN_ON_BANNED_IP` / `BLOCK_TOR` | `false` | Ban akun yang menggambar dari IP yang di-ban / blokir exit node Tor |
| `DISCORD_*` | — | Integrasi OAuth + bot (opsional) |
| `DB_MAX_CONNECTIONS` | `32` | Ukuran pool PostgreSQL |
| `SESSION_CACHE_TTL_MS` | `60000` | Berapa lama sesi divalidasi dari RAM |
| `USER_CACHE_TTL_MS` | `30000` | TTL cache baris pengguna |
| `TILE_CACHE_MAX_TILES` | `512` | Berapa banyak tile yang digambar yang tetap di RAM |
| `TILE_FLUSH_MS` / `STATS_FLUSH_MS` | `500` / `1000` | Interval write-behind untuk blob tile & statistik |
| `FRONTEND_HOST` / `FRONTEND_PORT` | `localhost` / `3001` | Target proxy frontend Nuxt |
| `FRONTEND_DIR` | `./frontend` | Direktori frontend statis (relatif terhadap cwd) |
| `ANTI_BOT_MODE` | `log` | `off` / `log` / `enforce` — pertahanan bot bawaan |
| `ANTI_BOT_KEY` | diturunkan dari `JWT_SECRET` | Kunci HMAC untuk ID pengunjung |
| `ANTI_BOT_POW_BITS` | `18` | Tingkat kesulitan proof-of-work |
| `ANTI_BOT_ENFORCE_THRESHOLD` | `100` | Skor yang memblokir penggambaran pada mode `enforce` |

## Referensi baris perintah

`openplace-backend` adalah satu binary dengan subperintah:

| Perintah | Deskripsi |
|---|---|
| `serve` | Menjalankan server HTTP (beban kerja bawaan) |
| `setup` | Menerapkan migrasi dan mengisi pengguna sistem |
| `import-geonames <file.zip\|file.txt>` | Mengimpor dump GeoNames sebagai wilayah |
| `import-ip-list <file> [--reason ip-list]` | Mengimpor IP/CIDR yang di-ban (satu per baris, komentar `#`) |
| `system-notification <title> <message>` | Menyiarkan notifikasi sistem ke semua pengguna |
| `redraw-tiles` | Merender ulang semua PNG tile dari baris piksel |
| `init-leaderboard` | Menginisialisasi view papan peringkat |
| `migrate-from-mysql <mysql-url> [--force]` | Mengimpor data dari database Node.js lama |
| `seed-bench` | Mengisi data sintetis untuk load testing |

## Migrasi dari backend Node.js lama

Memindahkan komunitas yang sudah ada dari stack Node.js/MariaDB hanya membutuhkan satu perintah. Kata sandi (bcrypt) dan bahkan sesi login yang sedang aktif pun ikut terbawa — jika `JWT_SECRET` tidak berubah, pengguna tetap dalam keadaan login.

```sh
# 1. Siapkan backend baru (lihat Instalasi dari sumber)
openplace-backend setup

# 2. Impor semuanya dari database MariaDB/MySQL lama
DATABASE_URL="postgres://…new…" \
  openplace-backend migrate-from-mysql "mysql://root:password@old-host:3306/openplace"

# 3. Jalankan backend baru
openplace-backend serve
```

Yang dimigrasikan: **pengguna** (beserta hash kata sandi), **piksel**, **blob PNG tile**, **aliansi** (anggota, undangan, ban), **lokasi favorit**, **IP yang di-ban**, **wilayah**, **tiket** (beserta tangkapan layar), **catatan pengguna**, **view papan peringkat**, **statistik wilayah**, **notifikasi**, **gambar profil** dan **sesi**. ID dipertahankan.

Importer menolak menulis ke database target yang tidak kosong kecuali Anda memberikan `--force`; menjalankannya kembali aman (ia melakukan upsert).

## Ringkasan API

Setiap rute tersedia dengan dan tanpa prefiks `/api`. Autentikasi menggunakan JWT HS256 dalam cookie `j` yang HttpOnly.

| Grup | Sorotan |
|---|---|
| `POST /login` `POST /register` `POST /auth/logout` `POST /auth/request-password-reset` `POST /auth/reset-password` | Siklus akun |
| `GET /me` `POST /me/update` `DELETE /me` `GET/POST /me/profile-picture*` `DELETE /me/sessions` | Manajemen profil |
| `POST /s0/pixel/{tileX}/{tileY}` | Menggambar piksel (batch, berbayar charge) |
| `GET /files/s0/tiles/{x}/{y}.png` | Gambar tile (mendukung 304 via `Last-Modified`) |
| `GET /s0/pixel/{tileX}/{tileY}?x=&y=` | Siapa yang menggambar sebuah piksel + info wilayah |
| `GET /leaderboard/{player,alliance,country,region}/…` | Papan peringkat |
| `POST/GET /alliance…` | Buat/gabung/keluar/undangan/ban/papan peringkat aliansi |
| `POST /purchase` `POST /flag/equip/{id}` | Toko (charges, palet, bendera) |
| `GET /notification/…` | Kotak masuk notifikasi (+ siaran sistem) |
| `POST /report-user` `POST /admin/ban-user` | Laporan moderasi |
| `/admin/*` `/moderator/*` | Panel admin & moderator (HTML + JSON) |
| `GET /v1/autocomplete?text=` | Autocomplete wilayah (GeoJSON) |
| `GET /health` `GET /checkrobots` `GET /challenge` | Utilitas |

Referensi yang sahih adalah [protokol Wplace](../../protocol.md) asli.

## Pertahanan bot & otomatisasi (bawaan, self-hosted)

wplace asli bergantung pada SaaS fingerprinting berbayar. openplace menyertakan
padanan yang **sepenuhnya milik Anda sendiri**: tanpa layanan pihak ketiga, tanpa
data yang keluar dari server Anda, dan setiap lapisannya dapat diaktifkan atau
dinonaktifkan dari panel admin di `/admin/customize` — tanpa perlu restart.

| Lapisan | Apa yang dilakukannya | Berfungsi tanpa JS? |
|---|---|---|
| **Pengumpul fingerprint** | Skrip mandiri (`/fp.js`) disuntikkan otomatis ke setiap halaman yang disajikan — tanpa perubahan frontend. Membuat hash karakteristik canvas / WebGL / audio / font di sisi klien; server menurunkan **ID pengunjung** yang stabil (`HMAC-SHA256` dengan kunci di sisi server — klien tidak dapat memalsukannya). | sebagian |
| **Pengaitan multi-akun** | Satu ID pengunjung yang menggambar dari beberapa akun dilaporkan ke admin di `/admin/users` (`fp_accounts`, `fp_linked_users`). Menggerakkan aturan `ALLOW_MULTI_ACCOUNT`. | tidak |
| **Skor perilaku** | Setiap permintaan penggambaran diamati di sisi server: interval permintaan yang teratur seperti mesin, ukuran batch yang seragam sempurna, UA otomatisasi (`HeadlessChrome`, `Puppeteer`, `python-requests`, `navigator.webdriver`), fingerprint yang hilang. | **ya** |
| **Tantangan proof-of-work** | Pengguna yang ditandai membersihkan skornya dengan menyelesaikan PoW SHA-256 (`/fp/challenge`) — pengguna sungguhan tidak akan menyadarinya; farm skrip justru membakar CPU. | ya |

### Sinyal yang menaikkan skor bot pengguna

| Sinyal | Bobot |
|---|---|
| Interval penggambaran yang teratur seperti mesin (koefisien variasi < 0.08) | +40 |
| Ukuran batch yang seragam sempurna di banyak permintaan | +25 |
| User agent headless / otomatisasi, `navigator.webdriver` | +60 |
| Volume penggambaran tinggi tanpa fingerprint yang pernah terkumpul | +20 |

### Penegakan

`ANTI_BOT_MODE` — `off` / `log` (bawaan: mengamati, memaparkan skor ke admin) /
`enforce`: pengguna dengan skor di atas `ANTI_BOT_ENFORCE_THRESHOLD` mendapatkan
403 saat menggambar hingga mereka menyelesaikan PoW. Admin dan moderator selalu
dikecualikan. Semua ini dapat diedit saat runtime di `/admin/customize` — termasuk
beralih ke `enforce` di tengah serangan. Ketika `ALLOW_BOTS=true` (aturan komunitas),
pertahankan `log`: komunitas bot tetap terlihat tetapi tidak pernah diblokir.

> [CATATAN]
> Tidak ada fingerprinting yang bisa mengalahkan penyerang yang gigih dengan
> otomatisasi browser stealth — pertahanan di sini menaikkan biaya otomatisasi
> massal dan membuatnya terlihat oleh moderator, yang justru dilengkapi oleh
> sistem charge, laporan, dan cascading ban IP. Hanya hash turunan dan kolom
> kasar yang disimpan (tanpa data canvas/audio mentah), sehingga data yang
> tersimpan tetap minimal.



### Menjalankan di belakang Cloudflare

Backend menyelesaikan IP klien persis seperti backend Node asli:
`cf-connecting-ip` → `x-forwarded-for` (entri pertama) → alamat soket. Ban IP,
rate limit, dan statistik dikunci pada IP yang telah diselesaikan ini, sehingga
semuanya langsung bekerja di belakang Cloudflare (atau reverse proxy apa pun
yang mengatur header-header tersebut).

> [PENTING]
> Header-header ini dipercaya tanpa syarat (sama seperti backend asli),
> sehingga akses langsung ke origin harus diblokir — jika tidak, klien dapat
> memalsukan `cf-connecting-ip` dan menghindari ban IP / rate limit. Batasi
> port 3000 hanya untuk rentang IP Cloudflare, atau tempatkan Caddy /
> Cloudflare Tunnel di depannya.

## Benchmark: bagaimana kami mengukurnya

Generator beban disertakan bersama backend (`backend-rs/src/bin/loadgen.rs`) — reproduksi tabel di atas dengan:

```sh
# isi data identik ke kedua stack
DATABASE_URL="postgres://…" backend-rs/target/release/openplace-backend seed-bench \
  --regions 1000 --users 5000 --tiles 20

# gempur salah satu stack (contoh)
backend-rs/target/release/loadgen --url http://127.0.0.1:3900 \
  --scenario tile --tile 0,0 --conns 32 --duration 20
backend-rs/target/release/loadgen --url http://127.0.0.1:3100 \
  --scenario paint --paint 25 --logins 50 --conns 50 --duration 20
```

Catatan keadilan: endpoint papan peringkat dibatasi oleh SQL agregasi yang sama yang dijalankan kedua stack, itulah sebabnya angkanya sederhana, hanya 1.4×; angka penggambaran mencakup commit DB sinkron (pengurangan charge bersifat durabel di kedua stack sebelum merespons).

## Menambahkan terjemahan

Terjemahan sangat kami sambut — termasuk yang dibuat dengan bantuan AI! Terjemahan mesin adalah titik awal yang sama sekali tidak masalah, dan peninjauan atau perbaikan dari penutur asli selalu kami hargai.

Untuk menambahkan bahasa baru atau menyegarkan terjemahan yang sudah ada untuk `README.md` beserta panduan instalasi, silakan ikuti langkah-langkah berikut.

### Ubah nomor versi di bagian atas README ini untuk menandakan bahwa bahasa baru telah ditambahkan

Nomor versi diformat sebagai `X.XX`, di mana "X" pertama mewakili jumlah bahasa yang telah diterjemahkan secara resmi sejauh ini. "X" kedua setelah titik diubah setiap kali ada modifikasi pada versi bahasa Inggris dari README. Nomor versi ini membantu penerjemah mengetahui kapan terjemahan mereka perlu diperbarui.

### Buat folder baru di direktori `translations` yang dinamai sesuai kode ISO bahasa Anda

Jika Anda tidak yakin apa kode ISO Anda, Anda dapat memeriksanya [di sini](https://gist.githubusercontent.com/josantonius/b455e315bc7f790d14b136d61d9ae468/raw/416def351bc1f790d14b136d61d9ae468/language-codes.json) atau cukup mencarinya di internet. Yang Anda cari adalah kode dua huruf seperti `"en"` untuk bahasa Inggris.

### Salin file bahasa Inggris ke folder baru Anda

Salin file bahasa Inggris dari folder `translations/en` dan `README.md` utama ke folder yang baru saja Anda buat.
Sekarang Anda seharusnya memiliki empat file: `README.md` dan tiga file markdown instalasi (`.md`).

### Tambahkan bendera yang benar ke kedua README

Saat membuat terjemahan baru, Anda harus memperbarui **dua** file README:

#### 1. **README bahasa Inggris asli**

Tambahkan **hanya bendera negara/bahasa yang Anda terjemahkan** di bagian atas.
Bendera ini harus menautkan ke README terjemahan baru Anda.

Gunakan template ini:

```html
<a href="translations/LANGUAGE_ISO_CODE/NAME_OF_YOUR_README.md"><img src="https://flagcdn.com/256x192/LANGUAGE_ISO_CODE.png" width="48" alt="NAME_OF_COUNTRY Flag"></a>
```

Ganti placeholder dengan kode ISO dan nama negara untuk terjemahan Anda.

#### 2. **README terjemahan Anda**

Di bagian atas README terjemahan Anda, tambahkan **hanya bendera Amerika**, yang menautkan kembali ke README bahasa Inggris.

> [CATATAN]
> Bendera di README bahasa Inggris harus tetap dalam urutan abjad berdasarkan kode ISO.

### Perbarui tautan di bagian Memulai

Di bagian **Memulai**, perbarui tautan agar menunjuk ke file terjemahan Anda.
Jika Anda tidak yakin bagaimana melakukannya, lihat folder bahasa lain (misalnya, `fr`).

### Terjemahkan file

Terjemahkan kontennya dan pertahankan strukturnya: blok kode, nama variabel environment, endpoint, dan nama file tetap dalam bahasa Inggris. Sebelum membuka pull request Anda, klik **setiap** tautan dan bendera untuk memastikan semuanya berfungsi.

Setelah semuanya terlihat baik, buka pull request Anda — terima kasih! 💙

## Lisensi

Dilisensikan di bawah Lisensi Apache, versi 2.0. Lihat [LICENSE.md](https://github.com/13MrBlackCat13/openplace/blob/main/LICENSE.md).

### Ucapan terima kasih

Data wilayah berasal dari [GeoNames Gazetteer](https://download.geonames.org/export/dump/), dan dilisensikan di bawah [Creative Commons Attribution 4.0 License](https://creativecommons.org/licenses/by/4.0/). Data disediakan "apa adanya" tanpa jaminan atau representasi mengenai keakuratan, ketepatan waktu, atau kelengkapan.
