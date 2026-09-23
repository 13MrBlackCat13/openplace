# openplace

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

Openplace (viết thường) là một backend mã nguồn mở miễn phí, không chính thức cho [wplace.](https://wplace.live) — một fork Rust của [openplace gốc](https://github.com/openplaceteam/openplace) (Node.js), được viết lại để đạt tốc độ cao và chống lại tự động hóa. Chúng tôi hướng tới việc mang lại sự tự do và linh hoạt để mọi người dùng đều có thể tự tạo trải nghiệm wplace riêng tư cho bản thân, bạn bè, hoặc thậm chí cộng đồng của mình — **theo cách của bạn**: chống bot tích hợp sẵn, fingerprinting tự lưu trữ, và các quy tắc cộng đồng mà bạn có thể thay đổi ngay lúc chạy từ trang quản trị.

Backend được viết bằng **Rust** (Axum + Tokio + SQLx) trên nền **PostgreSQL** và phục vụ HTTP API của wplace với đường xử lý nóng (hot path) hoàn toàn nằm trong bộ nhớ: các Tile đã vẽ được giữ dưới dạng lưới màu trong RAM và phục vụ dưới dạng PNG đánh chỉ mục (indexed), charges và phần thưởng được áp dụng trong một câu lệnh nguyên tố (atomic) duy nhất, còn thống kê khu vực được ghi phía sau request. Đây là bản thay thế trực tiếp (drop-in) cho backend Node.js gốc — cùng các route, cùng các hợp đồng JSON, cùng cookie.

> [CẢNH BÁO ⚠️]
> Đây là một dự án đang trong quá trình hoàn thiện. Hãy sẵn sàng đón nhận các tính năng chưa hoàn thiện và lỗi. Hãy giúp chúng tôi bằng cách đăng các vấn đề vào kênh #tech-support trong [máy chủ Discord](https://discord.gg/ZRC4DnP9Z2) của chúng tôi hoặc bằng cách đóng góp pull request. Cảm ơn!

## Mục lục

- [Tính năng](#tính-năng)
- [Hiệu năng](#hiệu-năng)
- [Chống bot & tự động hóa (tích hợp sẵn, tự lưu trữ)](#chống-bot--tự-động-hóa-tích-hợp-sẵn-tự-lưu-trữ)
- [Bắt đầu nhanh (Docker)](#bắt-đầu-nhanh-docker)
- [Cài đặt từ mã nguồn](#cài-đặt-từ-mã-nguồn)
- [Cấu hình](#cấu-hình)
- [Tham chiếu dòng lệnh](#tham-chiếu-dòng-lệnh)
- [Chuyển đổi từ backend Node.js cũ](#chuyển-đổi-từ-backend-nodejs-cũ)
- [Tổng quan API](#tổng-quan-api)
- [Benchmark: cách chúng tôi đo](#benchmark-cách-chúng-tôi-đo)
- [Thêm bản dịch](#thêm-bản-dịch)
- [Giấy phép](#giấy-phép)

## Tính năng

- 🤖 **Chống bot & tự động hóa tích hợp sẵn** — fingerprinting tự lưu trữ, phân tích hành vi vẽ pixel, liên kết đa tài khoản và thử thách PoW. Không SaaS, không có dữ liệu nào rời khỏi máy chủ của bạn
- 🦀 **Rust (Axum + Tokio + SQLx)** — máy chủ HTTP đa luồng, độ trễ thấp
- 🖼️ **Cỗ máy Tile trong bộ nhớ** — các Tile nóng nằm trong RAM (mỗi Tile là một lưới màu 1 MB) và được phục vụ dưới dạng PNG đánh chỉ mục theo bảng màu, không cần truy vấn DB cho mỗi request
- ⚡ **Quy trình vẽ nguyên tố (atomic)** — hồi phục charge, kiểm tra đủ điều kiện, phần thưởng cấp độ & Droplet trong một câu `UPDATE … RETURNING` duy nhất; ghi pixel theo lô (batched upsert); ghi nền (write-behind) cho blob Tile & thống kê khu vực
- 🗺️ **Khu vực GeoNames** — tra cứu khu vực gần nhất bằng KD-tree với ghi nhớ (memoization) theo từng pixel, bảng xếp hạng khu vực/quốc gia, autocomplete
- 🏆 **Bảng xếp hạng** — bảng người chơi/liên minh/quốc gia/khu vực theo hôm nay/tuần/tháng/mọi thời đại, materialized views + bảng xếp hạng khu vực thời gian thực
- 🛡️ **Bộ công cụ kiểm duyệt** — ticket kèm ảnh chụp màn hình, lệnh cấm với mở rộng theo dải IP, timeout, trang quản trị/người kiểm duyệt, nhập danh sách IP bị cấm
- 🛒 **Cửa hàng & tiến trình** — tiền tệ Droplets, charges, bảng màu trả phí, 251 lá cờ, cấp độ
- 💬 **Tích hợp Discord** — liên kết OAuth, tăng cooldown theo vai trò (role), gateway bot, thông báo DM
- 🔐 **Phiên & xác thực** — cookie JWT, mật khẩu bcrypt, cache phiên trong bộ nhớ, giới hạn tốc độ theo IP
- 🐘 **PostgreSQL 17** — upsert `ON CONFLICT`, khóa thống kê `UNIQUE NULLS NOT DISTINCT`, tổng hợp song song

## Hiệu năng

Bộ dữ liệu tổng hợp giống hệt nhau (1.000 khu vực, 5.000 người dùng, 20 Tile × 250k pixel đã vẽ = 5 triệu dòng), cả hai stack chạy trên cùng một máy, cơ sở dữ liệu trong Docker, cùng một công cụ tạo tải HTTP (kết nối keep-alive đã ấm, mỗi lần chạy 20 giây). Các con số lấy từ một máy phát triển Windows 11 — cái quan trọng là **tỷ lệ tương đối**:

| Kịch bản (độ đồng thời) | Node.js + MariaDB | Rust + PostgreSQL | Tăng tốc |
|---|---|---|---|
| `GET /health` — chi phí framework (64) | 10.315 rps · p50 5,8 ms | **135.448 rps · p50 0,43 ms** | **13×** |
| `GET /files/s0/tiles/0/0.png` — Tile nóng (32) | 434 rps · p50 71,9 ms | **9.412 rps · p50 2,95 ms** | **22×** |
| Khối lượng đọc trộn lẫn (32) | 521 rps · p50 60,6 ms | **5.177 rps · p50 0,85 ms** | **10×** |
| `GET /s0/pixel/…` — thông tin pixel (32) | 3.202 rps · p50 9,4 ms | **6.445 rps · p50 4,7 ms** | 2× |
| `GET /me` — hồ sơ đã xác thực (50) | 2.063 rps · p50 23,7 ms | **7.224 rps · p50 6,6 ms** | 3,5× |
| `GET /leaderboard/player/all-time` (32) | 2.983 rps · p50 9,9 ms | 4.094 rps · p50 7,1 ms | 1,4× |
| `POST paint` 25 pixel (50 luồng đồng thời) | 4,5 rps · **45% 5xx** · p50 8,9 s | **434 rps · 0 lỗi · p50 111 ms** | **~97×** |
| `POST paint` 25 pixel (8 luồng đồng thời) | 4,2 rps · p50 2,0 s | **401 rps · 0 lỗi · p50 18,4 ms** | **~95×** |

Xét về thông lượng vẽ: stack cũ chỉ duy trì ~105 pixel đã vẽ/giây khi có đồng thời, trong khi stack Rust duy trì **~10.000 pixel đã vẽ/giây** với zero lỗi. Với 50 người vẽ đồng thời, backend Node bắt đầu trả về HTTP 500 (xung đột ghi của Prisma trên dòng user nóng) và độ trễ tăng lên hàng giây — backend Rust giữ p99 dưới 270 ms.

### Vì sao nó tốt hơn về mặt cấu trúc — chứ không chỉ nhanh hơn

Tốc độ chỉ là triệu chứng, không phải mục tiêu. Bản viết lại loại bỏ các lớp vấn đề mà một backend Node.js + ORM thường gặp ở quy mô lớn:

| | Node.js gốc | openplace (Rust) |
|---|---|---|
| Giao dịch vẽ | giao dịch Prisma nhiều bước, `SELECT … FOR UPDATE`, vòng lặp retry, cần cấu hình timeout khi tải cao | một câu `UPDATE … WHERE charges_left >= cost RETURNING` nguyên tố — không có gì để timeout |
| Quy trình Tile | decode → canvas → sharp re-quantize cho mỗi lần vẽ, blob được ghi lại từ DB | lưới màu trong RAM là nguồn chân lý duy nhất; PNG indexed được mã hóa lại trong ~1–3 ms; vòng reconcile tự phục hồi dựng lại bất kỳ Tile nào có các dòng pixel mới hơn blob của nó (an toàn khi crash) |
| Tải DB mỗi request | phiên (session) + user được đọc lại từ DB ở mỗi request | cache TTL cho session, user, khu vực, cài đặt |
| Docker | npm install + prisma generate/db push khi container khởi động, proxy khởi động trước khi app sẵn sàng | một binary tĩnh duy nhất, probe healthcheck tích hợp sẵn, Caddy chờ app healthy |
| Chống bot | không có sẵn | fingerprinting tự lưu trữ + chấm điểm hành vi + PoW (xem phía trên), tinh chỉnh được ngay lúc chạy |

### Các điểm đau đã biết của dự án gốc — đã được xử lý trong fork này

Các vấn đề thực tế từ tracker của dự án gốc, và hiện trạng của chúng tại đây:

| Vấn đề từ dự án gốc | Hiện trạng trong openplace (Rust) |
|---|---|
| ["68 econnrefused khi host cục bộ"](https://github.com/openplaceteam/openplace/issues/68) | Compose được tinh gọn: một binary duy nhất, healthcheck tích hợp sẵn, thứ tự khởi động nghiêm ngặt |
| ["66 Cấu hình Docker cần được làm lại toàn diện"](https://github.com/openplaceteam/openplace/issues/66) | Viết lại từ đầu: Postgres 17 + healthcheck, build Rust nhiều giai đoạn, **không** bước npm/prisma nào lúc khởi động |
| ["58 502 Bad Gateway sau khi thiết lập Docker"](https://github.com/openplaceteam/openplace/issues/58) | Caddy giờ chờ `condition: service_healthy` trên app — về mặt kỹ thuật nó không thể nào proxy một backend chưa sẵn sàng |
| ["59 Backend đôi khi không render Tile"](https://github.com/openplaceteam/openplace/issues/59) | Quy trình tất định: lưới RAM là nguồn chân lý + vòng reconcile (mỗi 5 phút) dựng lại bất kỳ Tile nào có pixel mới hơn blob đã lưu |
| ["51 Timeout giao dịch Prisma khi khối lượng công việc lớn"](https://github.com/openplaceteam/openplace/issues/51) | Không Prisma, không có giao dịch dài phải tinh chỉnh — đường nóng chỉ là một câu lệnh duy nhất; duy trì ~10k pixel đã vẽ/giây với zero lỗi |
| ["67 Cần trang moderator và overlay"](https://github.com/openplaceteam/openplace/issues/67) | Trang `/moderation` + toàn bộ moderator API đều có mặt, được phục vụ và kiểm thử trên trình duyệt với frontend thật |
| ["45 Liên kết Discord không hoạt động nếu không có server"](https://github.com/openplaceteam/openplace/issues/45) | Liên kết thuần OAuth — không cần guild; đồng bộ guild của bot là tùy chọn và hạ cấp êm dịu khi thiếu |
| ["44 Username Discord không bị xóa khi hủy liên kết"](https://github.com/openplaceteam/openplace/issues/44) | Đã sửa: hủy liên kết xóa cả `discord` và `discord_user_id` và đặt lại cooldown |
| ["55 Các ID đưa cho /moderator/users là… tôi không biết"](https://github.com/openplaceteam/openplace/issues/55) | Hình dạng API được ghi trong phần Tổng quan API của README; các ID rõ ràng và được giữ nguyên |

Vẫn nằm trong lộ trình (chưa được hiện thực, nói thật): các endpoint kháng nghị (`/report/appeal`, `/me/last-appeal` — upstream #56/#57) và template canvas (upstream #61).


## Bắt đầu nhanh (Docker)

Yêu cầu: [Docker](https://docs.docker.com/get-docker/) với Compose v2. Hướng dẫn chi tiết từng bước: [SETUP_DOCKER.md](SETUP_DOCKER.md).

```sh
# 1. Clone kèm submodule frontend
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace

# 2. Cấu hình
cp .env.example .env
# → sửa .env và đặt JWT_SECRET (bắt buộc) cùng mọi thứ khác bạn cần

# 3. Chạy
docker compose up -d --build
```

Sau đó khởi tạo cơ sở dữ liệu và nhập dữ liệu khu vực:

```sh
# tạo các bảng + người dùng hệ thống
docker compose exec app openplace-backend setup

# nhập dữ liệu thành phố GeoNames (xem "Dữ liệu khu vực" phía dưới)
docker compose exec app openplace-backend import-geonames /path/to/cities1000.zip
```

API lắng nghe trên `:3000` (đằng sau Caddy ở `:80`/`:443`), frontend Nuxt trên `127.0.0.1:3001`.

> [MẸO]
> `setup`, `import-geonames` và các lệnh khác là subcommand của binary
> `openplace-backend` duy nhất — xem [tham chiếu dòng lệnh](#tham-chiếu-dòng-lệnh).

## Cài đặt từ mã nguồn

Yêu cầu: [Rust](https://rustup.rs/) 1.85+, [PostgreSQL](https://www.postgresql.org/) 15+ (khuyến nghị 17), tùy chọn [Caddy](https://caddyserver.com/) cho TLS/reverse proxy. Hướng dẫn chi tiết từng bước: [Windows](SETUP_WINDOWS.md) · [macOS](SETUP_MACOS.md).

```sh
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace

# 1. Cơ sở dữ liệu
createdb openplace            # hoặc dùng Docker: docker run -d --name openplace-pg \
                              #   -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres \
                              #   -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine

# 2. Môi trường
cp .env.example .env
# → đặt DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
# → đặt JWT_SECRET thành một chuỗi ngẫu nhiên dài

# 3. Build & chạy
cd backend-rs
cargo run --release -- setup          # migration + người dùng hệ thống
cargo run --release -- import-geonames cities1000.zip
cargo run --release -- serve          # HTTP API trên BACKEND_PORT (mặc định 3000)
```

> [LƯU Ý]
> Máy chủ phục vụ thư mục `frontend/` (git submodule) dưới dạng file tĩnh,
> phân giải qua `FRONTEND_DIR` (mặc định `./frontend`) **tính theo thư mục
> làm việc mà nó được khởi động từ đó**. Khi chạy binary từ `backend-rs/`,
> hãy đặt `FRONTEND_DIR=../frontend`.

### Dữ liệu khu vực

Ranh giới/thành phố của khu vực lấy từ dump [GeoNames](https://download.geonames.org/export/dump/). Tải một trong các file `cities500.zip` (nhỏ nhất), `cities1000.zip`, `cities5000.zip` hoặc `allCountries.zip` (lớn nhất) rồi nhập vào — bộ nhập chấp nhận cả `.zip` lẫn TSV thô:

```sh
openplace-backend import-geonames cities1000.zip
```

Không có dữ liệu khu vực, backend vẫn hoạt động, nhưng mọi pixel sẽ được ánh xạ về khu vực dự phòng.

## Cấu hình

Toàn bộ cấu hình dựa trên biến môi trường (`.env`). Danh sách đầy đủ kèm chú thích nằm trong [.env.example](../../.env.example); những điểm nổi bật:

| Biến | Mặc định | Mô tả |
|---|---|---|
| `DATABASE_URL` | — | chuỗi kết nối PostgreSQL (**bắt buộc**) |
| `JWT_SECRET` | — | khóa ký HS256 (**bắt buộc**) |
| `BACKEND_PORT` | `3000` | cổng HTTP (`PORT` cũng được thừa nhận) |
| `EXTERNAL_URL` | — | URL gốc công khai (dùng trong các link đặt lại mật khẩu) |
| `COOLDOWN_MS` | `30000` | chu kỳ hồi phục charge cơ bản |
| `LEVEL_BASE_PIXEL` / `LEVEL_EXPONENT` | `30` / `0.65` | đường cong cấp độ: `(pixels/30)^0.65 + 1` |
| `ENABLE_RATE_LIMIT` | `false` | bật giới hạn tốc độ theo IP |
| `*_RATE_LIMIT_ATTEMPTS` / `*_RATE_LIMIT_MS` | xem `.env.example` | giới hạn cho login/signup/đặt lại mật khẩu/vẽ |
| `BAN_ON_BANNED_IP` / `BLOCK_TOR` | `false` | cấm tài khoản vẽ từ IP bị cấm / chặn exit node Tor |
| `DISCORD_*` | — | tích hợp OAuth + bot (tùy chọn) |
| `DB_MAX_CONNECTIONS` | `32` | kích thước pool PostgreSQL |
| `SESSION_CACHE_TTL_MS` | `60000` | khoảng thời gian session được xác thực từ RAM |
| `USER_CACHE_TTL_MS` | `30000` | TTL cache dòng user |
| `TILE_CACHE_MAX_TILES` | `512` | số Tile đã vẽ tối đa giữ trong RAM |
| `TILE_FLUSH_MS` / `STATS_FLUSH_MS` | `500` / `1000` | chu kỳ ghi nền cho blob Tile & thống kê |
| `FRONTEND_HOST` / `FRONTEND_PORT` | `localhost` / `3001` | đích proxy cho frontend Nuxt |
| `FRONTEND_DIR` | `./frontend` | thư mục frontend tĩnh (tính theo cwd) |
| `ANTI_BOT_MODE` | `log` | `off` / `log` / `enforce` — chống bot tích hợp sẵn |
| `ANTI_BOT_KEY` | suy ra từ `JWT_SECRET` | khóa HMAC cho visitor ID |
| `ANTI_BOT_POW_BITS` | `18` | độ khó của proof-of-work |
| `ANTI_BOT_ENFORCE_THRESHOLD` | `100` | điểm số bị chặn vẽ ở chế độ `enforce` |

## Tham chiếu dòng lệnh

`openplace-backend` là một binary duy nhất với các subcommand:

| Lệnh | Mô tả |
|---|---|
| `serve` | Chạy máy chủ HTTP (khối lượng công việc mặc định) |
| `setup` | Áp dụng migration và tạo người dùng hệ thống |
| `import-geonames <file.zip\|file.txt>` | Nhập một dump GeoNames làm khu vực |
| `import-ip-list <file> [--reason ip-list]` | Nhập IP/CIDR bị cấm (mỗi dòng một mục, chú thích bằng `#`) |
| `system-notification <title> <message>` | Phát thông báo hệ thống tới toàn bộ người dùng |
| `redraw-tiles` | Vẽ lại toàn bộ PNG Tile từ các dòng pixel |
| `init-leaderboard` | Khởi tạo các view bảng xếp hạng |
| `migrate-from-mysql <mysql-url> [--force]` | Nhập dữ liệu từ cơ sở dữ liệu Node.js cũ |
| `seed-bench` | Tạo dữ liệu tổng hợp để kiểm thử tải |

## Chuyển đổi từ backend Node.js cũ

Chuyển một cộng đồng hiện có rời khỏi stack Node.js/MariaDB chỉ cần một lệnh duy nhất. Mật khẩu (bcrypt) và thậm chí các phiên đăng nhập đang hoạt động đều được chuyển sang — nếu `JWT_SECRET` không đổi, người dùng vẫn giữ trạng thái đăng nhập.

```sh
# 1. Thiết lập backend mới (xem Cài đặt từ mã nguồn)
openplace-backend setup

# 2. Nhập mọi thứ từ cơ sở dữ liệu MariaDB/MySQL cũ
DATABASE_URL="postgres://…new…" \
  openplace-backend migrate-from-mysql "mysql://root:password@old-host:3306/openplace"

# 3. Khởi động backend mới
openplace-backend serve
```

Những gì được chuyển đổi: **người dùng** (kèm hash mật khẩu), **pixel**, **blob PNG Tile**, **liên minh** (thành viên, lời mời, lệnh cấm), **vị trí yêu thích**, **IP bị cấm**, **khu vực**, **ticket** (kèm ảnh chụp màn hình), **ghi chú người dùng**, **view bảng xếp hạng**, **thống kê khu vực**, **thông báo**, **ảnh đại diện** và **phiên đăng nhập**. Các ID được giữ nguyên.

Bộ nhập từ chối ghi vào cơ sở dữ liệu đích không trống trừ khi bạn truyền `--force`; chạy lại là an toàn (nó dùng upsert).

## Tổng quan API

Mọi route đều khả dụng cả có lẫn không có tiền tố `/api`. Xác thực là một JWT HS256 trong cookie HttpOnly `j`.

| Nhóm | Điểm nổi bật |
|---|---|
| `POST /login` `POST /register` `POST /auth/logout` `POST /auth/request-password-reset` `POST /auth/reset-password` | Vòng đời tài khoản |
| `GET /me` `POST /me/update` `DELETE /me` `GET/POST /me/profile-picture*` `DELETE /me/sessions` | Quản lý hồ sơ |
| `POST /s0/pixel/{tileX}/{tileY}` | Vẽ pixel (theo lô, trừ charge) |
| `GET /files/s0/tiles/{x}/{y}.png` | Ảnh Tile (hỗ trợ 304, `Last-Modified`) |
| `GET /s0/pixel/{tileX}/{tileY}?x=&y=` | Ai đã vẽ pixel + thông tin khu vực |
| `GET /leaderboard/{player,alliance,country,region}/…` | Bảng xếp hạng |
| `POST/GET /alliance…` | Tạo/tham gia/rời/mời/cấm/bảng xếp hạng liên minh |
| `POST /purchase` `POST /flag/equip/{id}` | Cửa hàng (charges, bảng màu, cờ) |
| `GET /notification/…` | Hộp thư thông báo (+ thông báo hệ thống) |
| `POST /report-user` `POST /admin/ban-user` | Báo cáo kiểm duyệt |
| `/admin/*` `/moderator/*` | Trang quản trị & người kiểm duyệt (HTML + JSON) |
| `GET /v1/autocomplete?text=` | Autocomplete khu vực (GeoJSON) |
| `GET /health` `GET /checkrobots` `GET /challenge` | Tiện ích |

Tài liệu tham chiếu chính thức là [giao thức Wplace](../../protocol.md) gốc.

## Chống bot & tự động hóa (tích hợp sẵn, tự lưu trữ)

wplace gốc dựa vào một dịch vụ SaaS fingerprinting trả phí. openplace đi kèm một giải pháp tương đương **hoàn toàn thuộc về bạn**: không dịch vụ bên thứ ba, không dữ liệu nào rời khỏi máy chủ của bạn, và mọi lớp đều có thể bật/tắt từ trang quản trị tại `/admin/customize` — không cần khởi động lại.

| Lớp | Chức năng | Hoạt động không cần JS? |
|---|---|---|
| **Bộ thu fingerprint** | Một script độc lập (`/fp.js`) được tự động chèn vào mọi trang được phục vụ — không cần thay đổi frontend. Hash các đặc trưng canvas / WebGL / audio / font phía client; máy chủ suy ra một **visitor ID** ổn định (`HMAC-SHA256` với khóa phía máy chủ — client không thể giả mạo). | một phần |
| **Liên kết đa tài khoản** | Một visitor ID vẽ từ nhiều tài khoản sẽ được hiển thị cho quản trị viên trong `/admin/users` (`fp_accounts`, `fp_linked_users`). Cung cấp năng lực cho quy tắc `ALLOW_MULTI_ACCOUNT`. | không |
| **Chấm điểm hành vi** | Mỗi request vẽ đều được quan sát phía máy chủ: khoảng cách request đều đặn như máy móc, kích thước lô đồng đều hoàn hảo, UA tự động hóa (`HeadlessChrome`, `Puppeteer`, `python-requests`, `navigator.webdriver`), thiếu fingerprint. | **có** |
| **Thử thách proof-of-work** | Người dùng bị đánh dấu xóa điểm của mình bằng cách giải một PoW SHA-256 (`/fp/challenge`) — người dùng thật không bao giờ nhận ra; các trang trại script bị đốt CPU. | có |

### Các tín hiệu làm tăng điểm bot của người dùng

| Tín hiệu | Trọng số |
|---|---|
| Khoảng cách vẽ đều đặn như máy móc (hệ số biến thiên < 0,08) | +40 |
| Kích thước lô đồng đều hoàn hảo qua nhiều request | +25 |
| User agent headless / tự động hóa, `navigator.webdriver` | +60 |
| Khối lượng vẽ cao nhưng chưa bao giờ thu được fingerprint | +20 |

### Thực thi

`ANTI_BOT_MODE` — `off` / `log` (mặc định: quan sát, hiển thị điểm cho quản trị viên) / `enforce`: người dùng vượt `ANTI_BOT_ENFORCE_THRESHOLD` sẽ nhận 403 khi vẽ cho đến khi giải xong một PoW. Quản trị viên và người kiểm duyệt luôn được miễn trừ. Toàn bộ nội dung này có thể chỉnh sửa ngay lúc chạy trong `/admin/customize` — kể cả việc chuyển sang `enforce` giữa chừng khi đang bị tấn công. Khi `ALLOW_BOTS=true` (quy tắc cộng đồng), hãy giữ `log`: cộng đồng bot vẫn hiển thị rõ nhưng không bao giờ bị chặn.

> [LƯU Ý]
> Không có cơ chế fingerprinting nào đánh bại được một đối thủ quyết tâm với
> tự động hóa trình duyệt tinh vi — lớp phòng thủ ở đây làm tăng chi phí của
> tự động hóa hàng loạt và khiến nó hiển thị với người kiểm duyệt, điều mà hệ
> thống charge, báo cáo và lệnh cấm IP theo chuỗi hoàn thiện. Chỉ những hash
> phái sinh và các trường thô đại khái được lưu (không lưu dữ liệu canvas/audio
> thô), giữ cho dữ liệu lưu trữ ở mức tối thiểu.



### Chạy đằng sau Cloudflare

Backend phân giải IP client giống hệt backend Node gốc: `cf-connecting-ip` → `x-forwarded-for` (phần tử đầu tiên) → địa chỉ socket. Lệnh cấm IP, giới hạn tốc độ và thống kê đều dựa trên IP đã phân giải này, nên mọi thứ hoạt động ngay tức khắc đằng sau Cloudflare (hoặc bất kỳ reverse proxy nào đặt các header này).

> [QUAN TRỌNG]
> Các header này được tin tưởng vô điều kiện (giống hệt backend gốc), nên
> phải chặn truy cập trực tiếp vào origin — nếu không, client có thể giả mạo
> `cf-connecting-ip` để né lệnh cấm IP / giới hạn tốc độ. Hãy giới hạn cổng 3000
> cho các dải IP của Cloudflare, hoặc đặt Caddy / Cloudflare Tunnel phía trước.

## Benchmark: cách chúng tôi đo

Công cụ tạo tải đi kèm backend (`backend-rs/src/bin/loadgen.rs`) — tái tạo bảng phía trên bằng:

```sh
# nạp dữ liệu giống hệt nhau vào cả hai stack
DATABASE_URL="postgres://…" backend-rs/target/release/openplace-backend seed-bench \
  --regions 1000 --users 5000 --tiles 20

# dồn tải vào một trong hai stack (ví dụ)
backend-rs/target/release/loadgen --url http://127.0.0.1:3900 \
  --scenario tile --tile 0,0 --conns 32 --duration 20
backend-rs/target/release/loadgen --url http://127.0.0.1:3100 \
  --scenario paint --paint 25 --logins 50 --conns 50 --duration 20
```

Ghi chú về tính công bằng: endpoint bảng xếp hạng bị chặn bởi cùng truy vấn tổng hợp SQL mà cả hai stack đều chạy, nên hệ số khiêm tốn chỉ 1,4×; các con số vẽ bao gồm cả commit DB đồng bộ (việc trừ charge là bền vững ở cả hai stack trước khi trả lời).

## Thêm bản dịch

> [CẢNH BÁO ⚠️]
> Các đóng góp có sử dụng AI sẽ bị từ chối, và bạn **SẼ** bị cấm khỏi repo này. Bạn phải thành thạo ngôn ngữ mà bạn dịch.

Để đóng góp vào repo này và dịch `README.md` cùng các file cài đặt khác, vui lòng làm theo các bước sau.

### Thay đổi số phiên bản ở đầu README này để chỉ ra rằng một ngôn ngữ mới đã được thêm

Số phiên bản được định dạng dưới dạng `X.XX`, trong đó chữ "X" đầu tiên đại diện cho số lượng ngôn ngữ đã được dịch chính thức tính đến nay. Bộ chữ "X" thứ hai sau dấu chấm được thay đổi mỗi khi bản tiếng Anh của README có bất kỳ chỉnh sửa nào.
Số phiên bản này giúp người dịch biết khi nào họ cần cập nhật nội dung bản dịch hiện có của mình.

### Tạo một thư mục mới trong thư mục `translations` đặt tên theo mã ISO của ngôn ngữ của bạn

Nếu bạn không chắc mã ISO của mình là gì, bạn có thể kiểm tra [tại đây](https://gist.githubusercontent.com/josantonius/b455e315bc7f790d14b136d61d9ae468/raw/416def351bc1f790d14b136d61d9ae468/language-codes.json) hoặc đơn giản là tìm kiếm trên mạng. Bạn cần tìm một mã gồm hai chữ cái, ví dụ như `"vi"` cho tiếng Việt.

### Sao chép các file tiếng Anh vào thư mục mới của bạn

Sao chép các file tiếng Anh từ thư mục `translations` và file `README.md` chính vào thư mục bạn vừa tạo.
Lúc này bạn sẽ có bốn file: `README.md` và ba file markdown (`.md`) hướng dẫn cài đặt.

### Thêm đúng cờ vào cả hai README

Khi tạo một bản dịch mới, bạn phải cập nhật **hai** file README:

#### 1. **README tiếng Anh gốc**

Chỉ thêm **lá cờ của quốc gia/ngôn ngữ bạn đang dịch sang** ở phần đầu.
Lá cờ này phải liên kết đến README dịch mới của bạn.

Sử dụng mẫu này:

```html
<a href="translations/LANGUAGE_ISO_CODE/NAME_OF_YOUR_README.md"><img src="https://flagcdn.com/256x192/LANGUAGE_ISO_CODE.png" width="48" alt="NAME_OF_COUNTRY Flag"></a>
```

Thay các chỗ giữ chỗ bằng mã ISO và tên quốc gia cho bản dịch của bạn.

#### 2. **README đã dịch của bạn**

Ở đầu README đã dịch của bạn, chỉ thêm **lá cờ Mỹ**, liên kết ngược về README tiếng Anh.

> [CẢNH BÁO ⚠️]
> Các lá cờ trong README tiếng Anh phải được giữ theo thứ tự bảng chữ cái dựa theo mã ISO.

### Cập nhật các liên kết trong phần Bắt đầu

Trong phần **Bắt đầu**, cập nhật các liên kết để chúng trỏ đến các file đã dịch của bạn.
Nếu bạn không biết cách làm, hãy tham khảo một thư mục ngôn ngữ khác (ví dụ như `fr`).

### Dịch tất cả các file

Dịch toàn bộ các file một cách đầy đủ và chính xác.
Khi đã hoàn tất, hãy tạo một pull request. Một người đóng góp hoặc người dùng sẽ xác minh công việc của bạn.
**Đừng quên:** việc sử dụng AI bị nghiêm cấm tuyệt đối và sẽ dẫn đến lệnh cấm vĩnh viễn nếu bị phát hiện.

### Kiểm tra công việc của bạn

Nhấn vào **TẤT CẢ** các liên kết và lá cờ. Mỗi cái phải hoạt động chính xác và dẫn đến file hoặc trang web phù hợp.
Nếu có gì đó không hoạt động đúng, hãy sửa nó trước khi gửi pull request.
Khi mọi thứ hoạt động như mong đợi, bạn có thể tự tin mở pull request của mình.
Hãy nhớ rằng: các hướng dẫn này sẽ được xem xét lại đối với tất cả các bản dịch để đảm bảo tuân thủ đầy đủ.

## Giấy phép

Được cấp phép theo Giấy phép Apache, phiên bản 2.0. Tham khảo [LICENSE.md](https://github.com/13MrBlackCat13/openplace/blob/main/LICENSE.md).

### Lời cảm ơn

Dữ liệu khu vực lấy từ [GeoNames Gazetteer](https://download.geonames.org/export/dump/), và được cấp phép theo [Creative Commons Attribution 4.0 License](https://creativecommons.org/licenses/by/4.0/). Dữ liệu được cung cấp "nguyên trạng" mà không có bất kỳ bảo đảm hay cam kết nào về độ chính xác, tính kịp thời hoặc đầy đủ.
