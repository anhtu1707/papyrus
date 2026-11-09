#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(test)]
mod integration_tests;
mod model;
mod native;

use model::{
    BOOKMARKS, Bookmark, Command, DOWNLOADS, EXTENSIONS, HISTORY, HistoryEntry, NEW_TAB, PASSWORDS,
    PasswordEntry, Preferences, SETTINGS, SavedState, TabInfo,
};
use serde::Serialize;
use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};
use tao::{
    dpi::{LogicalPosition, LogicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy},
    platform::run_return::EventLoopExtRunReturn,
    platform::windows::WindowExtWindows,
    window::{Window, WindowBuilder},
};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Environment;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;
use windows::core::{HSTRING, PCWSTR, w};
use wry::{Rect, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows};

type SharedPreferences = Rc<RefCell<Preferences>>;
#[derive(Debug)]
enum EventMessage {
    Command(Command),
    Source(u64, String),
    Title(u64, String),
    Loading(u64, u64),
    Finished(u64, u64, Option<String>),
    Blocked(u64),
    AudioPlaying(u64, bool),
    Muted(u64, bool),
    History,
    Popup,
    CloseTab(u64),
    ClearCompleted(Result<(), String>),
    Notice(String),
    Download(String),
    ProcessFailed(u64, i32, String),
    UpdateAvailable {
        version: String,
        url: String,
        notes: String,
        download_url: String,
    },
    Exit,
}
struct Tab {
    navigation: u64,
    info: TabInfo,
    view: Option<WebView>,
}

impl Tab {
    pub fn close_view(&mut self) {
        if let Some(view) = self.view.take() {
            let _ = native::close_view(&view);
        }
    }
}

impl Drop for Tab {
    fn drop(&mut self) {
        self.close_view();
    }
}
#[derive(Serialize)]
struct Snapshot<'a> {
    tabs: Vec<&'a TabInfo>,
    active: u64,
    preferences: &'a Preferences,
    bookmarks: &'a [Bookmark],
    history: &'a [HistoryEntry],
    passwords: &'a [PasswordEntry],
    blocked: u64,
    clearing: bool,
    can_back: bool,
    can_forward: bool,
    downloads: &'a [String],
    notice: &'a str,
    data_dir: String,
    version: &'static str,
    builtin_count: usize,
}
struct Browser {
    tabs: Vec<Tab>,
    closed_tabs: Vec<(String, String)>,
    active: u64,
    next_id: u64,
    header: WebView,
    // Views must be dropped before contexts. Profiles are persistent by design.
    content_context: WebContext,
    environment: Option<ICoreWebView2Environment>,
    window: Window,
    proxy: EventLoopProxy<EventMessage>,
    popups: native::Popups,
    preferences: SharedPreferences,
    saved: SavedState,
    state_path: PathBuf,
    data_dir: PathBuf,
    header_height: f64,
    blocked: u64,
    clearing: bool,
    downloads: Vec<String>,
    notice: String,
    header_ready: bool,
    download_paths: Rc<RefCell<Vec<PathBuf>>>,
}

const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,Translate,OptimizationHints,MediaRouter \
    --enable-features=TimerThrottling,HighPriorityBackgroundThreads,IntensiveWakeUpThrottling,SubframeShutdown,ResourceScheduler \
    --process-per-site --disk-cache-size=104857600 --js-flags=--max-old-space-size=512";

const ADBLOCK_INIT_SCRIPT: &str = r#"
(function() {
  if (!location.hostname.includes('youtube.com')) return;

  function stripAds(obj) {
    if (!obj || typeof obj !== 'object') return obj;
    delete obj.adPlacements;
    delete obj.playerAds;
    delete obj.adSlots;
    delete obj.adBreakHeartbeatParams;
    if (obj.playerResponse) stripAds(obj.playerResponse);
    return obj;
  }

  // Intercept initial player data
  try {
    let _ytInitialPlayerResponse = window.ytInitialPlayerResponse;
    Object.defineProperty(window, 'ytInitialPlayerResponse', {
      get() { return _ytInitialPlayerResponse; },
      set(val) { _ytInitialPlayerResponse = stripAds(val); },
      configurable: true
    });
    if (_ytInitialPlayerResponse) stripAds(_ytInitialPlayerResponse);
  } catch(e) {}

  // Intercept fetch API on /player
  try {
    const origFetch = window.fetch;
    window.fetch = async function(...args) {
      const res = await origFetch.apply(this, args);
      const url = typeof args[0] === 'string' ? args[0] : (args[0] && args[0].url) || '';
      if (url.includes('/player') || url.includes('youtubei/v1/player')) {
        try {
          const clone = res.clone();
          const json = await clone.json();
          stripAds(json);
          const text = JSON.stringify(json);
          return new Response(text, {
            status: res.status,
            statusText: res.statusText,
            headers: res.headers
          });
        } catch(e) {}
      }
      return res;
    };
  } catch(e) {}

  // Intercept XMLHttpRequest on /player
  try {
    const origXhrOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function(method, url, ...rest) {
      this._url = url;
      return origXhrOpen.apply(this, [method, url, ...rest]);
    };
    const origXhrSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.send = function(...args) {
      if (this._url && (this._url.includes('/player') || this._url.includes('youtubei/v1/player'))) {
        this.addEventListener('readystatechange', function() {
          if (this.readyState === 4 && this.status === 200) {
            try {
              const data = JSON.parse(this.responseText);
              stripAds(data);
              Object.defineProperty(this, 'responseText', { value: JSON.stringify(data), configurable: true });
              Object.defineProperty(this, 'response', { value: JSON.stringify(data), configurable: true });
            } catch(e) {}
          }
        });
      }
      return origXhrSend.apply(this, args);
    };
  } catch(e) {}

  // Hide ad DOM elements
  const s = document.createElement('style');
  s.textContent = `
    .video-ads, .ytp-ad-module, .ytp-ad-overlay-container,
    ytd-ad-slot-renderer, [class*="ytp-ad-"], [class*="ad-showing"],
    #masthead-ad, ytd-banner-promo-renderer, ytd-in-feed-ad-layout-renderer,
    #player-ads, .ytd-merch-shelf-renderer, ytd-ad-slot-renderer,
    [id*="ad-slot"], [id*="player-ads"], ytd-promoted-video-renderer,
    .ytp-ad-text, .ytp-ad-preview-container, .ytp-ad-player-overlay-layout,
    .ytp-ad-skip-button-slot, #inline-survey { display: none !important; }
  `;
  (document.head || document.documentElement).appendChild(s);

  // Fast-skip loop: click skip button, invoke skipAd, speed up to 16x
  setInterval(function() {
    const player = document.getElementById('movie_player') || document.querySelector('.html5-video-player');
    const isAd = player && (
      player.classList.contains('ad-showing') ||
      player.classList.contains('ad-interrupting') ||
      !!document.querySelector('.ad-showing, .ad-interrupting, .ytp-ad-player-overlay, .ytp-ad-text, .ytp-ad-preview-text, .ytp-ad-skip-button-slot')
    );

    if (player && typeof player.skipAd === 'function') {
      try { player.skipAd(); } catch(e) {}
    }

    const skipBtns = document.querySelectorAll(
      '.ytp-ad-skip-button, .ytp-ad-skip-button-modern, .ytp-skip-ad-button, ' +
      'button.ytp-ad-skip-button-text, [class*="ytp-ad-skip"], button.ytp-ad-overlay-close-button, ' +
      'button[id*="skip"], [aria-label*="skip" i], [aria-label*="bỏ qua" i]'
    );
    for (const btn of skipBtns) {
      try { btn.click(); } catch(e) {}
    }

    if (isAd) {
      const v = document.querySelector('video');
      if (v) {
        v.muted = true;
        v.playbackRate = 16.0;
        if (!isNaN(v.duration) && isFinite(v.duration) && v.duration > 0) {
          v.currentTime = v.duration;
        }
      }
    }
  }, 250);
})();
"#;

