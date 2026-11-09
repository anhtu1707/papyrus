use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
use url::Url;

pub const NEW_TAB: &str = "papyrus://newtab";
pub const SETTINGS: &str = "papyrus://settings";
pub const HISTORY: &str = "papyrus://history";
pub const BOOKMARKS: &str = "papyrus://bookmarks";
pub const DOWNLOADS: &str = "papyrus://downloads";
pub const PASSWORDS: &str = "papyrus://passwords";
pub const EXTENSIONS: &str = "papyrus://extensions";
pub const BUILTIN_BLOCKS: &[&str] = &[
    "doubleclick.net",
    "google-analytics.com",
    "googlesyndication.com",
    "googletagmanager.com",
    "googleadservices.com",
    "adservice.google.com",
    "googleads.g.doubleclick.net",
    "pagead2.googlesyndication.com",
    "securepubads.g.doubleclick.net",
    "static.doubleclick.net",
    "partner.googleadservices.com",
    "ads.google.com",
    "adservice.google.com.vn",
    "adnxs.com",
    "adnxs-simple.com",
    "criteo.com",
    "criteo.net",
    "taboola.com",
    "taboolasyndication.com",
    "outbrain.com",
    "outbrainimg.com",
    "scorecardresearch.com",
    "amazon-adsystem.com",
    "pubmatic.com",
    "openx.net",
    "rubiconproject.com",
    "casalemedia.com",
    "media.net",
    "smartadserver.com",
    "coinhive.com",
    "coin-hive.com",
    "jsecoin.com",
    "crypto-loot.com",
    "authedmine.com",
    "monerominer.rocks",
    "popads.net",
    "popcash.net",
    "adroll.com",
    "googletagservices.com",
    "eclick.vn",
    "ambient-platform.vn",
    "admicro.vn",
    "vietnamnetad.vn",
    "ads.twitter.com",
    "ads.linkedin.com",
    "analytics.tiktok.com",
    "hotjar.com",
    "hotjar.io",
    "crazyegg.com",
    "fullstory.com",
    "mc.yandex.ru",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub title: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub engine: String,
    pub zoom: u16,
    pub theme: String,
    pub startup: String,
    pub show_bookmarks: bool,
    pub adblock: bool,
    pub block_popups: bool,
    pub allowed_hosts: Vec<String>,
    pub blocked_hosts: Vec<String>,
    pub download_dir: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            engine: "google".into(),
            zoom: 100,
            theme: "parchment".into(),
            startup: "newtab".into(),
            show_bookmarks: true,
            adblock: true,
            block_popups: true,
            allowed_hosts: vec![],
            blocked_hosts: vec![],
            download_dir: String::new(),
        }
    }
}

impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        if !["google", "duckduckgo", "brave"].contains(&self.engine.as_str())
            || !["parchment", "kraft", "dark"].contains(&self.theme.as_str())
            || !["newtab", "restore", "search"].contains(&self.startup.as_str())
            || !(50..=200).contains(&self.zoom)
        {
            return Err("Cài đặt không hợp lệ.".into());
        }
        for host in self.allowed_hosts.iter().chain(&self.blocked_hosts) {
            if !valid_rule_or_host(host) {
                return Err(format!("Tên miền hoặc quy tắc không hợp lệ: {host}"));
            }
        }
        if self.allowed_hosts.len() + self.blocked_hosts.len() > 10_000 {
            return Err("Danh sách bộ lọc quá lớn (tối đa 10.000 tên miền).".into());
        }
        if !self.download_dir.is_empty() && !Path::new(&self.download_dir).is_absolute() {
            return Err("Thư mục tải xuống phải là đường dẫn tuyệt đối.".into());
        }
        Ok(())
    }
}

pub fn valid_rule_or_host(entry: &str) -> bool {
    let entry = entry.trim();
    if entry.is_empty()
        || entry.len() > 1024
        || entry.contains(' ')
        || entry.contains('\0')
        || entry.contains('\n')
        || entry.contains('\r')
        || entry.starts_with('!')
        || entry.starts_with('#')
    {
        return false;
    }
    if valid_host(entry) {
        return true;
    }
    adblock::lists::parse_filter(entry, false, adblock::lists::ParseOptions::default()).is_ok()
}

pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host == host.to_ascii_lowercase()
        && host.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub title: String,
    pub url: String,
    pub time: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PasswordEntry {
    pub id: u64,
    pub url: String,
    pub username: String,
    pub password: String,
    pub updated: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedState {
    pub preferences: Preferences,
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<HistoryEntry>,
    pub passwords: Vec<PasswordEntry>,
    pub restored_urls: Vec<String>,
    pub restored_active: usize,
    pub last_session_urls: Vec<String>,
    pub last_session_active: usize,
    pub last_session_pinned: Vec<bool>,
    pub window_x: Option<i32>,
    pub window_y: Option<i32>,
    pub window_width: Option<u32>,
    pub window_height: Option<u32>,
    pub window_maximized: bool,
}

impl Default for SavedState {
    fn default() -> Self {
        Self {
            preferences: Preferences::default(),
            bookmarks: vec![
                Bookmark {
                    title: "YouTube".into(),
                    url: "https://youtube.com/".into(),
                },
                Bookmark {
                    title: "GitHub".into(),
                    url: "https://github.com/".into(),
                },
                Bookmark {
                    title: "Wikipedia".into(),
                    url: "https://wikipedia.org/".into(),
                },
            ],
            history: vec![],
            passwords: vec![],
            restored_urls: vec![],
            restored_active: 0,
            last_session_urls: vec![],
            last_session_active: 0,
            last_session_pinned: vec![],
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
            window_maximized: false,
        }
    }
}

impl SavedState {
    pub fn read(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let bytes = fs::read(path).map_err(|e| format!("Không đọc được cài đặt: {e}"))?;
        let state: Self = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Tệp cài đặt bị lỗi; bản gốc được giữ nguyên: {e}"))?;
        state.preferences.validate()?;
        if state.bookmarks.len() > 10_000
            || state
                .bookmarks
                .iter()
                .any(|b| b.title.len() > 1024 || !allowed_navigation(&b.url))
        {
            return Err("Dữ liệu ghim không hợp lệ; bản gốc được giữ nguyên.".into());
        }
        Ok(state)
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        let tmp = path.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        use std::io::Write;
        file.write_all(&data)
            .and_then(|()| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(tmp, path).map_err(|e| format!("Không lưu được cài đặt: {e}"))
    }
}

pub fn search_home(engine: &str) -> &'static str {
    match engine {
        "duckduckgo" => "https://duckduckgo.com/",
        "brave" => "https://search.brave.com/",
        _ => "https://www.google.com/",
    }
}

pub fn allowed_navigation(raw: &str) -> bool {
    raw == NEW_TAB
        || raw == SETTINGS
        || raw == HISTORY
        || raw == BOOKMARKS
        || raw == DOWNLOADS
        || raw == PASSWORDS
        || raw == EXTENSIONS
        || raw == "about:blank"
        || Url::parse(raw).is_ok_and(|u| {
            matches!(u.scheme(), "http" | "https" | "file")
                && (u.scheme() == "file" || u.host_str().is_some())
                && u.username().is_empty()
                && u.password().is_none()
        })
}

pub fn resolve_input(input: &str, engine: &str) -> Result<String, String> {
    let input = input.trim().trim_matches('"').trim_matches('\'').trim();
    if input.is_empty() {
        return Ok(NEW_TAB.into());
    }

    // Normalize internal page aliases (schemes: about:, chrome:, papyrus: and bare names)
    let lower = input.to_ascii_lowercase();
    let stripped = if let Some(rest) = lower.strip_prefix("papyrus://") {
        rest
    } else if let Some(rest) = lower.strip_prefix("papyrus:") {
        rest.trim_start_matches('/')
    } else if let Some(rest) = lower.strip_prefix("chrome://") {
        rest
    } else if let Some(rest) = lower.strip_prefix("chrome:") {
        rest.trim_start_matches('/')
    } else if let Some(rest) = lower.strip_prefix("about://") {
        rest
    } else if let Some(rest) = lower.strip_prefix("about:") {
        rest.trim_start_matches('/')
    } else {
        lower.as_str()
    };
    let page = stripped.trim_matches('/');

    match page {
        "settings" => return Ok(SETTINGS.into()),
        "history" => return Ok(HISTORY.into()),
        "bookmarks" => return Ok(BOOKMARKS.into()),
        "downloads" => return Ok(DOWNLOADS.into()),
        "passwords" => return Ok(PASSWORDS.into()),
        "extensions" => return Ok(EXTENSIONS.into()),
        "newtab" | "new-tab" => return Ok(NEW_TAB.into()),
        "blank" if lower.starts_with("about:") => return Ok("about:blank".into()),
        _ => {}
    }

    // Direct file:// URI
    if input.starts_with("file://") {
        return if allowed_navigation(input) {
            Ok(input.into())
        } else {
            Err("Đường dẫn tệp không hợp lệ.".into())
        };
    }
    // Windows local file path (e.g. C:\path\file.pdf, D:/file.html, or existing local path)
    let path = std::path::Path::new(input);
    let is_win_drive = input.len() >= 3
        && input.as_bytes()[0].is_ascii_alphabetic()
        && input.as_bytes()[1] == b':'
        && (input.as_bytes()[2] == b'\\' || input.as_bytes()[2] == b'/');
    if (is_win_drive
        || path.is_absolute()
        || (path.exists() && (input.contains('.') || input.contains('\\') || input.contains('/'))))
        && let Ok(url) = Url::from_file_path(path)
    {
        return Ok(url.into());
    }
    if input.contains("://")
        || input.starts_with("javascript:")
        || input.starts_with("data:")
        || input.starts_with("about:")
    {
        return if allowed_navigation(input) {
            Ok(Url::parse(input).map_err(|e| e.to_string())?.into())
        } else {
            Err(
                "Chỉ cho phép địa chỉ HTTP/HTTPS, tệp cục bộ (file://) và trang nội bộ Papyrus."
                    .into(),
            )
        };
    }
    let first = input.split('/').next().unwrap_or_default();
    if !input.chars().any(char::is_whitespace)
        && (first.contains('.') || first.starts_with("localhost") || first.starts_with('['))
    {
        let local = first == "localhost"
            || first.starts_with("localhost:")
            || first.starts_with("127.")
            || first.starts_with("[::1]");
        let candidate = format!("{}://{input}", if local { "http" } else { "https" });
        if allowed_navigation(&candidate) {
            return Ok(Url::parse(&candidate).map_err(|e| e.to_string())?.into());
        }
    }
    let mut search = Url::parse(search_home(engine)).expect("constant search URL");
    if engine == "brave" || engine == "google" {
        search.set_path("/search");
    }
    search.query_pairs_mut().append_pair("q", input);
    Ok(search.into())
}

pub const BRAVE_ADBLOCK_RULES: &str = r#"
! Papyrus Bundled Rules - Ad networks, telemetry, trackers & coin miners
||googlesyndication.com^
||googleads.g.doubleclick.net^
||pagead2.googlesyndication.com^
||securepubads.g.doubleclick.net^
||static.doubleclick.net^
||partner.googleadservices.com^
||googleadservices.com^
||adservice.google.com^
||adservice.google.com.vn^
||ads.google.com^
||doubleclick.net^
||google-analytics.com^
||analytics.google.com^
||googletagmanager.com^
||googletagservices.com^
||youtube.com/pagead/*
||youtube.com/api/stats/ads*
||youtube.com/api/stats/atr*
||youtube.com/ptracking*
||youtube.com/youtubei/v1/att/get*
||youtube.com/get_midroll_info*
||youtube.com/api/stats/qoe?adformat=*
||s.youtube.com/api/stats/ads*
||s.youtube.com/api/stats/atr*
||s.youtube.com/api/stats/qoe?adformat=*
||play.google.com/log*
||adnxs.com^
||adnxs-simple.com^
||criteo.com^
||criteo.net^
||taboola.com^
||taboolasyndication.com^
||outbrain.com^
||outbrainimg.com^
||scorecardresearch.com^
||amazon-adsystem.com^
||pubmatic.com^
||openx.net^
||rubiconproject.com^
||casalemedia.com^
||media.net^
||smartadserver.com^
||coinhive.com^
||coin-hive.com^
||jsecoin.com^
||crypto-loot.com^
||authedmine.com^
||monerominer.rocks^
||minr.pw^
||webminepool.com^
||popads.net^
||popcash.net^
||adroll.com^
||eclick.vn^
||ambient-platform.vn^
||admicro.vn^
||vietnamnetad.vn^
||ads.twitter.com^
||analytics.twitter.com^
||ads.linkedin.com^
||px.ads.linkedin.com^
||analytics.tiktok.com^
||hotjar.com^
||hotjar.io^
||crazyegg.com^
||fullstory.com^
||mc.yandex.ru^
||facebook.com/tr/*
||connect.facebook.net/signals/*
! Exceptions protecting first-party functionality
@@||accounts.google.com^
@@||apis.google.com^
@@||fonts.googleapis.com^
@@||ajax.googleapis.com^
@@||gstatic.com^
@@||connect.facebook.net/*/sdk.js
@@||connect.facebook.net/en_US/sdk.js
@@||connect.facebook.net/vi_VN/sdk.js
"#;

pub fn is_standard_rule(rule: &str) -> bool {
    rule.contains("||")
        || rule.contains("@@")
        || rule.contains('^')
        || rule.contains('$')
        || rule.contains('*')
        || rule.contains('/')
        || rule.starts_with('|')
        || rule.ends_with('|')
}

pub fn extract_rule_domain(rule: &str) -> Option<&str> {
    let mut s = rule.trim();
    if let Some(rest) = s.strip_prefix("@@") {
        s = rest;
    }
    if let Some(rest) = s.strip_prefix("||") {
        s = rest;
    }
    if let Some(rest) = s.strip_prefix("*.") {
        s = rest;
    }
    if let Some(idx) = s.find(['^', '$', '/', '*']) {
        s = &s[..idx];
    }
    let s = s.trim_matches('.');
    if !s.is_empty() { Some(s) } else { None }
}

fn hash_rules(blocked: &[String], allowed: &[String]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    blocked.hash(&mut hasher);
    allowed.hash(&mut hasher);
    hasher.finish()
}

fn build_engine(blocked: &[String], allowed: &[String]) -> adblock::Engine {
    let mut filter_set = adblock::lists::FilterSet::new(false);
    filter_set.add_filter_list(
        BRAVE_ADBLOCK_RULES.to_string(),
        adblock::lists::ParseOptions::default(),
    );

    let mut user_rules = String::new();
    for rule in blocked {
        let trimmed = rule.trim();
        if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('#') {
            continue;
        }
        if is_standard_rule(trimmed) {
            user_rules.push_str(trimmed);
        } else {
            user_rules.push_str(&format!("||{trimmed}^"));
        }
        user_rules.push('\n');
    }

    for rule in allowed {
        let trimmed = rule.trim();
        if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("@@") {
            user_rules.push_str(trimmed);
        } else if is_standard_rule(trimmed) {
            user_rules.push_str(&format!("@@{trimmed}"));
        } else {
            user_rules.push_str(&format!("@@||{trimmed}^"));
        }
        user_rules.push('\n');
    }

    if !user_rules.is_empty() {
        filter_set.add_filter_list(user_rules, adblock::lists::ParseOptions::default());
    }

    adblock::Engine::new_with_filter_set(filter_set)
}

struct CachedEngine {
    preferences_hash: u64,
    engine: adblock::Engine,
}

thread_local! {
    static ENGINE_CACHE: std::cell::RefCell<Option<CachedEngine>> = const { std::cell::RefCell::new(None) };
}

pub fn domain_matches(host: &str, rule: &str) -> bool {
    host == rule
        || host
            .strip_suffix(rule)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

pub fn should_block(
    raw: &str,
    top_url: &str,
    resource_type: &str,
    method: &str,
    settings: &Preferences,
) -> bool {
    if !settings.adblock {
        return false;
    }
    let Ok(url) = Url::parse(raw) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let top = Url::parse(top_url).ok();
    let top_host = top.as_ref().and_then(Url::host_str).unwrap_or_default();

    // 1. User Whitelist (Sub-resource whitelisting invariant for top-level pages)
    if settings.allowed_hosts.iter().any(|r| {
        let trimmed = r.trim();
        if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('#') {
            return false;
        }
        let domain = extract_rule_domain(trimmed).unwrap_or(trimmed);
        (!is_standard_rule(trimmed) || trimmed.contains("$document"))
            && !top_host.is_empty()
            && domain_matches(top_host, domain)
            || (!is_standard_rule(trimmed) && domain_matches(host, domain))
    }) {
        return false;
    }

    // 2. Adblock Engine (Compiled bundled rules + custom filter rules with authentic exception evaluation)
    let rules_hash = hash_rules(&settings.blocked_hosts, &settings.allowed_hosts);
    let engine_decision = ENGINE_CACHE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if borrow.as_ref().map(|c| c.preferences_hash) != Some(rules_hash) {
            *borrow = Some(CachedEngine {
                preferences_hash: rules_hash,
                engine: build_engine(&settings.blocked_hosts, &settings.allowed_hosts),
            });
        }
        let engine = &borrow.as_ref().unwrap().engine;
        if let Ok(req) = adblock::request::Request::new(raw, top_url, resource_type, method) {
            let res = engine.check_network_request(&req);
            if res.exception.is_some() {
                Some(false)
            } else if res.should_block() {
                Some(true)
            } else {
                None
            }
        } else {
            None
        }
    });

    if let Some(decision) = engine_decision {
        return decision;
    }

    // 3. User Blacklist Fallback (Direct domain match for plain hosts or plain domain rules when engine has no match)
    if settings.blocked_hosts.iter().any(|r| {
        let trimmed = r.trim();
        if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('#') {
            return false;
        }
        if !is_standard_rule(trimmed) {
            domain_matches(host, trimmed)
        } else if !trimmed.contains('$') && !trimmed.contains('/') {
            let domain = extract_rule_domain(trimmed).unwrap_or(trimmed);
            domain_matches(host, domain)
        } else {
            false
        }
    }) {
        return true;
    }

    // 4. YouTube & GoogleVideo heuristics
    if host.ends_with("youtube.com") {
        let path = url.path();
        if path.starts_with("/pagead/")
            || path.starts_with("/api/stats/ads")
            || path.starts_with("/api/stats/atr")
            || path.starts_with("/ptracking")
            || path.starts_with("/youtubei/v1/att/get")
            || (path.starts_with("/api/stats/qoe")
                && url.query().is_some_and(|q| q.contains("adformat")))
        {
            return true;
        }
    }

    if host.ends_with("googlevideo.com")
        && let Some(query) = url.query()
        && (query.contains("adformat=")
            || query.contains("ctier=L")
            || query.contains("el=adunit")
            || query.contains("ad_type="))
    {
        return true;
    }

    // 5. Built-in domain blocklist fallback
    BUILTIN_BLOCKS
        .iter()
        .copied()
        .any(|r| domain_matches(host, r))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub blocked: u64,
    pub error: Option<String>,
    pub pinned: bool,
    pub muted: bool,
    pub is_playing_audio: bool,
}

