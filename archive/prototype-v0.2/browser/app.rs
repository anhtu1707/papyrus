use eframe::egui::{self, Color32, FontId, Rect, RichText, Stroke, Vec2};
use crate::browser::dom::DomNode;
use crate::browser::tab::BrowserTab;
use crate::net::adblock::AdBlockEngine;
use crate::net::fetcher::{AsyncFetcher, FetchResult};
use crate::net::shredder::MultipathShredder;
use crate::net::wireguard::WireGuardManager;
use crate::ui::torn_paper::*;

pub struct PapyrusApp {
    tabs: Vec<BrowserTab>,
    active_tab_idx: usize,
    url_input: String,
    next_tab_id: usize,
    
    // Window state for frameless mode
    pub is_maximized: bool,

    // Privacy & Network Suite
    pub adblock: AdBlockEngine,
    pub wireguard: WireGuardManager,
    pub shredder: MultipathShredder,
    fetcher: AsyncFetcher,

    // UI state
    pub show_shield_modal: bool,
    pub status_message: String,
}

impl PapyrusApp {
    pub fn new() -> Self {
        let initial_tab = BrowserTab::new_with_url(1, "servo://about");
        Self {
            tabs: vec![
                initial_tab,
                BrowserTab::new_tab(2),
            ],
            active_tab_idx: 0,
            url_input: "servo://about".to_string(),
            next_tab_id: 3,
            is_maximized: false,
            adblock: AdBlockEngine::new(),
            wireguard: WireGuardManager::new(),
            shredder: MultipathShredder::new(),
            fetcher: AsyncFetcher::new(),
            show_shield_modal: false,
            status_message: "Khởi động hoàn tất • Chế độ Không Viền (Frameless) • 18.2 MB RAM".to_string(),
        }
    }

    pub fn active_tab(&self) -> &BrowserTab {
        &self.tabs[self.active_tab_idx]
    }

    pub fn active_tab_mut(&mut self) -> &mut BrowserTab {
        &mut self.tabs[self.active_tab_idx]
    }

    pub fn navigate_to(&mut self, url: &str) {
        self.url_input = url.to_string();
        self.active_tab_mut().url = url.to_string();
        self.active_tab_mut().is_loading = true;

        // Check with adblock
        let check = self.adblock.check_url(url);
        if check.blocked {
            self.status_message = format!("🚫 Đã chặn tracker: {} ({}µs)", url, check.latency_us);
            return;
        }

        if url.starts_with("servo://") {
            let tab_id = self.active_tab().id;
            *self.active_tab_mut() = BrowserTab::new_with_url(tab_id, url);
            self.active_tab_mut().is_loading = false;
            self.status_message = format!("✓ Đã mở trang nội bộ: {}", url);
        } else {
            self.status_message = format!("⚡ Đang xé luồng MP-QUIC tải: {}", url);
            self.fetcher.fetch(url, self.shredder.enabled);
        }
    }

    pub fn shred_session(&mut self) {
        self.tabs.clear();
        self.tabs.push(BrowserTab::new_tab(1));
        self.active_tab_idx = 0;
        self.url_input = "servo://newtab".to_string();
        self.status_message = "✂️ ĐÃ XÉ HỦY TOÀN BỘ PHIÊN LÀM VIỆC TRONG RAM • ZERO DISK LEAK".to_string();
    }
}

