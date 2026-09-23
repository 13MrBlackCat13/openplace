# openplace — Hướng dẫn cài đặt cho Windows

Hướng dẫn này sẽ giúp bạn chuẩn bị một máy **Windows** để chạy **openplace** từ mã nguồn (backend Rust).

---

## 1. Cài đặt các yêu cầu trước

Bạn cần **Rust**, **Git** và **PostgreSQL 15+** (khuyến nghị 17).

- Cài **rustup** (Rust 1.85+) — dùng **winget** (Windows 10/11 PowerShell với quyền Quản trị) hoặc tải từ [rustup.rs](https://rustup.rs/):

```powershell
winget install Rustlang.Rustup
```

- Cài **Git**:

```powershell
winget install Git.Git
```

- **PostgreSQL**: cài từ [postgresql.org](https://www.postgresql.org/download/windows/), hoặc chạy bằng Docker:

```powershell
docker run -d --name openplace-pg `
  -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres `
  -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine
```

---

## 2. Sao chép repo

```powershell
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

> [LƯU Ý]
> `--recurse-submodules` là bắt buộc: frontend Nuxt là một git submodule.

---

## 3. Cấu hình môi trường

1. Sao chép `.env.example` thành `.env`:

```powershell
Copy-Item .env.example .env
```

2. Chỉnh sửa `.env` và cấu hình cài đặt của bạn:
    - Đặt `DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"` (thay `password` bằng mật khẩu PostgreSQL của bạn)
    - Đặt `JWT_SECRET` thành một chuỗi ký tự ngẫu nhiên dài

> [CẢNH BÁO ⚠️]
> Nếu dùng ký tự đặc biệt trong biến môi trường, hãy mã hóa theo bảng: [Percent-Encoding](https://developer.mozilla.org/en-US/docs/Glossary/Percent-encoding)

---

## 4. Build và khởi tạo

```powershell
cd backend-rs
cargo run --release -- setup          # migration + người dùng hệ thống
cargo run --release -- import-geonames cities1000.zip
```

Dữ liệu khu vực GeoNames (`cities500.zip` nhỏ nhất, `cities1000.zip`, `cities5000.zip`, `allCountries.zip` lớn nhất) có thể tải từ [download.geonames.org](https://download.geonames.org/export/dump/). Bộ nhập chấp nhận cả `.zip` lẫn TSV thô.

---

## 5. Chạy máy chủ

```powershell
$env:DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
cargo run --release -- serve          # HTTP API trên BACKEND_PORT (mặc định 3000)
```

> [LƯU Ý]
> Máy chủ phục vụ thư mục `frontend/` (git submodule) dưới dạng file tĩnh, phân giải qua `FRONTEND_DIR` (mặc định `./frontend`) **tính theo thư mục làm việc hiện tại**. Khi chạy binary từ `backend-rs/`, hãy đặt:

```powershell
$env:FRONTEND_DIR="../frontend"
```

> [MẸO]
> `setup`, `import-geonames`, `serve` và các lệnh khác là subcommand của binary `openplace-backend` duy nhất — tham khảo đầy đủ nằm trong [README](README.md).

---

## Truy cập máy chủ của bạn

- Đối với production, hãy cấu hình chứng chỉ SSL (ví dụ với [Caddy](https://caddyserver.com/) làm reverse proxy).
- Đối với sử dụng cục bộ/riêng tư, điều hướng đến:

```
https://{your-local-IP}:8080
```

> [CẢNH BÁO ⚠️]
> **Quan trọng:** openplace chỉ hoạt động qua HTTPS. Nếu bạn truy cập qua HTTP, bạn sẽ nhận **400 Bad Request**.

---

## Cập nhật cơ sở dữ liệu

Nếu schema thay đổi, chạy lại migration:

```powershell
cargo run --release -- setup
```