fn header_html() -> String {
    include_str!("../assets/header.html")
        .replace("/*HEADER_CSS*/", include_str!("../assets/header.css"))
        .replace("/*HEADER_JS*/", include_str!("../assets/header.js"))
}
fn html_navigation_url(html: &str) -> String {
    use base64::Engine;
    format!(
        "data:text/html;charset=utf-8;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(html)
    )
}
fn send(proxy: &EventLoopProxy<EventMessage>, event: EventMessage) {
    let _ = proxy.send_event(event);
}

impl Browser {
    #[allow(dead_code)]
    fn create(window: Window, proxy: EventLoopProxy<EventMessage>) -> Result<Self, String> {
        let data_dir = std::env::var_os("PAPYRUS_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Papyrus")))
            .ok_or("Không xác định được thư mục dữ liệu ứng dụng.")?;
        Self::create_at(window, proxy, data_dir)
    }

    fn create_at(
        window: Window,
        proxy: EventLoopProxy<EventMessage>,
        data_dir: PathBuf,
    ) -> Result<Self, String> {
        native::install_dispatcher(window.hwnd(), proxy.clone()).map_err(|e| e.to_string())?;
        fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Không tạo được thư mục dữ liệu: {e}"))?;
        let state_path = data_dir.join("state.json");
        let saved = SavedState::read(&state_path)?;
        native::apply_window_theme(window.hwnd(), saved.preferences.theme == "dark");
        let preferences = Rc::new(RefCell::new(saved.preferences.clone()));
        let mut content_context = WebContext::new(Some(data_dir.join("profile")));
        let header_proxy = proxy.clone();
        let html = header_html();
        let trusted_header_url = html_navigation_url(&html);
        let header = WebViewBuilder::new_with_web_context(&mut content_context)
            .with_html(html)
            .with_bounds(Rect { position: LogicalPosition::new(0.0,0.0).into(), size: LogicalSize::new(1280.0,115.0).into() })
            .with_additional_browser_args(BROWSER_ARGS)
            .with_background_throttling(wry::BackgroundThrottlingPolicy::Throttle)
            .with_devtools(cfg!(debug_assertions))
            .with_default_context_menus(false)
            .with_navigation_handler(move |url| {
                url == trusted_header_url
            })
            .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
            .with_ipc_handler(move |request| {
                if let Some(command) = model::parse_command(&request.uri().to_string(), request.body()) { send(&header_proxy, EventMessage::Command(command)); }
            })
            .build_as_child(&window).map_err(|e| format!("Không mở được WebView2. Cài Microsoft Edge WebView2 Evergreen Runtime rồi thử lại. Chi tiết: {e}"))?;
        let _ = native::setup_header(&header, proxy.clone());
        let environment = native::environment(&header).ok();
        let mut app = Self {
            tabs: vec![],
            closed_tabs: vec![],
            active: 0,
            next_id: 1,
            header,
            content_context,
            environment,
            window,
            proxy,
            popups: Rc::new(RefCell::new(vec![])),
            preferences,
            saved,
            state_path,
            data_dir: data_dir.clone(),
            header_height: 0.0,
            blocked: 0,
            clearing: false,
            downloads: vec![],
            notice: String::new(),
            header_ready: false,
            download_paths: Rc::new(RefCell::new(vec![])),
        };
        let crash_flag = data_dir.join("running.flag");
        let was_crashed = crash_flag.exists();
        let _ = fs::write(&crash_flag, b"1");

        let is_new_window = std::env::args().any(|a| a == "--new-window");
        let args: Vec<String> = std::env::args().skip(1).collect();
        let file_or_url_args: Vec<String> = args
            .iter()
            .filter(|a| !a.starts_with("--"))
            .filter_map(|a| model::resolve_input(a, &app.saved.preferences.engine).ok())
            .collect();

        let (urls, pinned_states, active_index, crashed_notice) = if !file_or_url_args.is_empty() {
            (file_or_url_args, vec![], 0, false)
        } else if is_new_window {
            (vec![], vec![], 0, false)
        } else if was_crashed && !app.saved.last_session_urls.is_empty() {
            let mut valid = Vec::new();
            let mut pinned = Vec::new();
            for (i, url) in app.saved.last_session_urls.iter().enumerate() {
                if model::allowed_navigation(url) && valid.len() < 100 {
                    valid.push(url.clone());
                    pinned.push(
                        app.saved
                            .last_session_pinned
                            .get(i)
                            .copied()
                            .unwrap_or(false),
                    );
                }
            }
            (valid, pinned, app.saved.last_session_active, true)
        } else if app.saved.preferences.startup == "restore" {
            let mut valid = Vec::new();
            let mut pinned = Vec::new();
            for (i, url) in app.saved.restored_urls.iter().enumerate() {
                if model::allowed_navigation(url) && valid.len() < 100 {
                    valid.push(url.clone());
                    pinned.push(
                        app.saved
                            .last_session_pinned
                            .get(i)
                            .copied()
                            .unwrap_or(false),
                    );
                }
            }
            (valid, pinned, app.saved.restored_active, false)
        } else if app.saved.preferences.startup == "search" {
            (
                vec![model::search_home(&app.saved.preferences.engine).into()],
                vec![false],
                0,
                false,
            )
        } else {
            (vec![], vec![], 0, false)
        };

        if crashed_notice {
            app.notice = "Đã tự động khôi phục các tab từ phiên làm việc trước do trình duyệt bị tắt không đúng cách.".into();
        }
        let mut to_open: Vec<(String, bool)> = if urls.is_empty() {
            vec![(NEW_TAB.into(), false)]
        } else {
            urls.into_iter()
                .enumerate()
                .map(|(i, u)| {
                    let p = pinned_states.get(i).copied().unwrap_or(false);
                    (u, p)
                })
                .collect()
        };
        to_open.sort_by_key(|(_, pinned)| !*pinned);
        let target_active_idx = active_index.min(to_open.len().saturating_sub(1));
        for (i, (url, pinned)) in to_open.iter().enumerate() {
            let is_target = i == target_active_idx;
            let id = app.new_tab_opt(url, is_target, None, !is_target)?;
            if *pinned && let Some(tab) = app.tabs.iter_mut().find(|t| t.info.id == id) {
                tab.info.pinned = true;
            }
        }
        if let Some(tab) = app.tabs.get(target_active_idx) {
            app.active = tab.info.id;
        }
        app.layout()?;
        Ok(app)
    }
    fn active_tab(&self) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.info.id == self.active)
    }
    fn active_view(&self) -> Option<&WebView> {
        self.active_tab().and_then(|t| t.view.as_ref())
    }
    fn create_view(
        &mut self,
        id: u64,
        env: Option<ICoreWebView2Environment>,
    ) -> Result<WebView, String> {
        let title_proxy = self.proxy.clone();
        let navigation_proxy = self.proxy.clone();
        let start_proxy = self.proxy.clone();
        let complete_proxy = self.proxy.clone();
        let download_prefs = self.preferences.clone();
        let download_paths = self.download_paths.clone();
        let trusted_newtab_url = html_navigation_url(include_str!("../assets/newtab.html"));
        let mut builder = WebViewBuilder::new_with_web_context(&mut self.content_context)
            .with_visible(false)
            .with_autoplay(false)
            .with_additional_browser_args(BROWSER_ARGS)
            .with_background_throttling(wry::BackgroundThrottlingPolicy::Throttle)
            .with_devtools(true)
            .with_browser_extensions_enabled(true)
            .with_default_context_menus(true)
            .with_hotkeys_zoom(true)
            .with_initialization_script(ADBLOCK_INIT_SCRIPT);

        let env_to_use = env.or_else(|| self.environment.clone());
        if let Some(e) = &env_to_use {
            builder = builder.with_environment(e.clone());
        }

        let builder = builder
            .with_navigation_handler(move |url| {
                let allowed = url == trusted_newtab_url
                    || (model::allowed_navigation(&url) && !url.starts_with("papyrus:"));
                if !allowed {
                    send(
                        &navigation_proxy,
                        EventMessage::Notice(
                            "Đã từ chối điều hướng tới giao thức không được hỗ trợ.".into(),
                        ),
                    );
                }
                allowed
            })
            // Websites get no native IPC handler and cannot supply trusted URL events.
            .with_document_title_changed_handler(move |title| {
                send(&title_proxy, EventMessage::Title(id, title))
            })
            .with_download_started_handler(move |url, path| {
                let result = (|| -> Result<(), String> {
                    let preferred = download_prefs.borrow().download_dir.clone();
                    let root = if preferred.is_empty() {
                        path.parent()
                            .filter(|p| p.is_absolute())
                            .map(PathBuf::from)
                            .ok_or("WebView2 chưa cung cấp thư mục tải xuống hợp lệ.")?
                    } else {
                        PathBuf::from(preferred)
                    };
                    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
                    let name = sanitize_filename(
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("download.bin"),
                    );
                    let mut candidate = root.join(&name);
                    let mut index = 1;
                    while candidate.exists() || download_paths.borrow().contains(&candidate) {
                        candidate = root.join(format!("{index}-{name}"));
                        index += 1;
                    }
                    download_paths.borrow_mut().push(candidate.clone());
                    *path = candidate;
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        send(
                            &start_proxy,
                            EventMessage::Download(format!("Đang tải: {}", path.display())),
                        );
                        true
                    }
                    Err(error) => {
                        let host = url::Url::parse(&url)
                            .ok()
                            .and_then(|u| u.host_str().map(str::to_owned))
                            .unwrap_or_default();
                        send(
                            &start_proxy,
                            EventMessage::Notice(format!("Không thể tải tệp từ {host}: {error}")),
                        );
                        false
                    }
                }
            })
            .with_download_completed_handler(move |_, path, success| {
                let path = path
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "tệp".into());
                send(
                    &complete_proxy,
                    EventMessage::Download(format!(
                        "{}: {path}",
                        if success {
                            "Đã tải"
                        } else {
                            "Tải thất bại hoặc đã hủy"
                        }
                    )),
                );
            });
        let view = builder
            .build_as_child(&self.window)
            .map_err(|e| e.to_string())?;
        native::attach(&view, id, self.preferences.clone(), self.popups.clone(), self.proxy.clone())
            .map_err(|e| format!("Không khởi tạo được bộ bảo vệ WebView2. Hãy cập nhật WebView2 Evergreen Runtime: {e}"))?;
        view.zoom(f64::from(self.saved.preferences.zoom) / 100.0)
            .map_err(|e| e.to_string())?;
        if self.environment.is_none() {
            self.environment = Some(native::environment(&view).map_err(|e| e.to_string())?);
        }
        Ok(view)
    }
    fn new_tab(
        &mut self,
        url: &str,
        select: bool,
        env: Option<ICoreWebView2Environment>,
    ) -> Result<u64, String> {
        self.new_tab_opt(url, select, env, false)
    }

    fn new_tab_opt(
        &mut self,
        url: &str,
        select: bool,
        env: Option<ICoreWebView2Environment>,
        lazy: bool,
    ) -> Result<u64, String> {
        if self.tabs.len() >= 100 {
            return Err("Đã đạt giới hạn 100 tab. Hãy đóng bớt tab trước.".into());
        }
        let id = self.next_id;
        self.next_id += 1;
        let is_internal = [
            SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
        ]
        .contains(&url);
        let view = if is_internal || lazy {
            None
        } else {
            let view = self.create_view(id, env)?;
            if url == NEW_TAB {
                view.load_html(include_str!("../assets/newtab.html"))
            } else {
                view.load_url(url)
            }
            .map_err(|e| e.to_string())?;
            Some(view)
        };
        self.tabs.push(Tab {
            navigation: 0,
            info: TabInfo {
                id,
                title: if url == SETTINGS {
                    "Cài đặt".into()
                } else if url == HISTORY {
                    "Lịch sử".into()
                } else if url == BOOKMARKS {
                    "Dấu trang".into()
                } else if url == DOWNLOADS {
                    "Tệp tải xuống".into()
                } else if url == PASSWORDS {
                    "Mật khẩu".into()
                } else if url == EXTENSIONS {
                    "Kho tiện ích".into()
                } else if url == NEW_TAB {
                    "Tab mới".into()
                } else {
                    url.into()
                },
                url: url.into(),
                loading: if lazy {
                    false
                } else {
                    ![
                        NEW_TAB, SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
                    ]
                    .contains(&url)
                },
                blocked: 0,
                error: None,
                pinned: false,
                muted: false,
                is_playing_audio: false,
            },
            view,
        });
        if select || self.active == 0 {
            self.active = id;
        }
        Ok(id)
    }

    fn ensure_tab_view(&mut self, id: u64) -> Result<(), String> {
        let is_internal = self
            .tabs
            .iter()
            .find(|t| t.info.id == id)
            .map(|t| {
                [
                    SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
                ]
                .contains(&t.info.url.as_str())
            })
            .unwrap_or(false);
        if is_internal {
            return Ok(());
        }
        let needs_view = self.tabs.iter().any(|t| t.info.id == id && t.view.is_none());
        if needs_view {
            let view = self.create_view(id, None)?;
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                if tab.info.url == NEW_TAB {
                    view.load_html(include_str!("../assets/newtab.html"))
                        .map_err(|e| e.to_string())?;
                } else {
                    view.load_url(&tab.info.url).map_err(|e| e.to_string())?;
                    tab.info.loading = true;
                }
                tab.view = Some(view);
            }
            let _ = self.publish();
        }
        Ok(())
    }
    fn open_internal_tab(&mut self, target_url: &str) -> Result<(), String> {
        if self.active_tab().is_some_and(|t| t.info.url == target_url) {
            return self.close_tab(self.active);
        }
        if let Some(tab) = self.tabs.iter().find(|t| t.info.url == target_url) {
            self.active = tab.info.id;
        } else {
            self.new_tab(target_url, true, None)?;
        }
        self.header_height = 0.0;
        self.layout()?;
        let _ = self.header.focus();
        self.persist()?;
        self.publish()
    }
    fn record_history(&mut self, url: &str, title: &str) {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default();
        if let Some(first) = self.saved.history.first_mut()
            && first.url == url
        {
            if !title.is_empty() && first.title == first.url {
                first.title = title.chars().take(256).collect();
                let _ = self.persist();
            }
            return;
        }
        self.saved.history.insert(
            0,
            HistoryEntry {
                title: if title.is_empty() {
                    url.to_string()
                } else {
                    title.chars().take(256).collect()
                },
                url: url.to_string(),
                time: now,
            },
        );
        if self.saved.history.len() > 5000 {
            self.saved.history.truncate(5000);
        }
        let _ = self.persist();
    }
    fn navigate(&mut self, input: &str) -> Result<(), String> {
        let url = model::resolve_input(input, &self.saved.preferences.engine)?;
        let index = self
            .tabs
            .iter()
            .position(|t| t.info.id == self.active)
            .ok_or("Không tìm thấy tab.")?;
        if [
            SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
        ]
        .contains(&url.as_str())
        {
            self.tabs[index].close_view();
            self.tabs[index].info.url = url.clone();
            self.tabs[index].info.title = if url == SETTINGS {
                "Cài đặt".into()
            } else if url == HISTORY {
                "Lịch sử".into()
            } else if url == BOOKMARKS {
                "Dấu trang".into()
            } else if url == DOWNLOADS {
                "Tệp tải xuống".into()
            } else if url == PASSWORDS {
                "Mật khẩu".into()
            } else if url == EXTENSIONS {
                "Kho tiện ích".into()
            } else {
                "Trang nội bộ".into()
            };
            self.tabs[index].info.loading = false;
            self.tabs[index].info.error = None;
        } else {
            if self.tabs[index].view.is_none() {
                self.tabs[index].view = Some(self.create_view(self.active, None)?);
            }
            let view = self.tabs[index].view.as_ref().expect("created view");
            if url == NEW_TAB {
                view.load_html(include_str!("../assets/newtab.html"))
            } else {
                view.load_url(&url)
            }
            .map_err(|e| e.to_string())?;
            self.tabs[index].info.url = url;
            self.tabs[index].info.loading = true;
            self.tabs[index].info.error = None;
        }
        self.header_height = 0.0;
        self.layout()
    }
    fn record_closed_tab(&mut self, url: &str, title: &str) {
        if !url.starts_with("papyrus:") && url != "about:blank" && !url.is_empty() {
            self.closed_tabs.push((url.to_string(), title.to_string()));
            if self.closed_tabs.len() > 50 {
                self.closed_tabs.remove(0);
            }
        }
    }
    fn close_tab(&mut self, mut id: u64) -> Result<(), String> {
        if id == 0 {
            id = self.active;
        }
        if !self.tabs.iter().any(|t| t.info.id == id) {
            return Ok(());
        }
        if let Some((url, title)) = self
            .tabs
            .iter()
            .find(|t| t.info.id == id)
            .map(|t| (t.info.url.clone(), t.info.title.clone()))
        {
            self.record_closed_tab(&url, &title);
        }
        let ids = self.tabs.iter().map(|t| t.info.id).collect::<Vec<_>>();
        let next = model::next_active_after_close(&ids, self.active, id);
        self.tabs.retain(|t| t.info.id != id);
        if let Some(next) = next {
            self.active = next;
        } else {
            self.active = 0;
            self.new_tab(NEW_TAB, true, None)?;
        }
        self.header_height = 0.0;
        self.layout()
    }
    fn layout(&mut self) -> Result<(), String> {
        self.ensure_tab_view(self.active)?;
        let size = self
            .window
            .inner_size()
            .to_logical::<f64>(self.window.scale_factor());
        let is_internal = self.active_tab().is_some_and(|t| {
            [
                SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
            ]
            .contains(&t.info.url.as_str())
        });
        let base = if self.saved.preferences.show_bookmarks {
            116.0
        } else {
            87.0
        };
        let height = if is_internal || self.clearing {
            size.height
        } else {
            self.header_height.max(base).min(size.height)
        };
        self.header
            .set_bounds(Rect {
                position: LogicalPosition::new(0.0, 0.0).into(),
                size: LogicalSize::new(size.width, height.max(1.0)).into(),
            })
            .map_err(|e| e.to_string())?;
        for tab in &self.tabs {
            if let Some(view) = &tab.view {
                view.set_bounds(Rect {
                    position: LogicalPosition::new(0.0, base).into(),
                    size: LogicalSize::new(size.width, (size.height - base).max(1.0)).into(),
                })
                .map_err(|e| e.to_string())?;
                let is_active = tab.info.id == self.active && !is_internal && !self.clearing;
                view.set_visible(is_active)
                    .map_err(|e| e.to_string())?;
                if is_active {
                    let _ = native::resume_view(view);
                } else if !tab.info.is_playing_audio {
                    let _ = native::suspend_view(view);
                }
            }
        }
        Ok(())
    }
    fn persist(&mut self) -> Result<(), String> {
        self.saved.last_session_urls = self.tabs.iter().map(|t| t.info.url.clone()).collect();
        self.saved.last_session_pinned = self.tabs.iter().map(|t| t.info.pinned).collect();
        self.saved.last_session_active = self
            .tabs
            .iter()
            .position(|t| t.info.id == self.active)
            .unwrap_or_default();
        if self.saved.preferences.startup == "restore" {
            self.saved.restored_urls = self.saved.last_session_urls.clone();
            self.saved.restored_active = self.saved.last_session_active;
        } else {
            self.saved.restored_urls.clear();
            self.saved.restored_active = 0;
        }

        let is_maximized = self.window.is_maximized();
        self.saved.window_maximized = is_maximized;
        if !is_maximized {
            if let Ok(pos) = self.window.outer_position()
                && pos.x > -30000
                && pos.y > -30000
            {
                self.saved.window_x = Some(pos.x);
                self.saved.window_y = Some(pos.y);
            }
            let size = self.window.inner_size();
            if size.width >= 680 && size.height >= 480 {
                self.saved.window_width = Some(size.width);
                self.saved.window_height = Some(size.height);
            }
        }
        self.saved.write(&self.state_path)
    }
    fn publish(&self) -> Result<(), String> {
        if !self.header_ready {
            return Ok(());
        }
        let (can_back, can_forward) = self
            .active_view()
            .map(native::history)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let snapshot = Snapshot {
            tabs: self.tabs.iter().map(|t| &t.info).collect(),
            active: self.active,
            preferences: &self.saved.preferences,
            bookmarks: &self.saved.bookmarks,
            history: &self.saved.history,
            passwords: &self.saved.passwords,
            blocked: self.blocked,
            clearing: self.clearing,
            can_back,
            can_forward,
            downloads: &self.downloads,
            notice: &self.notice,
            data_dir: self.data_dir.display().to_string(),
            version: env!("CARGO_PKG_VERSION"),
            builtin_count: model::BUILTIN_BLOCKS.len(),
        };
        let json = serde_json::to_string(&snapshot).map_err(|e| e.to_string())?;
        self.header
            .evaluate_script(&format!("window.applyState({json});"))
            .map_err(|e| e.to_string())
    }
    fn notify(&mut self, message: String) {
        self.notice = message.clone();
        eprintln!("Papyrus: {message}");
        if self.header_ready {
            match serde_json::to_string(&message) {
                Ok(json) => {
                    if let Err(e) = self
                        .header
                        .evaluate_script(&format!("window.showNotice({json});"))
                    {
                        eprintln!("Papyrus: Không hiển thị được thông báo: {e}");
                    }
                }
                Err(e) => eprintln!("Papyrus: {e}"),
            }
        }
    }
    fn preferences(&mut self, preferences: Preferences) -> Result<(), String> {
        preferences.validate()?;
        if !preferences.download_dir.is_empty() {
            let dir = PathBuf::from(&preferences.download_dir);
            fs::create_dir_all(&dir).map_err(|e| format!("Không tạo được thư mục tải: {e}"))?;
            if !dir.is_dir() {
                return Err("Vị trí tải xuống không phải thư mục.".into());
            }
        }
        let previous = self.saved.preferences.clone();
        self.saved.preferences = preferences.clone();
        if let Err(e) = self.persist() {
            self.saved.preferences = previous;
            return Err(e);
        }
        native::apply_window_theme(self.window.hwnd(), preferences.theme == "dark");
        *self.preferences.borrow_mut() = preferences.clone();
        for tab in &self.tabs {
            if let Some(view) = &tab.view {
                view.zoom(f64::from(preferences.zoom) / 100.0)
                    .map_err(|e| e.to_string())?;
            }
        }
        self.layout()?;
        self.notify("Đã lưu cài đặt. Tải lại website nếu vừa đổi bộ lọc.".into());
        Ok(())
    }
    fn start_clear(&mut self) -> Result<(), String> {
        self.clearing = true;
        self.saved.restored_urls.clear();
        self.saved.restored_active = 0;
        self.saved.write(&self.state_path)?;
        for tab in &self.tabs {
            if let Some(view) = &tab.view {
                native::stop(view).map_err(|e| e.to_string())?;
            }
        }
        self.tabs.clear();
        self.active = 0;
        let id = self.next_id;
        self.next_id += 1;
        let view = self.create_view(id, None)?;
        self.tabs.push(Tab {
            navigation: 0,
            info: TabInfo {
                id,
                title: "Đang xóa dữ liệu".into(),
                url: NEW_TAB.into(),
                loading: false,
                blocked: 0,
                error: None,
                pinned: false,
                muted: false,
                is_playing_audio: false,
            },
            view: Some(view),
        });
        self.active = id;
        self.layout()?;
        native::clear_data(
            self.active_view().expect("clear keeper"),
            self.proxy.clone(),
        )
        .map_err(|e| e.to_string())
    }
    fn command(&mut self, command: Command) -> Result<bool, String> {
        if self.clearing && !matches!(command, Command::Ready) {
            return Err("Đang xóa dữ liệu. Vui lòng chờ kết quả trước khi tiếp tục.".into());
        }
        match command {
            Command::Ready => self.header_ready = true,
            Command::FocusAddress => {
                self.header.focus().map_err(|e| e.to_string())?;
                self.header.evaluate_script("document.getElementById('url').focus();document.getElementById('url').select();").map_err(|e| e.to_string())?;
            }
            Command::Navigate { input } => {
                self.navigate(&input)?;
                self.persist()?;
            }
            Command::NewTab => {
                self.new_tab(NEW_TAB, true, None)?;
                self.header_height = 0.0;
                self.layout()?;
                self.persist()?;
                let _ = self.header.evaluate_script("document.getElementById('url').focus();document.getElementById('url').select();");
            }
            Command::NewWindow => {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe).arg("--new-window").spawn();
                }
            }
            Command::SwitchTab { id } => {
                if self.tabs.iter().any(|t| t.info.id == id) {
                    self.active = id;
                    self.header_height = 0.0;
                    self.layout()?;
                    self.persist()?;
                }
            }
            Command::CloseTab { id } => {
                self.close_tab(id)?;
                self.persist()?;
            }
            Command::Back | Command::Forward => {
                if let Some(view) = self.active_view() {
                    native::navigate_history(view, matches!(command, Command::Forward))
                        .map_err(|e| e.to_string())?;
                }
            }
            Command::Reload => {
                let active_id = self.active;
                let mut navigate_fallback = None;
                let mut recreate_info = None;

                if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == active_id) {
                    tab.info.error = None;
                    tab.info.loading = true;
                    if let Some(view) = &tab.view {
                        if let Err(e) = native::reload(view) {
                            eprintln!(
                                "[Papyrus] Reload failed ({e}), recreating view for tab {active_id}"
                            );
                            tab.close_view();
                            recreate_info = Some((tab.info.id, tab.info.url.clone()));
                        }
                    } else {
                        navigate_fallback = Some(tab.info.url.clone());
                    }
                }

                if let Some(url) = navigate_fallback {
                    self.navigate(&url)?;
                } else if let Some((id, url)) = recreate_info {
                    match self.create_view(id, None) {
                        Ok(new_view) => {
                            let _ = new_view.load_url(&url);
                            if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                                tab.view = Some(new_view);
                            }
                        }
                        Err(err) => {
                            if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                                tab.info.error =
                                    Some(format!("Không thể khởi động lại trang: {err}"));
                                tab.info.loading = false;
                            }
                        }
                    }
                }
                self.layout()?;
                self.publish()?;
            }
            Command::Home => {
                self.navigate(NEW_TAB)?;
                self.persist()?;
            }
            Command::Close => {
                let _ = fs::remove_file(self.data_dir.join("running.flag"));
                self.persist()?;
                return Ok(true);
            }
            Command::Minimize => self.window.set_minimized(true),
            Command::Maximize => self.window.set_maximized(!self.window.is_maximized()),
            Command::Snap { layout } => {
                native::snap_window(self.window.hwnd(), &layout);
            }
            Command::Drag => self.window.drag_window().map_err(|e| e.to_string())?,
            Command::Settings => self.open_internal_tab(SETTINGS)?,
            Command::History => self.open_internal_tab(HISTORY)?,
            Command::Bookmarks => self.open_internal_tab(BOOKMARKS)?,
            Command::Downloads => self.open_internal_tab(DOWNLOADS)?,
            Command::Passwords => self.open_internal_tab(PASSWORDS)?,
            Command::Extensions => self.open_internal_tab(EXTENSIONS)?,
            Command::SavePassword {
                url,
                username,
                password,
            } => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                if let Some(entry) = self
                    .saved
                    .passwords
                    .iter_mut()
                    .find(|p| p.url == url && p.username == username)
                {
                    entry.password = password;
                    entry.updated = now;
                } else {
                    let next_id = self.saved.passwords.iter().map(|p| p.id).max().unwrap_or(0) + 1;
                    self.saved.passwords.push(PasswordEntry {
                        id: next_id,
                        url,
                        username,
                        password,
                        updated: now,
                    });
                }
                self.persist()?;
                self.notify("Đã lưu thông tin đăng nhập vào kho mật khẩu an toàn.".into());
            }
            Command::DeletePassword { id } => {
                self.saved.passwords.retain(|p| p.id != id);
                self.persist()?;
                self.notify("Đã xóa tài khoản khỏi kho mật khẩu.".into());
            }
            Command::ClearPasswords => {
                self.saved.passwords.clear();
                self.persist()?;
                self.notify("Đã xóa toàn bộ mật khẩu đã lưu.".into());
            }
            Command::OpenDownloadedFile { path } => {
                let p = std::path::Path::new(&path);
                if !p.exists() {
                    self.notify(format!("Tệp không tồn tại: {path}"));
                } else {
                    unsafe {
                        let path_wide: Vec<u16> =
                            path.encode_utf16().chain(std::iter::once(0)).collect();
                        let res = ShellExecuteW(
                            None,
                            w!("open"),
                            PCWSTR(path_wide.as_ptr()),
                            PCWSTR::null(),
                            PCWSTR::null(),
                            SW_SHOW,
                        );
                        if res.0 as usize <= 32 {
                            self.notify(format!(
                                "Không thể mở tệp: {path} (Mã lỗi: {})",
                                res.0 as usize
                            ));
                        }
                    }
                }
            }
            Command::ShowDownloadInFolder { path } => {
                let p = std::path::Path::new(&path);
                if !p.exists() {
                    if let Some(parent) = p.parent().filter(|parent| parent.exists()) {
                        let parent_wide: Vec<u16> = parent
                            .display()
                            .to_string()
                            .encode_utf16()
                            .chain(std::iter::once(0))
                            .collect();
                        unsafe {
                            let _ = ShellExecuteW(
                                None,
                                w!("open"),
                                w!("explorer.exe"),
                                PCWSTR(parent_wide.as_ptr()),
                                PCWSTR::null(),
                                SW_SHOW,
                            );
                        }
                        self.notify(format!(
                            "Tệp đã bị xóa; đã mở thư mục: {}",
                            parent.display()
                        ));
                    } else {
                        self.notify(format!("Đường dẫn không tồn tại: {path}"));
                    }
                } else {
                    unsafe {
                        let args = format!("/select,\"{}\"", path);
                        let args_wide: Vec<u16> =
                            args.encode_utf16().chain(std::iter::once(0)).collect();
                        let res = ShellExecuteW(
                            None,
                            w!("open"),
                            w!("explorer.exe"),
                            PCWSTR(args_wide.as_ptr()),
                            PCWSTR::null(),
                            SW_SHOW,
                        );
                        if res.0 as usize <= 32 {
                            self.notify(format!("Không thể mở thư mục chứa tệp: {path}"));
                        }
                    }
                }
            }
            Command::ClearDownloads => {
                self.downloads.clear();
                self.publish()?;
                self.notify("Đã xóa danh sách tệp tải xuống.".into());
            }
            Command::ReopenTab | Command::RestoreTab => {
                if let Some((url, _)) = self.closed_tabs.pop() {
                    self.new_tab(&url, true, None)?;
                    self.header_height = 0.0;
                    self.layout()?;
                    self.persist()?;
                    self.publish()?;
                }
            }
            Command::PinTab { id } => {
                let target_id = if id == 0 { self.active } else { id };
                if let Some(pos) = self.tabs.iter().position(|t| t.info.id == target_id) {
                    let is_now_pinned = !self.tabs[pos].info.pinned;
                    self.tabs[pos].info.pinned = is_now_pinned;

                    let other_pinned_count = self
                        .tabs
                        .iter()
                        .enumerate()
                        .filter(|&(i, t)| i != pos && t.info.pinned)
                        .count();

                    let tab = self.tabs.remove(pos);
                    self.tabs.insert(other_pinned_count, tab);

                    self.persist()?;
                    self.publish()?;
                }
            }
            Command::ToggleMuteTab { id } => {
                let target_id = if id == 0 { self.active } else { id };
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == target_id) {
                    let new_muted = !tab.info.muted;
                    if let Some(view) = &tab.view {
                        let _ = native::set_muted(view, new_muted);
                    }
                    tab.info.muted = new_muted;
                    self.publish()?;
                }
            }
            Command::DuplicateTab { id } => {
                let target_id = if id == 0 { self.active } else { id };
                if let Some(source_idx) = self.tabs.iter().position(|t| t.info.id == target_id) {
                    let url = self.tabs[source_idx].info.url.clone();
                    let is_pinned = self.tabs[source_idx].info.pinned;
                    let new_id = self.new_tab(&url, true, None)?;
                    if let Some(created_idx) = self.tabs.iter().position(|t| t.info.id == new_id) {
                        let mut tab = self.tabs.remove(created_idx);
                        if is_pinned {
                            tab.info.pinned = true;
                        }
                        let insert_idx = (source_idx + 1).min(self.tabs.len());
                        self.tabs.insert(insert_idx, tab);
                    }
                    self.active = new_id;
                    self.header_height = 0.0;
                    self.layout()?;
                    self.persist()?;
                    self.publish()?;
                }
            }
            Command::ZoomIn => {
                let mut p = self.saved.preferences.clone();
                p.zoom = (p.zoom + 10).min(200);
                self.preferences(p)?;
            }
            Command::ZoomOut => {
                let mut p = self.saved.preferences.clone();
                p.zoom = p.zoom.saturating_sub(10).max(50);
                self.preferences(p)?;
            }
            Command::ZoomReset => {
                let mut p = self.saved.preferences.clone();
                p.zoom = 100;
                self.preferences(p)?;
            }
            Command::Print => {
                if let Some(view) = self.active_view() {
                    let _ = view.evaluate_script("window.print();");
                }
            }
            Command::NextTab => {
                if !self.tabs.is_empty()
                    && let Some(pos) = self.tabs.iter().position(|t| t.info.id == self.active)
                {
                    let next = (pos + 1) % self.tabs.len();
                    self.active = self.tabs[next].info.id;
                    self.header_height = 0.0;
                    self.layout()?;
                    self.persist()?;
                }
            }
            Command::PrevTab => {
                if !self.tabs.is_empty()
                    && let Some(pos) = self.tabs.iter().position(|t| t.info.id == self.active)
                {
                    let prev = if pos == 0 {
                        self.tabs.len() - 1
                    } else {
                        pos - 1
                    };
                    self.active = self.tabs[prev].info.id;
                    self.header_height = 0.0;
                    self.layout()?;
                    self.persist()?;
                }
            }
            Command::SelectTabByIndex { index } => {
                if let Some(tab) = self.tabs.get(index) {
                    self.active = tab.info.id;
                    self.header_height = 0.0;
                    self.layout()?;
                    self.persist()?;
                }
            }
            Command::SelectLastTab => {
                if let Some(tab) = self.tabs.last() {
                    self.active = tab.info.id;
                    self.header_height = 0.0;
                    self.layout()?;
                    self.persist()?;
                }
            }
            Command::ClearHistory => {
                self.saved.history.clear();
                self.persist()?;
                self.notify("Đã xóa toàn bộ lịch sử duyệt web.".into());
            }
            Command::DeleteHistoryEntry { index } => {
                if index < self.saved.history.len() {
                    self.saved.history.remove(index);
                    self.persist()?;
                }
            }
            Command::SavePreferences { preferences } => self.preferences(preferences)?,
            Command::ResetPreferences => self.preferences(Preferences::default())?,
            Command::Bookmark => {
                if let Some(tab) = self.active_tab() {
                    let bookmark = Bookmark {
                        title: tab.info.title.chars().take(256).collect(),
                        url: tab.info.url.clone(),
                    };
                    if !self.saved.bookmarks.iter().any(|b| b.url == bookmark.url) {
                        if self.saved.bookmarks.len() >= 10_000 {
                            return Err("Đã đạt giới hạn 10.000 ghim.".into());
                        }
                        self.saved.bookmarks.push(bookmark);
                        self.persist()?;
                    }
                    self.notify("Đã ghim trang.".into());
                }
            }
            Command::DeleteBookmark { index } => {
                if index < self.saved.bookmarks.len() {
                    self.saved.bookmarks.remove(index);
                    self.persist()?;
                }
            }
            Command::AddBookmark { title, url } => {
                let clean_url =
                    model::resolve_input(&url, &self.saved.preferences.engine).unwrap_or(url);
                let bookmark = Bookmark {
                    title: if title.trim().is_empty() {
                        clean_url.clone()
                    } else {
                        title.chars().take(256).collect()
                    },
                    url: clean_url,
                };
                if !self.saved.bookmarks.iter().any(|b| b.url == bookmark.url) {
                    if self.saved.bookmarks.len() >= 10_000 {
                        return Err("Đã đạt giới hạn 10.000 ghim.".into());
                    }
                    self.saved.bookmarks.push(bookmark);
                    self.persist()?;
                    self.notify("Đã thêm dấu trang mới.".into());
                } else {
                    self.notify("Trang này đã có trong danh sách dấu trang.".into());
                }
            }
            Command::ClearBookmarks => {
                self.saved.bookmarks.clear();
                self.persist()?;
                self.notify("Đã xóa toàn bộ dấu trang.".into());
            }
            Command::ClearData => {
                if let Err(e) = self.start_clear() {
                    self.clearing = false;
                    self.layout()?;
                    return Err(format!("Chưa xóa được dữ liệu: {e}"));
                }
            }
            Command::CancelClear => {}
            Command::DevTools => {
                if let Some(view) = self.active_view() {
                    let _ = native::open_devtools(view);
                }
            }
            Command::HeaderHeight { height } => {
                self.header_height = f64::from(height.min(800));
                self.layout()?;
            }
            Command::ShowMenu { x, y } => {
                if let Some(cmd) = native::show_main_menu(self.window.hwnd(), x, y) {
                    return self.command(cmd);
                }
            }
            Command::ShowSnapMenu { x, y } => {
                if let Some(layout) = native::show_snap_menu(self.window.hwnd(), x, y) {
                    native::snap_window(self.window.hwnd(), &layout);
                }
            }
            Command::ShowTabMenu { id, x, y } => {
                let tab_pos = self.tabs.iter().position(|t| t.info.id == id);
                let is_pinned = tab_pos.is_some_and(|pos| self.tabs[pos].info.pinned);
                let is_muted = tab_pos.is_some_and(|pos| self.tabs[pos].info.muted);
                let can_close_others = self.tabs.iter().any(|t| t.info.id != id && !t.info.pinned);
                let has_tabs_to_right = tab_pos
                    .is_some_and(|pos| self.tabs.iter().skip(pos + 1).any(|t| !t.info.pinned));
                let has_closed_tabs = !self.closed_tabs.is_empty();
                if let Some(action) = native::show_tab_menu(
                    self.window.hwnd(),
                    x,
                    y,
                    id == 0,
                    can_close_others,
                    has_tabs_to_right,
                    has_closed_tabs,
                    is_pinned,
                    is_muted,
                ) {
                    match action {
                        1 => {
                            let mut recreate_info = None;
                            let mut navigate_fallback = None;

                            if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                                tab.info.error = None;
                                tab.info.loading = true;
                                if let Some(view) = &tab.view {
                                    if let Err(e) = native::reload(view) {
                                        eprintln!(
                                            "[Papyrus] Context reload failed ({e}), recreating view for tab {id}"
                                        );
                                        tab.close_view();
                                        recreate_info = Some((tab.info.id, tab.info.url.clone()));
                                    }
                                } else {
                                    navigate_fallback = Some(tab.info.url.clone());
                                }
                            }

                            if let Some(url) = navigate_fallback {
                                let _ = self.navigate(&url);
                            } else if let Some((tab_id, url)) = recreate_info
                                && let Ok(new_view) = self.create_view(tab_id, None)
                            {
                                let _ = new_view.load_url(&url);
                                if let Some(tab) =
                                    self.tabs.iter_mut().find(|t| t.info.id == tab_id)
                                {
                                    tab.view = Some(new_view);
                                }
                            }
                            let _ = self.layout();
                            let _ = self.publish();
                        }
                        2 => return self.command(Command::DuplicateTab { id }),
                        3 => {
                            self.close_tab(id)?;
                            self.persist()?;
                        }
                        4 => {
                            let to_record: Vec<(String, String)> = self
                                .tabs
                                .iter()
                                .filter(|t| t.info.id != id && !t.info.pinned)
                                .map(|t| (t.info.url.clone(), t.info.title.clone()))
                                .collect();
                            for (url, title) in to_record {
                                self.record_closed_tab(&url, &title);
                            }
                            self.active = id;
                            self.tabs.retain(|t| t.info.id == id || t.info.pinned);
                            self.header_height = 0.0;
                            self.layout()?;
                            self.persist()?;
                            self.publish()?;
                        }
                        5 => {
                            if let Some(pos) = tab_pos {
                                let to_record: Vec<(String, String)> = self
                                    .tabs
                                    .iter()
                                    .skip(pos + 1)
                                    .filter(|t| !t.info.pinned)
                                    .map(|t| (t.info.url.clone(), t.info.title.clone()))
                                    .collect();
                                for (url, title) in to_record {
                                    self.record_closed_tab(&url, &title);
                                }
                                let to_close: Vec<u64> = self
                                    .tabs
                                    .iter()
                                    .skip(pos + 1)
                                    .filter(|t| !t.info.pinned)
                                    .map(|t| t.info.id)
                                    .collect();
                                if to_close.contains(&self.active) {
                                    self.active = id;
                                }
                                self.tabs.retain(|t| !to_close.contains(&t.info.id));
                                self.header_height = 0.0;
                                self.layout()?;
                                self.persist()?;
                                self.publish()?;
                            }
                        }
                        6 => return self.command(Command::Bookmark),
                        7 => return self.command(Command::PinTab { id }),
                        8 => return self.command(Command::ToggleMuteTab { id }),
                        10 => return self.command(Command::NewTab),
                        11 => return self.command(Command::ReopenTab),
                        _ => {}
                    }
                }
            }
            Command::SetDefaultBrowser => {
                if let Err(e) = native::register_as_default_browser() {
                    self.notify(format!("Lỗi khi thiết lập: {e}"));
                } else {
                    self.notify(
                        "Đã mở Cài đặt Windows để đặt Papyrus làm trình duyệt mặc định.".into(),
                    );
                }
            }
            Command::ToggleFullscreen => {
                let is_fs = self.window.fullscreen().is_some();
                self.window.set_fullscreen(if is_fs {
                    None
                } else {
                    Some(tao::window::Fullscreen::Borderless(None))
                });
                self.layout()?;
            }
            Command::ReorderTab { from, to } => {
                if from != to {
                    let from_pos = self.tabs.iter().position(|t| t.info.id == from);
                    let to_pos = self.tabs.iter().position(|t| t.info.id == to);
                    if let (Some(f), Some(t)) = (from_pos, to_pos)
                        && self.tabs[f].info.pinned == self.tabs[t].info.pinned
                    {
                        let tab = self.tabs.remove(f);
                        self.tabs.insert(t, tab);
                        self.persist()?;
                        self.publish()?;
                    }
                }
            }
            Command::CheckUpdate => {
                check_github_update(self.proxy.clone(), true);
            }
        }
        Ok(false)
    }
    fn event(&mut self, event: EventMessage) -> Result<bool, String> {
        match event {
            EventMessage::Command(command) => return self.command(command),
            EventMessage::Source(id, source) => {
                if self.clearing {
                    return Ok(false);
                }
                let mut title = String::new();
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                    if source != "about:blank" || tab.info.url != NEW_TAB {
                        tab.info.url = source.clone();
                    }
                    title = tab.info.title.clone();
                    self.persist()?;
                }
                self.record_history(&source, &title);
            }
            EventMessage::Title(id, title) => {
                let mut url = String::new();
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                    tab.info.title = title.chars().take(1024).collect();
                    url = tab.info.url.clone();
                }
                self.record_history(&url, &title);
            }
            EventMessage::Loading(id, navigation) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                    tab.navigation = navigation;
                    tab.info.loading = true;
                    tab.info.error = None;
                }
            }
            EventMessage::Finished(id, navigation, error) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id)
                    && tab.navigation == navigation
                {
                    tab.info.loading = false;
                    tab.info.error = error;
                }
            }
            EventMessage::Blocked(id) => {
                self.blocked += 1;
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                    tab.info.blocked += 1;
                }
                let _ = self.publish();
            }
            EventMessage::AudioPlaying(id, playing) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                    tab.info.is_playing_audio = playing;
                    if tab.info.id != self.active && let Some(view) = &tab.view {
                        if playing {
                            let _ = native::resume_view(view);
                        } else {
                            let _ = native::suspend_view(view);
                        }
                    }
                }
            }
            EventMessage::Muted(id, muted) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                    tab.info.muted = muted;
                }
            }
            EventMessage::History => {}
            EventMessage::Popup => {
                let pending = self.popups.borrow_mut().drain(..).collect::<Vec<_>>();
                for popup in pending {
                    let result = if self.clearing {
                        Err("Không mở popup khi đang xóa dữ liệu.".into())
                    } else {
                        self.new_tab("about:blank", true, Some(popup.environment))
                            .and_then(|id| {
                                let view = self
                                    .tabs
                                    .iter()
                                    .find(|t| t.info.id == id)
                                    .and_then(|t| t.view.as_ref())
                                    .expect("popup view");
                                // SAFETY: Same environment and UI thread; request is held by deferral.
                                unsafe {
                                    popup.args.SetNewWindow(
                                        &native::core(view).map_err(|e| e.to_string())?,
                                    )
                                }
                                .map_err(|e| e.to_string())
                            })
                    };
                    // Release the request even when building the child failed.
                    if let Err(e) = unsafe { popup.deferral.Complete() } {
                        self.notify(e.to_string());
                    }
                    if let Err(e) = result {
                        self.notify(e);
                    }
                }
                self.header_height = 0.0;
                self.layout()?;
                self.persist()?;
            }
            EventMessage::CloseTab(id) => {
                if !self.clearing {
                    self.close_tab(id)?;
                    self.persist()?;
                }
            }
            EventMessage::ClearCompleted(result) => {
                self.clearing = false;
                match result {
                    Ok(()) => {
                        self.tabs.clear();
                        self.active = 0;
                        self.blocked = 0;
                        self.new_tab(NEW_TAB, true, None)?;
                        self.notify(
                            "Đã xóa dữ liệu duyệt web. Ghim và cài đặt được giữ nguyên.".into(),
                        );
                    }
                    Err(e) => self.notify(format!(
                        "Xóa dữ liệu thất bại; chưa thể xác nhận dữ liệu đã được xóa: {e}"
                    )),
                }
                self.header_height = 0.0;
                self.layout()?;
                self.persist()?;
            }
            EventMessage::Notice(message) => self.notify(message),
            EventMessage::Download(message) => {
                self.downloads.push(message.clone());
                if self.downloads.len() > 100 {
                    self.downloads.remove(0);
                }
                self.notify(message);
                let _ = self.publish();
            }
            EventMessage::ProcessFailed(id, _kind, err) => {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                    tab.info.loading = false;
                    tab.info.error = Some(err);
                }
                let _ = self.publish();
            }
            EventMessage::UpdateAvailable {
                version,
                url,
                notes,
                download_url,
            } => {
                let info = serde_json::json!({
                    "version": version,
                    "url": url,
                    "notes": notes,
                    "download_url": download_url,
                });
                if let Ok(json) = serde_json::to_string(&info) {
                    let _ = self.header.evaluate_script(&format!("window.showUpdateToast({json});"));
                }
            }
            EventMessage::Exit => return Ok(true),
        }
        Ok(false)
    }
}

