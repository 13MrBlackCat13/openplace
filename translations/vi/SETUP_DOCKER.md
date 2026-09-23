# openplace — Hướng dẫn thiết lập với Docker

Hướng dẫn này sẽ giúp bạn chạy **openplace** (backend Rust) với Docker.

## Yêu cầu trước khi bắt đầu

Bạn cần **Docker** với **Docker Compose v2** (cú pháp `docker compose` — dấu cách; cú pháp cũ `docker-compose` thuộc Compose v1 và không còn được hỗ trợ).

-   **Windows/macOS**: Tải Docker Desktop từ [docker.com](https://www.docker.com/products/docker-desktop/)
-   **Linux**: Làm theo hướng dẫn cho bản phân phối của bạn tại [docs.docker.com](https://docs.docker.com/engine/install/)

## 1. Sao chép repo

```bash
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

> [LƯU Ý]
> `--recurse-submodules` là bắt buộc: frontend Nuxt là một git submodule.

## 2. Cấu hình môi trường

Sao chép `.env.example` thành `.env`:

```bash
cp .env.example .env
```

Chỉnh sửa file `.env` và cấu hình cài đặt của bạn:

-   Đặt `JWT_SECRET` (tạo một chuỗi ký tự ngẫu nhiên an toàn — **bắt buộc**)
-   `DATABASE_URL` không cần thiết với Docker: container PostgreSQL được tạo tự động bởi Compose (chỉ đặt nếu bạn muốn trỏ tới cơ sở dữ liệu bên ngoài)

> [CẢNH BÁO ⚠️]
> Nếu dùng ký tự đặc biệt trong biến môi trường, hãy mã hóa theo bảng: [Percent-Encoding](https://developer.mozilla.org/en-US/docs/Glossary/Percent-encoding)

## 3. Khởi động ứng dụng

Chạy toàn bộ hệ thống với Docker Compose:

```bash
docker compose up -d --build
```

Lệnh này sẽ khởi động:

-   **PostgreSQL 17** kèm healthcheck
-   **openplace-backend** (backend Rust) — một binary tĩnh duy nhất
-   **Reverse proxy Caddy** tại cổng 80/443 — chờ app healthy trước khi proxy
-   **Frontend Nuxt** tại `127.0.0.1:3001`

## 4. Khởi tạo cơ sở dữ liệu và nhập dữ liệu khu vực

Khi các container đã chạy, tạo các bảng và người dùng hệ thống:

```bash
docker compose exec app openplace-backend setup
```

Sau đó nhập dữ liệu thành phố GeoNames (xem phần "Dữ liệu khu vực" trong [README](README.md)):

```bash
docker compose exec app openplace-backend import-geonames /path/to/cities1000.zip
```

Bạn có thể tải các file dump GeoNames (`cities500.zip` nhỏ nhất, `cities1000.zip`, `cities5000.zip`, `allCountries.zip` lớn nhất) từ [download.geonames.org](https://download.geonames.org/export/dump/). Bộ nhập chấp nhận cả `.zip` lẫn TSV thô. Không có dữ liệu khu vực, backend vẫn hoạt động, nhưng mọi pixel sẽ được ánh xạ về khu vực dự phòng.

## 5. Truy cập ứng dụng

Một khi toàn bộ dịch vụ đều đang chạy, bạn có thể truy cập openplace tại:

| Dịch vụ | Địa chỉ |
|---|---|
| Ứng dụng (qua Caddy) | `http://localhost` / `https://localhost` |
| HTTP API (trực tiếp) | `:3000` |
| Frontend Nuxt | `127.0.0.1:3001` |

> [CẢNH BÁO ⚠️]
> openplace chỉ hoạt động qua HTTPS. Nếu bạn thử tải trang bằng HTTP, bạn sẽ nhận **400 Bad Request**.

> [MẸO]
> `setup`, `import-geonames` và các lệnh khác là subcommand của binary `openplace-backend` duy nhất — chạy bên trong container bằng `docker compose exec app openplace-backend <lệnh>`, xem log bằng `docker compose logs -f app`. Tham khảo đầy đủ nằm trong [README](README.md).
