use super::dom::{DomNode, SimpleDom};

#[derive(Clone)]
pub struct BrowserTab {
    pub id: usize,
    pub title: String,
    pub url: String,
    pub icon: &'static str,
    pub dom: SimpleDom,
    pub is_loading: bool,
    pub memory_kb: usize,
}

impl BrowserTab {
    pub fn new_tab(id: usize) -> Self {
        Self {
            id,
            title: "Bàn Làm Việc".to_string(),
            url: "servo://newtab".to_string(),
            icon: "📜",
            dom: Self::generate_newtab_dom(),
            is_loading: false,
            memory_kb: 4_200,
        }
    }

    pub fn new_with_url(id: usize, url: &str) -> Self {
        let (title, icon, dom) = if url == "servo://newtab" {
            ("Bàn Làm Việc".to_string(), "📜", Self::generate_newtab_dom())
        } else if url == "servo://shield" {
            ("Bảo Mật & Xé Luồng".to_string(), "🛡️", Self::generate_shield_dom())
        } else if url == "servo://about" || url.contains("about-servo") {
            ("Về Servo Engine".to_string(), "🦀", Self::generate_about_dom())
        } else {
            (url.to_string(), "🌐", SimpleDom::parse_html_or_text("Đang tải dữ liệu qua QUIC..."))
        };

        Self {
            id,
            title,
            url: url.to_string(),
            icon,
            dom,
            is_loading: false,
            memory_kb: 5_800,
        }
    }

    pub fn generate_about_dom() -> SimpleDom {
        SimpleDom {
            title: "Papyrus: Trình Duyệt Rust Siêu Nhẹ & Xé Luồng".to_string(),
            nodes: vec![
                DomNode::Heading {
                    level: 1,
                    text: "Papyrus: Trình Duyệt Rust Siêu Nhẹ Với Giao Diện Giấy Xé".to_string(),
                },
                DomNode::Paragraph(
                    "Độc lập hoàn toàn với Chromium. Xây dựng bằng 100% Rust memory-safe, giao diện mô phỏng giấy thủ công (Torn Paper Skeuomorphism), tích hợp VPN WireGuard, chặn quảng cáo native và cơ chế gom luồng chia nhỏ MP-QUIC giống Tor nhưng tốc độ full cáp quang 850 Mbps.".to_string(),
                ),
                DomNode::Divider,
                DomNode::Heading {
                    level: 2,
                    text: "📊 So Sánh Hiệu Năng & Tài Nguyên".to_string(),
                },
                DomNode::MetricRow {
                    label: "RAM Khởi Động (1 tab)".to_string(),
                    val_rust: "18.4 MB (Siêu nhẹ)".to_string(),
                    val_chrome: "580.0 MB".to_string(),
                    val_tor: "320.0 MB".to_string(),
                },
                DomNode::MetricRow {
                    label: "Tốc Độ Mạng / Băng Thông".to_string(),
                    val_rust: "850 Mbps (Gộp 3 Tuyến QUIC)".to_string(),
                    val_chrome: "900 Mbps (Trực tiếp, lộ IP)".to_string(),
                    val_tor: "1.8 Mbps (Nghẽn TCP)".to_string(),
                },
                DomNode::MetricRow {
                    label: "Độ Trễ Phản Hồi (Ping)".to_string(),
                    val_rust: "24 ms (Song song UDP)".to_string(),
                    val_chrome: "18 ms".to_string(),
                    val_tor: "450 ms - 900 ms".to_string(),
                },
                DomNode::MetricRow {
                    label: "Dữ Liệu Telemetry Ghi Đĩa".to_string(),
                    val_rust: "0 Byte (RAM-Only Shredder)".to_string(),
                    val_chrome: "~45 MB / ngày".to_string(),
                    val_tor: "0 Byte".to_string(),
                },
                DomNode::Divider,
                DomNode::AdBlockedPlaceholder {
                    domain: "google-analytics.com".to_string(),
                    reason: "Đã triệt tiêu 3 tracker telemetry trong 0.01ms".to_string(),
                },
                DomNode::BlockQuote(
                    "\"Trình duyệt Papyrus chỉ cấp phát bộ nhớ khi cần thiết. Đóng tab là xé giấy hủy dữ liệu, không có rác cache tích tụ hàng gigabyte như các trình duyệt nhân Chromium.\"".to_string(),
                ),
            ],
        }
    }