pub fn next_active_after_close(ids: &[u64], active: u64, closing: u64) -> Option<u64> {
    if closing != active {
        return ids.iter().copied().find(|id| *id == active);
    }
    let index = ids.iter().position(|id| *id == closing)?;
    ids.get(index + 1)
        .or_else(|| index.checked_sub(1).and_then(|i| ids.get(i)))
        .copied()
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Ready,
    FocusAddress,
    Navigate {
        input: String,
    },
    NewTab,
    NewWindow,
    SwitchTab {
        id: u64,
    },
    CloseTab {
        id: u64,
    },
    Back,
    Forward,
    Reload,
    Home,
    Close,
    Minimize,
    Maximize,
    Snap {
        layout: String,
    },
    Drag,
    Settings,
    History,
    Bookmarks,
    Downloads,
    Passwords,
    Extensions,
    SavePassword {
        url: String,
        username: String,
        password: String,
    },
    DeletePassword {
        id: u64,
    },
    ClearPasswords,
    OpenDownloadedFile {
        path: String,
    },
    ShowDownloadInFolder {
        path: String,
    },
    ClearDownloads,
    ReopenTab,
    RestoreTab,
    PinTab {
        id: u64,
    },
    ToggleMuteTab {
        id: u64,
    },
    DuplicateTab {
        id: u64,
    },
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Print,
    NextTab,
    PrevTab,
    SelectTabByIndex {
        index: usize,
    },
    SelectLastTab,
    ClearHistory,
    DeleteHistoryEntry {
        index: usize,
    },
    SavePreferences {
        preferences: Preferences,
    },
    ResetPreferences,
    Bookmark,
    AddBookmark {
        title: String,
        url: String,
    },
    DeleteBookmark {
        index: usize,
    },
    ClearBookmarks,
    ClearData,
    CancelClear,
    DevTools,
    HeaderHeight {
        height: u16,
    },
    ShowMenu {
        x: i32,
        y: i32,
    },
    ShowSnapMenu {
        x: i32,
        y: i32,
    },
    ShowTabMenu {
        id: u64,
        x: i32,
        y: i32,
    },
    SetDefaultBrowser,
    ToggleFullscreen,
    ReorderTab {
        from: u64,
        to: u64,
    },
    CheckUpdate,
}