fn check_github_update(proxy: tao::event_loop::EventLoopProxy<EventMessage>, manual: bool) {
    std::thread::spawn(move || {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let output = match Command::new("curl.exe")
            .creation_flags(CREATE_NO_WINDOW)
            .args([
                "-s",
                "-m",
                "8",
                "-H",
                "User-Agent: Papyrus-Browser",
                "-H",
                "Accept: application/vnd.github.v3+json",
                "https://api.github.com/repos/anhtu1707/papyrus/releases/latest",
            ])
            .output()
        {
            Ok(out) => out,
            Err(e) => {
                if manual {
                    send(&proxy, EventMessage::Notice(format!("Lỗi kiểm tra cập nhật: {e}")));
                }
                return;
            }
        };

        if !output.status.success() {
            if manual {
                send(&proxy, EventMessage::Notice("Không kết nối được tới máy chủ cập nhật GitHub.".into()));
            }
            return;
        }

        let body = match String::from_utf8(output.stdout) {
            Ok(b) => b,
            Err(_) => return,
        };

        let v: serde_json::Value = match serde_json::from_str(&body) {
            Ok(val) => val,
            Err(_) => {
                if manual {
                    send(&proxy, EventMessage::Notice("Không phân tích được phản hồi từ máy chủ.".into()));
                }
                return;
            }
        };

        if let Some(tag) = v.get("tag_name").and_then(|t| t.as_str()) {
            let remote_version = tag.trim_start_matches(|c: char| !c.is_ascii_digit()).trim();
            let current_version = env!("CARGO_PKG_VERSION");
            if model::is_newer_version(remote_version, current_version) {
                let url = v
                    .get("html_url")
                    .and_then(|u| u.as_str())
                    .unwrap_or("https://github.com/anhtu1707/papyrus/releases")
                    .to_string();
                let notes = v.get("body").and_then(|b| b.as_str()).unwrap_or("").to_string();

                let mut download_url = url.clone();
                if let Some(assets) = v.get("assets").and_then(|a| a.as_array()) {
                    for asset in assets {
                        let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        if name.ends_with(".msi") || name.ends_with(".exe") {
                            if let Some(dl) = asset.get("browser_download_url").and_then(|d| d.as_str()) {
                                download_url = dl.to_string();
                                if name.ends_with(".msi") {
                                    break;
                                }
                            }
                        }
                    }
                }

                send(
                    &proxy,
                    EventMessage::UpdateAvailable {
                        version: remote_version.to_string(),
                        url,
                        notes,
                        download_url,
                    },
                );
            } else if manual {
                send(
                    &proxy,
                    EventMessage::Notice(format!("Bạn đang sử dụng phiên bản mới nhất (v{current_version}).")),
                );
            }
        } else if manual {
            send(&proxy, EventMessage::Notice("Hiện chưa có bản phát hành nào mới hơn.".into()));
        }
    });
}