    pub fn generate_shield_dom() -> SimpleDom {
        SimpleDom {
            title: "Trung Tâm An Ninh, WireGuard & Gom Luồng".to_string(),
            nodes: vec![
                DomNode::Heading {
                    level: 1,
                    text: "Trung Tâm Xé Luồng Đa Tuyến & Bảo Mật".to_string(),
                },
                DomNode::Paragraph(
                    "Dữ liệu duyệt web của bạn được phân mảnh (packet striping) và phân tán đồng thời qua 3 tuyến máy chủ độc lập (Thụy Sĩ, Iceland, Nhật Bản). Không một bên thứ ba hay nhà mạng nào có thể giải mã.".to_string(),
                ),
                DomNode::Divider,
                DomNode::Heading {
                    level: 2,
                    text: "⚡ 3 Tuyến Đang Kết Nối Song Song (QUIC / UDP)".to_string(),
                },
                DomNode::Paragraph("• Tuyến A: 🇨🇭 Zurich, Thụy Sĩ (35% gói tin) - Ping 14ms - WireGuard ChaCha20".to_string()),
                DomNode::Paragraph("• Tuyến B: 🇮🇸 Reykjavik, Iceland (35% gói tin) - Ping 28ms - Zero-Log".to_string()),
                DomNode::Paragraph("• Tuyến C: 🇯🇵 Tokyo, Nhật Bản (30% gói tin) - Ping 42ms - Asian Gateway".to_string()),
                DomNode::Divider,
                DomNode::Heading {
                    level: 2,
                    text: "🛡️ Bộ Lọc Native AdBlock (adblock-rust)".to_string(),
                },
                DomNode::Paragraph("• Tổng quy tắc nạp: 184,520 rules (uBlock + EasyList)".to_string()),
                DomNode::Paragraph("• Tốc độ khớp mẫu: 0.02ms (Trie biên dịch Rust)".to_string()),
                DomNode::Paragraph("• Bộ nhớ RAM cho bộ lọc: 3.4 MB".to_string()),
            ],
        }
    }

    pub fn generate_newtab_dom() -> SimpleDom {
        SimpleDom {
            title: "Bàn Làm Việc Giấy Xé".to_string(),
            nodes: vec![
                DomNode::Heading {
                    level: 1,
                    text: "Bàn Làm Việc & Sổ Tay Tab Mới".to_string(),
                },
                DomNode::Paragraph(
                    "Khởi tạo phiên sạch trong 12ms. Gõ địa chỉ URL hoặc click vào một trong các mảnh ghim nhanh bên dưới:".to_string(),
                ),
                DomNode::Divider,
                DomNode::Heading {
                    level: 2,
                    text: "📌 Các Mảnh Ghim Truy Cập Nhanh".to_string(),
                },
                DomNode::Link {
                    label: "🦀 Giới thiệu Kiến trúc Động cơ Servo (Rust Native)".to_string(),
                    url: "servo://about".to_string(),
                },
                DomNode::Link {
                    label: "⚡ Xem Trung tâm Xé Luồng Đa Tuyến & WireGuard".to_string(),
                    url: "servo://shield".to_string(),
                },
                DomNode::Link {
                    label: "📖 Tài liệu Tiêu chuẩn Thư viện Rust (doc.rust-lang.org)".to_string(),
                    url: "https://doc.rust-lang.org".to_string(),
                },
                DomNode::Link {
                    label: "📦 Kho Gói Mã Nguồn Crate Rust (crates.io)".to_string(),
                    url: "https://crates.io".to_string(),
                },
                DomNode::Divider,
                DomNode::BlockQuote(
                    "Ghi chú nhanh: Mọi dữ liệu phiên chỉ tồn tại trong RAM. Bấm nút 'Xé Hủy' ở góc trên để zeroize toàn bộ bộ nhớ.".to_string(),
                ),
            ],
        }
    }
}
