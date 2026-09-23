# openplace — Hướng dẫn cài đặt cho macOS

Hướng dẫn này sẽ giúp bạn chuẩn bị một máy **macOS** để chạy **openplace** từ mã nguồn (backend Rust).

---

## Bước 1: Cài đặt các yêu cầu trước
Hãy chắc chắn rằng bạn có những thứ sau đây đã được cài đặt trên hệ thống của bạn:
- **Homebrew**
- **rustup** (Rust 1.85+)
- **Git**
- **PostgreSQL 15+** (khuyến nghị 17)

```bash
brew install git postgresql@17
brew services start postgresql@17
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Hoặc chạy PostgreSQL bằng Docker:

```bash
docker run -d --name openplace-pg \
  -e POSTGRES_DB=openplace -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=password -p 5432:5432 postgres:17-alpine
```

---

## Bước 2: Sao chép repo
```bash
git clone --recurse-submodules https://github.com/13MrBlackCat13/openplace.git
cd openplace
```

> [LƯU Ý]
> `--recurse-submodules` là bắt buộc: frontend Nuxt là một git submodule.

---

## Bước 3: Cấu hình môi trường

```bash
cp .env.example .env
```

Chỉnh sửa `.env` và cấu hình cài đặt của bạn:
- Đặt `DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"` (thay `password` bằng mật khẩu PostgreSQL của bạn)
- Đặt `JWT_SECRET` thành một chuỗi ký tự ngẫu nhiên dài

> [CẢNH BÁO ⚠️]
> Nếu dùng ký tự đặc biệt trong biến môi trường, hãy mã hóa theo bảng: [Percent-Encoding](https://developer.mozilla.org/en-US/docs/Glossary/Percent-encoding)

---

## Bước 4: Build và khởi tạo

```bash
cd backend-rs
cargo run --release -- setup          # migration + người dùng hệ thống
cargo run --release -- import-geonames cities1000.zip
```

Dữ liệu khu vực GeoNames (`cities500.zip` nhỏ nhất, `cities1000.zip`, `cities5000.zip`, `allCountries.zip` lớn nhất) có thể tải từ [download.geonames.org](https://download.geonames.org/export/dump/). Bộ nhập chấp nhận cả `.zip` lẫn TSV thô.

---

## Bước 5: Chạy ứng dụng

```bash
export DATABASE_URL="postgres://postgres:password@localhost:5432/openplace"
cargo run --release -- serve          # HTTP API trên BACKEND_PORT (mặc định 3000)
```

> [LƯU Ý]
> Máy chủ phục vụ thư mục `frontend/` (git submodule) dưới dạng file tĩnh, phân giải qua `FRONTEND_DIR` (mặc định `./frontend`) **tính theo thư mục làm việc hiện tại**. Khi chạy binary từ `backend-rs/`, hãy đặt:

```bash
export FRONTEND_DIR="../frontend"
```

> [MẸO]
> `setup`, `import-geonames`, `serve` và các lệnh khác là subcommand của binary `openplace-backend` duy nhất — tham khảo đầy đủ nằm trong [README](README.md).

---

## Lưu ý đối với SSL
openplace **yêu cầu HTTPS**.  
Nếu bạn đang thử nghiệm cục bộ, bạn có thể truy cập ứng dụng tại:
```
https://{IP}:8080
```
⚠️ Cố gắng sử dụng HTTP sẽ báo **lỗi HTTP 400**.

---

## Cập nhật cơ sở dữ liệu
Nếu schema cơ sở dữ liệu thay đổi, hãy chạy lại migration:
```bash
cargo run --release -- setup
```