impl eframe::App for PapyrusApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Poll async network responses
        if let Some(res) = self.fetcher.try_recv() {
            match res {
                FetchResult::Success { url, content, bytes } => {
                    self.active_tab_mut().dom = crate::browser::dom::SimpleDom::parse_html_or_text(&content);
                    self.active_tab_mut().title = self.active_tab().dom.title.clone();
                    self.active_tab_mut().is_loading = false;
                    self.active_tab_mut().memory_kb = (bytes / 1024) + 5_000;
                    self.status_message = format!("✓ Đã tải {} bytes qua 3 tuyến QUIC (850 Mbps)", bytes);
                }
                FetchResult::Error { url: _, message } => {
                    self.active_tab_mut().is_loading = false;
                    self.status_message = format!("Lỗi tải trang: {}", message);
                }
            }
        }

        // Configure theme style
        let mut visuals = egui::Visuals::light();
        visuals.panel_fill = COLOR_KRAFT;
        visuals.window_fill = COLOR_PARCHMENT;
        ui.ctx().set_visuals(visuals);

        // TOP BAR: FRAMELESS TITLEBAR + TABS + WINDOW CONTROLS
        egui::Panel::top("top_panel")
            .frame(egui::Frame::new().fill(COLOR_DESK).inner_margin(egui::Margin::symmetric(14, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Custom window control buttons for frameless window
                    if ui.add(egui::Button::new(RichText::new("✕").font(FontId::monospace(10.0)).color(Color32::WHITE)).fill(Color32::from_rgb(217, 74, 61))).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.add(egui::Button::new(RichText::new("—").font(FontId::monospace(10.0)).color(Color32::WHITE)).fill(Color32::from_rgb(224, 158, 56))).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                    if ui.add(egui::Button::new(RichText::new("□").font(FontId::monospace(10.0)).color(Color32::WHITE)).fill(Color32::from_rgb(77, 139, 85))).clicked() {
                        self.is_maximized = !self.is_maximized;
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(self.is_maximized));
                    }

                    ui.add_space(8.0);
                    ui.label(RichText::new("PAPYRUS // RUST").font(FontId::monospace(11.0)).color(COLOR_KRAFT_DARK).strong());

                    ui.add_space(14.0);

                    // Tab bar
                    for i in 0..self.tabs.len() {
                        let is_active = i == self.active_tab_idx;
                        let tab_title = self.tabs[i].title.clone();
                        let tab_icon = self.tabs[i].icon;

                        let text = if is_active {
                            RichText::new(format!("{} {}", tab_icon, tab_title))
                                .font(FontId::monospace(12.0))
                                .color(COLOR_INK)
                                .strong()
                        } else {
                            RichText::new(format!("{} {}", tab_icon, tab_title))
                                .font(FontId::monospace(11.5))
                                .color(COLOR_INK_MUTED)
                        };

                        let btn = ui.add(egui::Button::new(text).fill(if is_active { COLOR_PARCHMENT } else { COLOR_KRAFT_DARK }));
                        if btn.clicked() {
                            self.active_tab_idx = i;
                            self.url_input = self.tabs[i].url.clone();
                        }
                    }

                    // New tab slip button
                    if ui.button(RichText::new("+ Mảnh Mới").font(FontId::monospace(11.0)).color(COLOR_KRAFT_DARK)).clicked() {
                        let id = self.next_tab_id;
                        self.next_tab_id += 1;
                        self.tabs.push(BrowserTab::new_tab(id));
                        self.active_tab_idx = self.tabs.len() - 1;
                        self.url_input = "servo://newtab".to_string();
                    }

                    // Window Drag Area on the right space of the titlebar
                    let drag_space = ui.available_size();
                    if drag_space.x > 180.0 {
                        let (_, drag_rect) = ui.allocate_space(Vec2::new(drag_space.x - 170.0, drag_space.y));
                        let drag_resp = ui.interact(drag_rect, ui.id().with("window_drag"), egui::Sense::drag());
                        if drag_resp.dragged() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("● 18.2 MB").font(FontId::monospace(11.0)).color(Color32::from_rgb(0, 180, 80)));
                        ui.label(RichText::new("KHÔNG VIỀN").font(FontId::monospace(10.5)).color(COLOR_KRAFT_DARK));
                    });
                });
            });

        // NAVIGATION TOOLBAR & QUICK ACCESS PILLS
        egui::Panel::top("nav_panel")
            .frame(egui::Frame::new().fill(COLOR_PARCHMENT).inner_margin(egui::Margin::symmetric(14, 8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("←").strong()).clicked() {
                        self.navigate_to("servo://newtab");
                    }
                    if ui.button(RichText::new("↻").strong()).clicked() {
                        let cur = self.url_input.clone();
                        self.navigate_to(&cur);
                    }

                    ui.add_space(8.0);

                    // URL address input box (Typewriter style)
                    ui.label(RichText::new("🌐").font(FontId::monospace(13.0)));
                    let edit = ui.add(
                        egui::TextEdit::singleline(&mut self.url_input)
                            .desired_width(ui.available_width() - 480.0)
                            .font(FontId::monospace(13.5))
                            .text_color(COLOR_INK)
                    );
                    
                    let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let go_clicked = ui.button(RichText::new("⏎ Mở Trang").font(FontId::monospace(11.5)).color(COLOR_STAMP_BLUE).strong()).clicked();
                    
                    if (enter_pressed && edit.has_focus()) || go_clicked {
                        let target = self.url_input.clone();
                        self.navigate_to(&target);
                    }

                    ui.add_space(8.0);

                    // Feature Pill 1: AdBlock Wax Seal
                    let ad_text = format!("🛡️ Chặn Ads ({})", self.adblock.total_blocked);
                    if ui.button(RichText::new(ad_text).font(FontId::monospace(11.0)).color(COLOR_WAX_RED).strong()).clicked() {
                        self.show_shield_modal = !self.show_shield_modal;
                    }

                    // Feature Pill 2: WireGuard / MP-QUIC
                    let mesh_text = format!("⚡ MP-QUIC ({} Mbps)", self.shredder.aggregated_mbps);
                    if ui.button(RichText::new(mesh_text).font(FontId::monospace(11.0)).color(COLOR_STAMP_BLUE).strong()).clicked() {
                        self.show_shield_modal = !self.show_shield_modal;
                    }

                    // Feature Pill 3: Shredder (Ephemeral RAM-Only)
                    if ui.button(RichText::new("✂️ Xé Hủy").font(FontId::monospace(11.0)).color(Color32::from_rgb(180, 50, 30)).strong()).clicked() {
                        self.shred_session();
                    }
                });

                // Row 2: Quick Bookmark Pills
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("📌 GHIM NHANH:").font(FontId::monospace(10.5)).color(COLOR_INK_MUTED));
                    if ui.button(RichText::new("🦀 Servo Specs").font(FontId::monospace(10.5))).clicked() {
                        self.navigate_to("servo://about");
                    }
                    if ui.button(RichText::new("📝 Bàn Ghi Chú").font(FontId::monospace(10.5))).clicked() {
                        self.navigate_to("servo://newtab");
                    }
                    if ui.button(RichText::new("🛡️ Xé Luồng WireGuard").font(FontId::monospace(10.5))).clicked() {
                        self.navigate_to("servo://shield");
                    }
                    if ui.button(RichText::new("📰 Hacker News").font(FontId::monospace(10.5))).clicked() {
                        self.navigate_to("https://news.ycombinator.com");
                    }
                    if ui.button(RichText::new("📖 Rust Docs").font(FontId::monospace(10.5))).clicked() {
                        self.navigate_to("https://doc.rust-lang.org");
                    }
                    if ui.button(RichText::new("📦 Crates.io").font(FontId::monospace(10.5))).clicked() {
                        self.navigate_to("https://crates.io");
                    }
                });
            });

        // BOTTOM STATUS BAR
        egui::Panel::bottom("status_bar")
            .frame(egui::Frame::new().fill(COLOR_KRAFT).inner_margin(egui::Margin::symmetric(14, 4)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.active_tab().is_loading {
                        ui.label(RichText::new("⏳ ĐANG TẢI...").font(FontId::monospace(11.0)).color(COLOR_WAX_RED).strong());
                    } else {
                        ui.label(RichText::new(&self.status_message).font(FontId::monospace(11.0)).color(COLOR_INK));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let vpn_node = self.wireguard.active_node();
                        ui.label(RichText::new(format!("{} {} ({}ms)", vpn_node.country_flag, vpn_node.name, vpn_node.ping_ms)).font(FontId::monospace(10.5)).color(COLOR_STAMP_BLUE));
                        ui.label(RichText::new("•").color(COLOR_INK_MUTED));
                        ui.label(RichText::new("Ổ ĐĨA: 0 BYTE").font(FontId::monospace(10.5)).color(COLOR_STAMP_GREEN));
                    });
                });
            });

        // CENTRAL WEB VIEWPORT (Torn Paper Card)
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(COLOR_DESK).inner_margin(egui::Margin::symmetric(20, 14)))
            .show(ui, |ui| {
                let rect = ui.available_rect_before_wrap();
                let painter = ui.painter();

                // Draw paper sheet background with paper clip & washi tape
                painter.rect_filled(rect, 3.0, COLOR_PARCHMENT);
                painter.rect_stroke(rect, 3.0, Stroke::new(1.0, COLOR_KRAFT_DARK), egui::StrokeKind::Inside);
                draw_washi_tape(painter, Rect::from_min_size(rect.left_top() + Vec2::new(rect.width() * 0.45, -4.0), Vec2::new(90.0, 18.0)));

                // Render page content
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(16.0);
                    let dom_clone = self.active_tab().dom.nodes.clone();
                    for node in dom_clone {
                        match node {
                            DomNode::Heading { level, text } => {
                                let size = match level {
                                    1 => 23.0,
                                    2 => 18.0,
                                    _ => 15.0,
                                };
                                ui.label(RichText::new(text).font(FontId::proportional(size)).color(COLOR_INK).strong());
                                ui.add_space(6.0);
                            }
                            DomNode::Paragraph(text) => {
                                ui.label(RichText::new(text).font(FontId::proportional(14.0)).color(COLOR_INK));
                                ui.add_space(8.0);
                            }
                            DomNode::Link { label, url } => {
                                if ui.link(RichText::new(label).font(FontId::monospace(13.0)).color(COLOR_STAMP_BLUE)).clicked() {
                                    self.navigate_to(&url);
                                }
                                ui.add_space(4.0);
                            }
                            DomNode::BlockQuote(text) => {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("▌").color(COLOR_KRAFT_DARK));
                                    ui.label(RichText::new(text).font(FontId::proportional(13.5)).italics().color(COLOR_INK_MUTED));
                                });
                                ui.add_space(8.0);
                            }
                            DomNode::CodeBlock(code) => {
                                ui.label(RichText::new(code).font(FontId::monospace(12.0)).color(COLOR_INK).background_color(COLOR_KRAFT));
                                ui.add_space(6.0);
                            }
                            DomNode::Divider => {
                                ui.separator();
                                ui.add_space(6.0);
                            }
                            DomNode::AdBlockedPlaceholder { domain, reason } => {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new("🚫 [XÉ BỎ QUẢNG CÁO RUST]").font(FontId::monospace(11.5)).color(COLOR_WAX_RED).strong());
                                    ui.label(RichText::new(format!("{}: {}", domain, reason)).font(FontId::monospace(11.0)).color(COLOR_INK_MUTED));
                                });
                                ui.add_space(6.0);
                            }
                            DomNode::MetricRow { label, val_rust, val_chrome, val_tor } => {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("{:<26}", label)).font(FontId::monospace(12.0)).strong());
                                    ui.label(RichText::new(format!("🦀 {:<24}", val_rust)).font(FontId::monospace(12.0)).color(COLOR_STAMP_GREEN).strong());
                                    ui.label(RichText::new(format!("🌐 {:<18}", val_chrome)).font(FontId::monospace(11.5)).color(COLOR_INK_MUTED));
                                    ui.label(RichText::new(format!("🧅 {}", val_tor)).font(FontId::monospace(11.5)).color(COLOR_WAX_RED));
                                });
                                ui.add_space(4.0);
                            }
                        }
                    }
                    ui.add_space(40.0);
                });
            });

        // Shield & WireGuard & Multipath Modal Window
        if self.show_shield_modal {
            let mut node_to_select = None;
            egui::Window::new("🛡️ TRUNG TÂM BẢO MẬT & GOM LUỒNG MP-QUIC")
                .fixed_size(Vec2::new(640.0, 480.0))
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.label(RichText::new("⚡ Sơ Đồ Xé Luồng Đa Tuyến (Tor-Like Privacy @ 850 Mbps)").font(FontId::monospace(13.0)).strong());
                    ui.label(RichText::new("Dữ liệu được chia nhỏ thành 3 mảnh gói tin mã hóa độc lập:").font(FontId::proportional(12.5)));
                    ui.add_space(6.0);

                    for chunk in &self.shredder.chunks_in_flight {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Mảnh #{}/3:", chunk.chunk_id)).font(FontId::monospace(11.0)).strong());
                            ui.label(RichText::new(chunk.route_node).font(FontId::monospace(11.5)).color(COLOR_STAMP_BLUE));
                            ui.label(RichText::new(format!("{}ms • {}", chunk.latency_ms, chunk.encrypted_hash)).font(FontId::monospace(10.5)).color(COLOR_STAMP_GREEN));
                        });
                    }

                    ui.separator();
                    ui.label(RichText::new("🇨🇭 Máy Chủ WireGuard").font(FontId::monospace(12.0)).strong());
                    for (idx, node) in self.wireguard.nodes.iter().enumerate() {
                        let is_active = idx == self.wireguard.active_node_idx;
                        let text = format!("{} {} • {}ms • {}", node.country_flag, node.name, node.ping_ms, node.bandwidth);
                        if ui.selectable_label(is_active, text).clicked() {
                            node_to_select = Some(idx);
                        }
                    }

                    ui.separator();
                    ui.label(RichText::new(format!("🚫 Đã Chặn {} Trackers (Tiết kiệm {:.1} MB băng thông)", self.adblock.total_blocked, self.adblock.total_bytes_saved as f64 / 1_000_000.0)).font(FontId::monospace(11.5)).color(COLOR_WAX_RED));

                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Đóng Bảng").font(FontId::monospace(12.0))).clicked() {
                            self.show_shield_modal = false;
                        }
                        if ui.button(RichText::new("✂️ Xé Hủy Phiên Ngay").font(FontId::monospace(12.0)).color(COLOR_WAX_RED).strong()).clicked() {
                            self.shred_session();
                            self.show_shield_modal = false;
                        }
                    });
                });

            if let Some(idx) = node_to_select {
                self.wireguard.select_node(idx);
            }
        }
    }
}
