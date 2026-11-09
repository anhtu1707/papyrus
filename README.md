# Papyrus 0.3.1

Bản 0.3.1 thống nhất font các trang nội bộ sang Segoe UI/Tahoma/Arial để tránh chữ tiếng Việt dùng lẫn Georgia và Times New Roman. Chỉ thay đổi chữ của giao diện và tab mới, không ghi đè font do website lựa chọn.

Trình duyệt Windows với giao diện giấy, viết bằng Rust/Wry và dùng Microsoft Edge WebView2 (Chromium) để hiển thị website.

## Chạy ứng dụng

Mở `Run-Papyrus.bat` ở thư mục cha, hoặc mở `dist/Papyrus.exe`. Máy cần Windows 10/11 x64 và [Microsoft Edge WebView2 Evergreen Runtime](https://developer.microsoft.com/microsoft-edge/webview2/). Ứng dụng báo lỗi khởi động nếu không tạo được engine hoặc hồ sơ. Chỉ mở một tiến trình trên cùng một hồ sơ.

Từ mã nguồn, cài Rust stable với MSVC và Visual Studio C++ Build Tools, rồi chạy:

```powershell
cargo run --release --locked
```

## Chức năng hiện có

- Mỗi tab có WebView, document và lịch sử riêng; chuyển tab giữ nội dung đang mở. Đóng tab khác không làm đổi nhầm tab đang chọn.
- Thanh địa chỉ hỗ trợ HTTP/HTTPS, tìm kiếm Google/DuckDuckGo/Brave và localhost; URL được lấy từ sự kiện native, kể cả điều hướng SPA. Tiêu đề và ghim được hiển thị dưới dạng văn bản.
- Ghim, công cụ tìm kiếm, theme giấy, zoom engine, thanh ghim, startup và ngoại lệ bộ lọc được lưu qua lần khởi động.
- Khôi phục **địa chỉ** tối đa 100 tab khi bật startup “Khôi phục”; không hứa khôi phục form hoặc video sau khi thoát.
- Bộ lọc tên miền chặn request qua WebView2 trước khi gửi; hỗ trợ tên miền con, danh sách bổ sung và ngoại lệ theo website/tài nguyên. Danh sách tích hợp cục bộ, không tự cập nhật và không phải bộ chặn quảng cáo toàn diện.
- Popup do người dùng mở tạo tab riêng và giữ `window.opener`; popup tự động bị chặn theo cài đặt.
- Tải tệp vào thư mục đã chọn, theo dõi kết quả trong Cài đặt, tránh ghi đè tệp trùng tên và không tự chạy tệp.
- Xóa dữ liệu đóng các tab trước, chờ callback WebView2 xác nhận thành công hoặc báo lỗi, giữ ghim và cài đặt.

Phím tắt trong website: Ctrl+L mở thanh địa chỉ; Ctrl+T tạo tab; Ctrl+W đóng tab; Ctrl+D ghim; Ctrl+R/F5 tải lại; Alt+Left/Right quay lại/đi tiếp. Quyền camera/microphone và các quyền website do WebView2 hỏi người dùng, không được ứng dụng tự cấp.

## Dữ liệu và giới hạn

Hồ sơ website được lưu **trên đĩa** tại `%LOCALAPPDATA%/Papyrus/profile`. Cài đặt/ghim nằm ở `state.json`, tách khỏi profile. Có thể dùng biến môi trường `PAPYRUS_DATA_DIR` để chọn thư mục riêng. Xóa dữ liệu là xóa logic của WebView2, không phải secure erase ổ đĩa; ghim và cài đặt được giữ lại. Các thư mục phiên của bản cũ trong `%TEMP%/papyrus_sess_*` không được tự ý xóa.

Papyrus **chưa có** VPN, WireGuard, MP-QUIC, đổi IP/quốc gia, chống fingerprint hoặc chế độ chặn rò IP WebRTC chuyên biệt. Nếu hệ điều hành đang dùng VPN, lưu lượng tuân theo cấu hình mạng hệ điều hành. Không có các số đo RAM, Mbps, ping hoặc telemetry giả trên giao diện. Bộ lọc không bảo đảm chặn quảng cáo YouTube hay mọi tracker. Chưa tích hợp extension store, đồng bộ tài khoản, password manager hoặc trình cập nhật ứng dụng tự động.

Cookie/login được engine lưu theo hồ sơ. Một số website có thể từ chối trình duyệt nhúng; kiểm thử localhost không chứng minh mọi website hoặc mọi luồng đăng nhập bên thứ ba tương thích.

## Kiểm thử

```powershell
# Các kiểm tra Rust không cần giao diện
./scripts/verify.ps1 -Offline

# Cài công cụ kiểm thử frontend (chỉ cần cho phát triển)
python -m pip install -r tests/requirements.txt
python -m playwright install chromium

# Toàn bộ kiểm tra, cần desktop Windows và WebView2
./scripts/verify.ps1 -Offline -Native -Frontend
```

Test native chỉ dùng HTTP localhost và hồ sơ thử mới trong `target/webview-test-*`; không đụng hồ sơ người dùng. Nó kiểm tra chặn request, ngoại lệ, cách ly IPC, tab giữ form, lịch sử/SPA URL, chống chèn title, popup opener, zoom engine, xóa cookie/localStorage, tải file không ghi đè và ghi trạng thái khôi phục/ghim. `cargo test` mặc định bỏ qua test native có yêu cầu desktop; dùng `-- --include-ignored --test-threads=1` để chạy tất cả.

`tests/frontend.py` dùng Chromium headless kiểm tra chính tài nguyên được nhúng vào binary: DOM an toàn, URL có dấu nháy, cài đặt, theme, xác nhận xóa và layout từ 680–1920 px. Ảnh chụp kết quả nằm trong `target/settings-*.png`.

## Đóng gói

```powershell
./scripts/package.ps1 -Offline
```

Script kiểm tra trước khi build, tạo `dist/Papyrus.exe`, ZIP và SHA256. `-SkipChecks` chỉ dùng khi bộ kiểm tra đã được chạy. CI Windows thực hiện cùng quy trình; chưa có kết quả CI từ GitHub cho tới khi đẩy repository lên đó.

## Bố cục mã

- `src/main.rs`: quản lý tab, cửa sổ, cài đặt, dữ liệu và sự kiện ứng dụng.
- `src/model.rs`: URL, lệnh IPC, bộ lọc, cài đặt và các test lõi.
- `src/native.rs`: WebView2 COM, request filter, nguồn URL, popup, phím tắt và xóa dữ liệu.
- `src/integration_tests.rs`: kiểm thử WebView2 bằng hồ sơ cô lập.
- `assets/`: HTML/CSS/JavaScript được nhúng vào executable, không tải giao diện từ Internet.
- `archive/prototype-v0.2/`: bản mô phỏng cũ, không được biên dịch hoặc quảng cáo là tính năng hoạt động.

IPC chỉ được gắn vào header nội bộ bị khóa điều hướng. Content WebView không có handler và bị tắt WebMessage/host objects. Bộ lọc sử dụng [WebResourceRequested](https://learn.microsoft.com/microsoft-edge/webview2/how-to/webresourcerequested), bao gồm request source kinds trên WebView2 hỗ trợ; yêu cầu cập nhật Runtime nếu không khởi tạo được interface cần thiết.