fn sanitize_filename(name: &str) -> String {
    let name = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    let name = name.trim_matches([' ', '.']);
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if name.is_empty() {
        "download.bin".into()
    } else if [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ]
    .contains(&stem.as_str())
    {
        format!("_{name}")
    } else {
        name.chars().take(180).collect()
    }
}
fn startup() -> Result<(), String> {
    let mut event_loop = EventLoopBuilder::<EventMessage>::with_user_event().build();
    let data_dir = std::env::var_os("PAPYRUS_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Papyrus")))
        .ok_or("Không xác định được thư mục dữ liệu ứng dụng.")?;
    let state_path = data_dir.join("state.json");
    let saved = SavedState::read(&state_path).unwrap_or_default();

    let mut builder = WindowBuilder::new()
        .with_title(concat!("Papyrus ", env!("CARGO_PKG_VERSION")))
        .with_decorations(false)
        .with_min_inner_size(LogicalSize::new(680.0, 480.0));

    if let (Some(w), Some(h)) = (saved.window_width, saved.window_height) {
        if w >= 680 && h >= 480 {
            builder = builder.with_inner_size(tao::dpi::PhysicalSize::new(w, h));
        } else {
            builder = builder.with_inner_size(LogicalSize::new(1280.0, 840.0));
        }
    } else {
        builder = builder.with_inner_size(LogicalSize::new(1280.0, 840.0));
    }

    if let (Some(x), Some(y)) = (saved.window_x, saved.window_y) {
        let w = saved.window_width.unwrap_or(1280);
        let h = saved.window_height.unwrap_or(840);
        if native::is_rect_visible_on_any_monitor(x, y, w, h) {
            builder = builder.with_position(tao::dpi::PhysicalPosition::new(x, y));
        }
    }

    if saved.window_maximized {
        builder = builder.with_maximized(true);
    }

    let window = builder.build(&event_loop).map_err(|e| e.to_string())?;
    let app = Rc::new(RefCell::new(Browser::create_at(
        window,
        event_loop.create_proxy(),
        data_dir,
    )?));
    let hwnd = app.borrow().window.hwnd();
    unsafe {
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            ICON_BIG, ICON_SMALL, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED, LoadImageW, SendMessageW,
            WM_SETICON,
        };
        if let Ok(h_instance) = GetModuleHandleW(None)
            && let Ok(icon) = LoadImageW(
                Some(h_instance.into()),
                windows::core::PCWSTR(std::ptr::without_provenance(1)),
                IMAGE_ICON,
                0,
                0,
                LR_DEFAULTSIZE | LR_SHARED,
            )
        {
            let _ = SendMessageW(
                HWND(hwnd as *mut _),
                WM_SETICON,
                Some(WPARAM(ICON_BIG as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
            let _ = SendMessageW(
                HWND(hwnd as *mut _),
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(icon.0 as isize)),
            );
        }
    }
    let proxy = event_loop.create_proxy();
    let update_proxy = proxy.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(3));
        check_github_update(update_proxy, false);
    });
    event_loop.run_return(|event, _, flow| {
        *flow = ControlFlow::Wait;
        let event = match event {
            Event::UserEvent(EventMessage::Exit) => {
                *flow = ControlFlow::Exit;
                return;
            }
            Event::UserEvent(EventMessage::Command(Command::Drag)) => {
                let _ = app.borrow().window.drag_window();
                return;
            }
            Event::UserEvent(EventMessage::Command(Command::Maximize)) => {
                let win = &app.borrow().window;
                win.set_maximized(!win.is_maximized());
                return;
            }
            Event::UserEvent(event) => Some(event),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => Some(EventMessage::Command(Command::Close)),
            Event::WindowEvent {
                event: WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. },
                ..
            } => Some(EventMessage::History),
            _ => None,
        };
        if let Some(event) = event {
            let app = app.clone();
            let proxy = proxy.clone();
            if let Err(error) = native::dispatch(hwnd, move || {
                let mut app = app.borrow_mut();
                match app.event(event) {
                    Ok(true) => send(&proxy, EventMessage::Exit),
                    Ok(false) => {}
                    Err(e) => app.notify(e),
                }
                if let Err(e) = app.layout().and_then(|()| app.publish()) {
                    app.notify(e);
                }
            }) {
                eprintln!("Papyrus: {error}");
            }
        }
    });
    if let Ok(app) = app.try_borrow() {
        let _ = fs::remove_file(app.data_dir.join("running.flag"));
    }
    drop(app);
    std::process::exit(0);
}