pub fn parse_command(origin: &str, body: &str) -> Option<Command> {
    // Only the locked, application-owned about:blank header has an IPC handler.
    if origin != "about:blank" || body.len() > 256_000 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let object = value.as_object()?;
    let action = object.get("action")?.as_str()?;
    let allowed_extra: &[&str] = match action {
        "navigate" => &["input"],
        "switch_tab" | "close_tab" | "delete_password" | "pin_tab" | "toggle_mute_tab"
        | "duplicate_tab" => &["id"],
        "save_password" => &["url", "username", "password"],
        "add_bookmark" => &["title", "url"],
        "save_preferences" => &["preferences"],
        "delete_bookmark" | "delete_history_entry" | "select_tab_by_index" => &["index"],
        "header_height" => &["height"],
        "snap" => &["layout"],
        "show_menu" | "show_snap_menu" => &["x", "y"],
        "show_tab_menu" => &["id", "x", "y"],
        "reorder_tab" => &["from", "to"],
        "open_downloaded_file" | "show_download_in_folder" => &["path"],
        _ => &[],
    };
    if object
        .keys()
        .any(|key| key != "action" && !allowed_extra.contains(&key.as_str()))
    {
        return None;
    }
    serde_json::from_value(value).ok()
}

pub fn is_newer_version(remote: &str, current: &str) -> bool {
    let parse_parts = |s: &str| -> Vec<u32> {
        s.trim_start_matches(|c: char| !c.is_ascii_digit())
            .split('.')
            .map(|part| part.chars().take_while(|c| c.is_ascii_digit()).collect::<String>())
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let remote_parts = parse_parts(remote);
    let current_parts = parse_parts(current);
    for (r, c) in remote_parts.iter().zip(current_parts.iter()) {
        if r > c {
            return true;
        }
        if r < c {
            return false;
        }
    }
    remote_parts.len() > current_parts.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_searches_and_local_addresses() {
        assert!(
            resolve_input("Rust 1.90 release", "google")
                .unwrap()
                .starts_with("https://www.google.com/search?q=")
        );
        assert_eq!(
            resolve_input("localhost:3000", "google").unwrap(),
            "http://localhost:3000/"
        );
        assert_eq!(
            resolve_input("127.0.0.1:18765/x", "google").unwrap(),
            "http://127.0.0.1:18765/x"
        );
        assert_eq!(
            resolve_input("example.org/settings.html", "google").unwrap(),
            "https://example.org/settings.html"
        );
        assert!(resolve_input("javascript:alert(1)", "google").is_err());
        assert_eq!(
            resolve_input("file:///C:/secret", "google").unwrap(),
            "file:///C:/secret"
        );
        assert!(resolve_input("https://user:password@example.org", "google").is_err());
    }
    #[test]
    fn domain_filter_has_boundaries_and_exceptions() {
        let mut p = Preferences::default();
        assert!(should_block(
            "https://a.doubleclick.net/ad.js",
            "https://example.org",
            "script",
            "GET",
            &p
        ));
        assert!(!should_block(
            "https://notdoubleclick.net/x",
            "https://example.org",
            "script",
            "GET",
            &p
        ));
        assert!(!should_block(
            "https://example.org/?q=doubleclick.net",
            "https://example.org",
            "script",
            "GET",
            &p
        ));
        p.allowed_hosts.push("example.org".into());
        assert!(!should_block(
            "https://doubleclick.net/ad",
            "https://www.example.org",
            "script",
            "GET",
            &p
        ));
        p.allowed_hosts.clear();
        p.adblock = false;
        assert!(!should_block(
            "https://doubleclick.net/ad",
            "https://example.org",
            "script",
            "GET",
            &p
        ));
    }

    #[test]
    fn test_valid_rule_or_host() {
        assert!(valid_rule_or_host("example.com"));
        assert!(valid_rule_or_host("sub.example.co.uk"));
        assert!(valid_rule_or_host("||adservice.google.com^"));
        assert!(valid_rule_or_host("@@||legit-site.com^$document"));
        assert!(valid_rule_or_host("@@||cdn.assets.com^$script,stylesheet"));
        assert!(valid_rule_or_host("*.doubleclick.net"));
        assert!(valid_rule_or_host("||youtube.com/pagead/*"));
        assert!(valid_rule_or_host("||bad.com^$third-party,image"));

        // Negative cases
        assert!(!valid_rule_or_host(""));
        assert!(!valid_rule_or_host("   "));
        assert!(!valid_rule_or_host("invalid host with spaces"));
        assert!(!valid_rule_or_host("! comment line"));
        assert!(!valid_rule_or_host("# comment line"));
    }

    #[test]
    fn test_resource_context_and_method_forwarding() {
        let p = Preferences::default();
        // Script request blocking
        assert!(should_block(
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "https://example.org",
            "script",
            "GET",
            &p
        ));
        // Image request blocking
        assert!(should_block(
            "https://googleads.g.doubleclick.net/pagead/imgad?id=123",
            "https://example.org",
            "image",
            "GET",
            &p
        ));
        // POST XMLHttpRequest blocking
        assert!(should_block(
            "https://analytics.tiktok.com/api/v2/track",
            "https://example.org",
            "xmlhttprequest",
            "POST",
            &p
        ));
        // Legitimate main_frame request should NOT be blocked
        assert!(!should_block(
            "https://example.org/",
            "https://example.org/",
            "main_frame",
            "GET",
            &p
        ));
    }

    #[test]
    fn test_bundled_rules_ad_and_coin_miner_coverage() {
        let p = Preferences::default();
        // Major ad networks blocked
        assert!(should_block(
            "https://pubmatic.com/ad.js",
            "https://news.com",
            "script",
            "GET",
            &p
        ));
        assert!(should_block(
            "https://openx.net/track",
            "https://news.com",
            "xmlhttprequest",
            "GET",
            &p
        ));
        assert!(should_block(
            "https://rubiconproject.com/pixel",
            "https://news.com",
            "image",
            "GET",
            &p
        ));
        assert!(should_block(
            "https://popcash.net/pop.js",
            "https://news.com",
            "script",
            "GET",
            &p
        ));
        assert!(should_block(
            "https://mc.yandex.ru/metrika/tag.js",
            "https://news.com",
            "script",
            "GET",
            &p
        ));

        // Crypto miners blocked
        assert!(should_block(
            "https://coinhive.com/lib/miner.js",
            "https://news.com",
            "script",
            "GET",
            &p
        ));
        assert!(should_block(
            "https://crypto-loot.com/miner.js",
            "https://news.com",
            "script",
            "GET",
            &p
        ));
        assert!(should_block(
            "https://jsecoin.com/server.js",
            "https://news.com",
            "script",
            "GET",
            &p
        ));
        assert!(should_block(
            "https://authedmine.com/lib/authedmine.min.js",
            "https://news.com",
            "script",
            "GET",
            &p
        ));
        assert!(should_block(
            "https://monerominer.rocks/miner.js",
            "https://news.com",
            "script",
            "GET",
            &p
        ));

        // First-party exceptions MUST NOT be blocked
        assert!(!should_block(
            "https://accounts.google.com/o/oauth2/v2/auth",
            "https://example.com",
            "main_frame",
            "GET",
            &p
        ));
        assert!(!should_block(
            "https://fonts.googleapis.com/css?family=Open+Sans",
            "https://example.com",
            "stylesheet",
            "GET",
            &p
        ));
        assert!(!should_block(
            "https://ajax.googleapis.com/ajax/libs/jquery/3.6.0/jquery.min.js",
            "https://example.com",
            "script",
            "GET",
            &p
        ));
        assert!(!should_block(
            "https://connect.facebook.net/en_US/sdk.js",
            "https://example.com",
            "script",
            "GET",
            &p
        ));
        assert!(!should_block(
            "https://connect.facebook.net/vi_VN/sdk.js",
            "https://example.com",
            "script",
            "GET",
            &p
        ));
    }

    #[test]
    fn test_whitelisting_and_exception_rules_prevent_layout_breakage() {
        let mut p = Preferences::default();

        // Non-whitelisted site: ads are blocked
        assert!(should_block(
            "https://doubleclick.net/ad.js",
            "https://news.com/",
            "script",
            "GET",
            &p
        ));

        // Whitelisting site via plain domain allows all sub-resources (fonts, stylesheets, scripts)
        p.allowed_hosts.push("legit-site.com".into());
        assert!(!should_block(
            "https://legit-site.com/app.js",
            "https://legit-site.com/",
            "script",
            "GET",
            &p
        ));
        assert!(!should_block(
            "https://fonts.googleapis.com/css",
            "https://legit-site.com/",
            "stylesheet",
            "GET",
            &p
        ));
        assert!(!should_block(
            "https://fonts.gstatic.com/font.woff2",
            "https://legit-site.com/",
            "font",
            "GET",
            &p
        ));
        assert!(!should_block(
            "https://doubleclick.net/ad.js",
            "https://legit-site.com/",
            "script",
            "GET",
            &p
        ));

        // Whitelisting via exception rule @@||other-site.org^$document
        p.allowed_hosts.clear();
        p.allowed_hosts.push("@@||other-site.org^$document".into());
        assert!(!should_block(
            "https://cdn.other-site.org/style.css",
            "https://other-site.org/",
            "stylesheet",
            "GET",
            &p
        ));
        assert!(!should_block(
            "https://doubleclick.net/ad.js",
            "https://other-site.org/",
            "script",
            "GET",
            &p
        ));

        // User custom blocked host rule
        p.blocked_hosts.push("||custom-adserver.com^$script".into());
        assert!(should_block(
            "https://custom-adserver.com/ads.js",
            "https://news.com/",
            "script",
            "GET",
            &p
        ));
    }
    #[test]
    fn closing_other_tab_preserves_active_identity() {
        assert_eq!(next_active_after_close(&[1, 2, 3], 2, 1), Some(2));
        assert_eq!(next_active_after_close(&[1, 2, 3], 2, 2), Some(3));
        assert_eq!(next_active_after_close(&[1, 2, 3], 3, 3), Some(2));
        assert_eq!(next_active_after_close(&[1], 1, 1), None);
    }
    #[test]
    fn rejects_external_and_legacy_ipc() {
        assert!(parse_command("https://example.org/", r#"{"action":"clear_data"}"#).is_none());
        assert!(parse_command("about:blank", "url_changed:https://bank.test").is_none());
        assert!(parse_command("about:blank", r#"{"action":"ready","extra":1}"#).is_none());
        assert!(matches!(
            parse_command("about:blank", r#"{"action":"new_tab"}"#),
            Some(Command::NewTab)
        ));
    }
    #[test]
    fn settings_validate_and_roundtrip() {
        let mut state = SavedState::default();
        assert!(state.preferences.validate().is_ok());
        state.preferences.zoom = 0;
        assert!(state.preferences.validate().is_err());
        state.preferences.zoom = 125;
        state.preferences.allowed_hosts.push("example.org".into());
        let dir = std::env::temp_dir().join(format!("papyrus_config_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        state.write(&path).unwrap();
        state.preferences.engine = "brave".into();
        state.write(&path).unwrap();
        let read = SavedState::read(&path).unwrap();
        assert_eq!(read.preferences.engine, "brave");
        assert_eq!(read.preferences.zoom, 125);
        fs::write(&path, "corrupt").unwrap();
        assert!(SavedState::read(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "corrupt");
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn passwords_and_extensions_routing_and_roundtrip() {
        assert_eq!(
            resolve_input("papyrus://passwords", "google").unwrap(),
            PASSWORDS
        );
        assert_eq!(
            resolve_input("about:passwords", "google").unwrap(),
            PASSWORDS
        );
        assert_eq!(
            resolve_input("papyrus://extensions", "google").unwrap(),
            EXTENSIONS
        );
        assert_eq!(
            resolve_input("chrome://extensions", "google").unwrap(),
            EXTENSIONS
        );
        assert!(allowed_navigation(PASSWORDS));
        assert!(allowed_navigation(EXTENSIONS));

        let mut state = SavedState::default();
        state.passwords.push(PasswordEntry {
            id: 1,
            url: "https://example.com".into(),
            username: "admin".into(),
            password: "secret_password".into(),
            updated: 1700000000,
        });
        let dir = std::env::temp_dir().join(format!("papyrus_pwd_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        state.write(&path).unwrap();
        let read = SavedState::read(&path).unwrap();
        assert_eq!(read.passwords.len(), 1);
        assert_eq!(read.passwords[0].username, "admin");
        assert_eq!(read.passwords[0].password, "secret_password");
        let _ = fs::remove_file(path);
        let _ = fs::remove_dir(dir);
    }
    #[test]
    fn file_navigation_and_tab_menu_parsing() {
        assert!(allowed_navigation("file:///C:/Users/test/doc.pdf"));
        assert!(allowed_navigation("file:///D:/image.png"));
        assert_eq!(
            resolve_input("file:///C:/doc.pdf", "google").unwrap(),
            "file:///C:/doc.pdf"
        );
        let cmd = parse_command(
            "about:blank",
            r#"{"action":"show_tab_menu","id":2,"x":100,"y":20}"#,
        )
        .unwrap();
        assert!(matches!(
            cmd,
            Command::ShowTabMenu {
                id: 2,
                x: 100,
                y: 20
            }
        ));
        let cmd_def = parse_command("about:blank", r#"{"action":"set_default_browser"}"#).unwrap();
        assert!(matches!(cmd_def, Command::SetDefaultBrowser));
        let cmd_fs = parse_command("about:blank", r#"{"action":"toggle_fullscreen"}"#).unwrap();
        assert!(matches!(cmd_fs, Command::ToggleFullscreen));
        let cmd_reorder =
            parse_command("about:blank", r#"{"action":"reorder_tab","from":1,"to":2}"#).unwrap();
        assert!(matches!(
            cmd_reorder,
            Command::ReorderTab { from: 1, to: 2 }
        ));
    }
    #[test]
    fn test_brave_adblock_engine() {
        use adblock::Engine;
        use adblock::request::Request;

        let rules = r#"
            ||googlesyndication.com^
            ||doubleclick.net^
            ||youtube.com/pagead/*
            ||youtube.com/api/stats/ads*
            ||youtube.com/ptracking*
            ||googleads.g.doubleclick.net^
        "#;
        let engine = Engine::new_with_list_text(rules);

        let req = Request::new(
            "https://www.youtube.com/pagead/ad_view",
            "https://www.youtube.com/",
            "script",
            "GET",
        )
        .unwrap();
        let blocker_result = engine.check_network_request(&req);
        assert!(blocker_result.should_block());

        let clean_req = Request::new(
            "https://www.youtube.com/watch?v=123",
            "https://www.youtube.com/",
            "document",
            "GET",
        )
        .unwrap();
        let clean_result = engine.check_network_request(&clean_req);
        assert!(!clean_result.should_block());
    }

    #[test]
    fn passwords_and_downloads_ipc_whitelist_verification() {
        let save_cmd = parse_command(
            "about:blank",
            r#"{"action":"save_password","url":"https://example.com","username":"admin","password":"secret"}"#,
        );
        assert!(matches!(
            save_cmd,
            Some(Command::SavePassword { url, username, password })
            if url == "https://example.com" && username == "admin" && password == "secret"
        ));

        let del_cmd = parse_command("about:blank", r#"{"action":"delete_password","id":42}"#);
        assert!(matches!(del_cmd, Some(Command::DeletePassword { id: 42 })));

        let clear_pwd_cmd = parse_command("about:blank", r#"{"action":"clear_passwords"}"#);
        assert!(matches!(clear_pwd_cmd, Some(Command::ClearPasswords)));

        let open_dl = parse_command(
            "about:blank",
            r#"{"action":"open_downloaded_file","path":"C:\\file.pdf"}"#,
        );
        assert!(
            matches!(open_dl, Some(Command::OpenDownloadedFile { path }) if path == r"C:\file.pdf")
        );

        let show_dl = parse_command(
            "about:blank",
            r#"{"action":"show_download_in_folder","path":"C:\\file.pdf"}"#,
        );
        assert!(
            matches!(show_dl, Some(Command::ShowDownloadInFolder { path }) if path == r"C:\file.pdf")
        );

        let clear_dl = parse_command("about:blank", r#"{"action":"clear_downloads"}"#);
        assert!(matches!(clear_dl, Some(Command::ClearDownloads)));

        let add_bm = parse_command(
            "about:blank",
            r#"{"action":"add_bookmark","title":"VnExpress","url":"https://vnexpress.net"}"#,
        );
        assert!(matches!(
            add_bm,
            Some(Command::AddBookmark { title, url })
            if title == "VnExpress" && url == "https://vnexpress.net"
        ));

        let clear_bm = parse_command("about:blank", r#"{"action":"clear_bookmarks"}"#);
        assert!(matches!(clear_bm, Some(Command::ClearBookmarks)));

        let bm_page = parse_command("about:blank", r#"{"action":"bookmarks"}"#);
        assert!(matches!(bm_page, Some(Command::Bookmarks)));

        // Rejection of unknown fields
        assert!(
            parse_command(
                "about:blank",
                r#"{"action":"save_password","url":"u","username":"a","password":"p","extra":1}"#
            )
            .is_none()
        );
        assert!(
            parse_command(
                "about:blank",
                r#"{"action":"delete_password","id":42,"extra":1}"#
            )
            .is_none()
        );
        assert!(
            parse_command("about:blank", r#"{"action":"clear_downloads","extra":1}"#).is_none()
        );
        assert!(
            parse_command("about:blank", r#"{"action":"clear_bookmarks","extra":1}"#).is_none()
        );
        assert!(
            parse_command(
                "about:blank",
                r#"{"action":"add_bookmark","title":"t","url":"u","extra":1}"#
            )
            .is_none()
        );
        assert!(
            parse_command(
                "https://evil.org",
                r#"{"action":"open_downloaded_file","path":"C:\\file.pdf"}"#
            )
            .is_none()
        );
    }

    #[test]
    fn omnibox_internal_page_aliases_and_schemes() {
        assert!(allowed_navigation(BOOKMARKS));

        // Settings aliases
        assert_eq!(resolve_input("settings", "google").unwrap(), SETTINGS);
        assert_eq!(resolve_input("about:settings", "google").unwrap(), SETTINGS);
        assert_eq!(
            resolve_input("about://settings", "google").unwrap(),
            SETTINGS
        );
        assert_eq!(
            resolve_input("chrome:settings", "google").unwrap(),
            SETTINGS
        );
        assert_eq!(
            resolve_input("chrome://settings", "google").unwrap(),
            SETTINGS
        );
        assert_eq!(
            resolve_input("chrome://settings/", "google").unwrap(),
            SETTINGS
        );
        assert_eq!(
            resolve_input("papyrus:settings", "google").unwrap(),
            SETTINGS
        );
        assert_eq!(
            resolve_input("papyrus://settings", "google").unwrap(),
            SETTINGS
        );
        assert_eq!(
            resolve_input("CHROME://SETTINGS", "google").unwrap(),
            SETTINGS
        );

        // History aliases
        assert_eq!(resolve_input("history", "google").unwrap(), HISTORY);
        assert_eq!(resolve_input("about:history", "google").unwrap(), HISTORY);
        assert_eq!(
            resolve_input("chrome://history", "google").unwrap(),
            HISTORY
        );
        assert_eq!(
            resolve_input("papyrus://history", "google").unwrap(),
            HISTORY
        );

        // Bookmarks aliases
        assert_eq!(resolve_input("bookmarks", "google").unwrap(), BOOKMARKS);
        assert_eq!(
            resolve_input("about:bookmarks", "google").unwrap(),
            BOOKMARKS
        );
        assert_eq!(
            resolve_input("about://bookmarks", "google").unwrap(),
            BOOKMARKS
        );
        assert_eq!(
            resolve_input("chrome:bookmarks", "google").unwrap(),
            BOOKMARKS
        );
        assert_eq!(
            resolve_input("chrome://bookmarks", "google").unwrap(),
            BOOKMARKS
        );
        assert_eq!(
            resolve_input("chrome://bookmarks/", "google").unwrap(),
            BOOKMARKS
        );
        assert_eq!(
            resolve_input("papyrus:bookmarks", "google").unwrap(),
            BOOKMARKS
        );
        assert_eq!(
            resolve_input("papyrus://bookmarks", "google").unwrap(),
            BOOKMARKS
        );

        // Downloads aliases
        assert_eq!(resolve_input("downloads", "google").unwrap(), DOWNLOADS);
        assert_eq!(
            resolve_input("about:downloads", "google").unwrap(),
            DOWNLOADS
        );
        assert_eq!(
            resolve_input("chrome://downloads", "google").unwrap(),
            DOWNLOADS
        );
        assert_eq!(
            resolve_input("papyrus://downloads", "google").unwrap(),
            DOWNLOADS
        );

        // Passwords aliases
        assert_eq!(resolve_input("passwords", "google").unwrap(), PASSWORDS);
        assert_eq!(
            resolve_input("about:passwords", "google").unwrap(),
            PASSWORDS
        );
        assert_eq!(
            resolve_input("chrome://passwords", "google").unwrap(),
            PASSWORDS
        );
        assert_eq!(
            resolve_input("papyrus://passwords", "google").unwrap(),
            PASSWORDS
        );

        // Extensions aliases
        assert_eq!(resolve_input("extensions", "google").unwrap(), EXTENSIONS);
        assert_eq!(
            resolve_input("about:extensions", "google").unwrap(),
            EXTENSIONS
        );
        assert_eq!(
            resolve_input("chrome://extensions", "google").unwrap(),
            EXTENSIONS
        );
        assert_eq!(
            resolve_input("papyrus://extensions", "google").unwrap(),
            EXTENSIONS
        );

        // New Tab aliases
        assert_eq!(resolve_input("newtab", "google").unwrap(), NEW_TAB);
        assert_eq!(resolve_input("about:newtab", "google").unwrap(), NEW_TAB);
        assert_eq!(resolve_input("chrome://newtab", "google").unwrap(), NEW_TAB);
        assert_eq!(
            resolve_input("papyrus://newtab", "google").unwrap(),
            NEW_TAB
        );
        assert_eq!(
            resolve_input("about:blank", "google").unwrap(),
            "about:blank"
        );

        // External URLs remain unaffected
        assert_eq!(
            resolve_input("https://example.org/settings", "google").unwrap(),
            "https://example.org/settings"
        );
        assert_eq!(
            resolve_input("example.org/history.html", "google").unwrap(),
            "https://example.org/history.html"
        );
    }

    #[test]
    fn tab_lifecycle_and_pinning_command_parsing() {
        // PinTab
        let pin = parse_command("about:blank", r#"{"action":"pin_tab","id":5}"#).unwrap();
        assert!(matches!(pin, Command::PinTab { id: 5 }));
        assert!(parse_command("about:blank", r#"{"action":"pin_tab","id":5,"extra":1}"#).is_none());
        assert!(parse_command("https://evil.com", r#"{"action":"pin_tab","id":5}"#).is_none());

        // ToggleMuteTab
        let mute = parse_command("about:blank", r#"{"action":"toggle_mute_tab","id":42}"#).unwrap();
        assert!(matches!(mute, Command::ToggleMuteTab { id: 42 }));
        assert!(
            parse_command(
                "about:blank",
                r#"{"action":"toggle_mute_tab","id":42,"hack":true}"#
            )
            .is_none()
        );
        assert!(
            parse_command(
                "https://evil.com",
                r#"{"action":"toggle_mute_tab","id":42}"#
            )
            .is_none()
        );

        // DuplicateTab
        let dup = parse_command("about:blank", r#"{"action":"duplicate_tab","id":7}"#).unwrap();
        assert!(matches!(dup, Command::DuplicateTab { id: 7 }));
        assert!(
            parse_command(
                "about:blank",
                r#"{"action":"duplicate_tab","id":7,"evil":true}"#
            )
            .is_none()
        );

        // RestoreTab & ReopenTab
        let restore = parse_command("about:blank", r#"{"action":"restore_tab"}"#).unwrap();
        assert!(matches!(restore, Command::RestoreTab));
        let reopen = parse_command("about:blank", r#"{"action":"reopen_tab"}"#).unwrap();
        assert!(matches!(reopen, Command::ReopenTab));
        assert!(
            parse_command("about:blank", r#"{"action":"restore_tab","invalid":true}"#).is_none()
        );
    }

    #[test]
    fn tab_info_serialization_roundtrip() {
        let info = TabInfo {
            id: 1,
            title: "Test".into(),
            url: "https://example.com".into(),
            loading: false,
            blocked: 3,
            error: None,
            pinned: true,
            muted: true,
            is_playing_audio: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"pinned\":true"));
        assert!(json.contains("\"muted\":true"));
        assert!(json.contains("\"is_playing_audio\":true"));

        let deserialized: TabInfo = serde_json::from_str(&json).unwrap();
        assert!(deserialized.pinned);
        assert!(deserialized.muted);
        assert!(deserialized.is_playing_audio);
        assert_eq!(deserialized.id, 1);
        assert_eq!(deserialized.url, "https://example.com");
    }

    #[test]
    fn saved_state_geometry_and_pinned_serialization() {
        let state = SavedState {
            window_x: Some(120),
            window_y: Some(80),
            window_width: Some(1400),
            window_height: Some(900),
            window_maximized: true,
            last_session_pinned: vec![true, false, true],
            ..Default::default()
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"window_x\":120"));
        assert!(json.contains("\"window_maximized\":true"));
        assert!(json.contains("\"last_session_pinned\":[true,false,true]"));

        let restored: SavedState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.window_x, Some(120));
        assert_eq!(restored.window_y, Some(80));
        assert_eq!(restored.window_width, Some(1400));
        assert_eq!(restored.window_height, Some(900));
        assert!(restored.window_maximized);
        assert_eq!(restored.last_session_pinned, vec![true, false, true]);

        // Backward compatibility: old state without window geometry and last_session_pinned
        let legacy_json = r#"{"preferences":{"engine":"google","zoom":100,"theme":"system","startup":"newtab","show_bookmarks":true,"adblock":true,"block_popups":true,"allowed_hosts":[],"blocked_hosts":[],"download_dir":""},"bookmarks":[],"history":[],"passwords":[],"restored_urls":[],"restored_active":0,"last_session_urls":["https://example.com"],"last_session_active":0}"#;
        let legacy: SavedState = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(legacy.window_x, None);
        assert_eq!(legacy.window_y, None);
        assert_eq!(legacy.window_width, None);
        assert_eq!(legacy.window_height, None);
        assert!(!legacy.window_maximized);
        assert!(legacy.last_session_pinned.is_empty());
        assert_eq!(legacy.last_session_urls, vec!["https://example.com"]);
    }

    #[test]
    fn test_version_check_and_command() {
        assert!(is_newer_version("0.3.8", "0.3.7"));
        assert!(is_newer_version("v0.3.8", "0.3.7"));
        assert!(is_newer_version("1.0.0", "0.3.7"));
        assert!(!is_newer_version("0.3.7", "0.3.7"));
        assert!(!is_newer_version("0.3.6", "0.3.7"));
        assert!(is_newer_version("0.3.7.1", "0.3.7"));

        let cmd = parse_command("about:blank", r#"{"action":"check_update"}"#).unwrap();
        assert!(matches!(cmd, Command::CheckUpdate));
    }
}