fn main() {
    if let Err(error) = startup() {
        eprintln!("Papyrus: {error}");
        let message = HSTRING::from(error);
        // SAFETY: MessageBox retains no pointer after this blocking call returns.
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                None,
                PCWSTR(message.as_ptr()),
                windows::core::w!("Không mở được Papyrus"),
                windows::Win32::UI::WindowsAndMessaging::MB_OK
                    | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_reject_device_names_and_separators() {
        assert_eq!(super::sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(super::sanitize_filename("../evil.exe"), "_evil.exe");
        assert_eq!(super::sanitize_filename("report?.pdf"), "report_.pdf");
        assert_eq!(super::sanitize_filename("..."), "download.bin");
    }

    #[test]
    fn tab_close_view_and_drop_safe_when_no_view() {
        let mut tab = Tab {
            navigation: 1,
            info: TabInfo {
                id: 10,
                title: "Test Tab".into(),
                url: "https://example.com".into(),
                loading: false,
                error: None,
                pinned: false,
                muted: false,
                blocked: 0,
                is_playing_audio: false,
            },
            view: None,
        };
        // Explicit close_view must be safe and idempotent
        tab.close_view();
        assert!(tab.view.is_none());
        tab.close_view();
        assert!(tab.view.is_none());
        // Dropping tab should execute Drop without panic
        drop(tab);
    }

    #[test]
    fn event_message_process_failed_variants() {
        let msg = EventMessage::ProcessFailed(
            101,
            1,
            "Tiến trình hiển thị trang web đã bị dừng đột ngột (Crash). Nhấn Tải lại để khôi phục."
                .into(),
        );
        match msg {
            EventMessage::ProcessFailed(id, kind, ref text) => {
                assert_eq!(id, 101);
                assert_eq!(kind, 1);
                assert!(text.contains("Crash"));
            }
            _ => panic!("Expected ProcessFailed variant"),
        }
    }

    #[test]
    fn internal_page_urls_trigger_view_cleanup() {
        let internal_urls = [
            SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
        ];
        for url in &internal_urls {
            assert!(url.starts_with("papyrus://"));
        }
    }
}
