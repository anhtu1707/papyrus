#![allow(dead_code, unused_imports)]

#[path = "../src/model.rs"]
mod model;

use model::{
    BOOKMARKS, Bookmark, Command, DOWNLOADS, EXTENSIONS, HISTORY, HistoryEntry, NEW_TAB, PASSWORDS,
    PasswordEntry, Preferences, SETTINGS, SavedState, TabInfo, parse_command,
};

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
};

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
    COREWEBVIEW2_PROCESS_FAILED_KIND_UNKNOWN_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED, COREWEBVIEW2_PROCESS_FAILED_REASON,
    COREWEBVIEW2_PROCESS_FAILED_REASON_CRASHED, COREWEBVIEW2_PROCESS_FAILED_REASON_OUT_OF_MEMORY,
    COREWEBVIEW2_PROCESS_FAILED_REASON_TERMINATED, COREWEBVIEW2_PROCESS_FAILED_REASON_UNRESPONSIVE,
};

// ============================================================================
// SIMULATION HARNESS FOR TAB RESOURCE LIFECYCLE & CRASH ORACLE
// ============================================================================

/// Tracks mock WebView2 controller allocations, active HWND/COM lifetimes,
/// and explicit `Close()` invocations.
#[derive(Clone, Default)]
pub struct ControllerTracker {
    pub active_views: Arc<Mutex<HashSet<u64>>>,
    pub close_counts: Arc<Mutex<HashMap<u64, usize>>>,
    pub reload_counts: Arc<Mutex<HashMap<u64, usize>>>,
    pub fail_reload_ids: Arc<Mutex<HashSet<u64>>>,
    pub fail_create_views: Arc<AtomicBool>,
    pub next_view_id: Arc<AtomicU64>,
}

impl ControllerTracker {
    pub fn new() -> Self {
        Self {
            active_views: Arc::new(Mutex::new(HashSet::new())),
            close_counts: Arc::new(Mutex::new(HashMap::new())),
            reload_counts: Arc::new(Mutex::new(HashMap::new())),
            fail_reload_ids: Arc::new(Mutex::new(HashSet::new())),
            fail_create_views: Arc::new(AtomicBool::new(false)),
            next_view_id: Arc::new(AtomicU64::new(1000)),
        }
    }

    pub fn allocate_view(&self) -> Result<MockWebView, String> {
        if self.fail_create_views.load(Ordering::SeqCst) {
            return Err("WebView2 runtime initialization failed: E_FAIL".into());
        }
        let id = self.next_view_id.fetch_add(1, Ordering::SeqCst);
        self.active_views.lock().unwrap().insert(id);
        Ok(MockWebView {
            id,
            tracker: self.clone(),
            loaded_url: Arc::new(Mutex::new(String::new())),
        })
    }

    pub fn record_close(&self, view_id: u64) {
        let mut active = self.active_views.lock().unwrap();
        active.remove(&view_id);
        let mut counts = self.close_counts.lock().unwrap();
        *counts.entry(view_id).or_insert(0) += 1;
    }

    pub fn record_reload(&self, view_id: u64) -> Result<(), String> {
        if self.fail_reload_ids.lock().unwrap().contains(&view_id) {
            return Err("COM channel disconnected: RPC_E_DISCONNECTED (0x80010108)".into());
        }
        let mut counts = self.reload_counts.lock().unwrap();
        *counts.entry(view_id).or_insert(0) += 1;
        Ok(())
    }

    pub fn live_view_count(&self) -> usize {
        self.active_views.lock().unwrap().len()
    }
}

/// Simulated WebView representing a wry::WebView with underlying COM controller.
#[derive(Clone)]
pub struct MockWebView {
    pub id: u64,
    pub tracker: ControllerTracker,
    pub loaded_url: Arc<Mutex<String>>,
}

impl MockWebView {
    pub fn load_url(&self, url: &str) -> Result<(), String> {
        *self.loaded_url.lock().unwrap() = url.to_string();
        Ok(())
    }

    pub fn load_html(&self, html: &str) -> Result<(), String> {
        *self.loaded_url.lock().unwrap() = format!("html:{}", html.len());
        Ok(())
    }

    pub fn reload(&self) -> Result<(), String> {
        self.tracker.record_reload(self.id)
    }

    pub fn close(&self) {
        self.tracker.record_close(self.id);
    }
}

/// Simulates the exact Tab struct in `papyrus/src/main.rs:47-65`
pub struct SimulatedTab {
    pub navigation: u64,
    pub info: TabInfo,
    pub view: Option<MockWebView>,
}

impl SimulatedTab {
    pub fn new(id: u64, url: &str, view: Option<MockWebView>) -> Self {
        Self {
            navigation: 1,
            info: TabInfo {
                id,
                title: format!("Tab {id}"),
                url: url.to_string(),
                loading: false,
                blocked: 0,
                error: None,
                pinned: false,
                muted: false,
                is_playing_audio: false,
            },
            view,
        }
    }

    /// Exact implementation from `papyrus/src/main.rs:54-58`
    pub fn close_view(&mut self) {
        if let Some(view) = self.view.take() {
            view.close();
        }
    }
}

/// Exact implementation from `papyrus/src/main.rs:61-65`
impl Drop for SimulatedTab {
    fn drop(&mut self) {
        self.close_view();
    }
}

/// State machine matching `Browser` in `src/main.rs`
pub struct SimulatedBrowser {
    pub tabs: Vec<SimulatedTab>,
    pub active: u64,
    pub next_tab_id: u64,
    pub closed_tabs: Vec<(String, String)>,
    pub tracker: ControllerTracker,
}

impl SimulatedBrowser {
    pub fn new(tracker: ControllerTracker) -> Self {
        let mut browser = Self {
            tabs: vec![],
            active: 0,
            next_tab_id: 1,
            closed_tabs: vec![],
            tracker,
        };
        browser.new_tab(NEW_TAB, true).unwrap();
        browser
    }

    pub fn new_tab(&mut self, url: &str, select: bool) -> Result<u64, String> {
        if self.tabs.len() >= 100 {
            return Err("Đã đạt giới hạn 100 tab. Hãy đóng bớt tab trước.".into());
        }
        let id = self.next_tab_id;
        self.next_tab_id += 1;

        let is_internal_no_view = [
            SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
        ]
        .contains(&url);

        let view = if is_internal_no_view {
            None
        } else {
            Some(self.tracker.allocate_view()?)
        };

        if let Some(ref v) = view {
            if url == NEW_TAB {
                let _ = v.load_html("<html>NewTab</html>");
            } else {
                let _ = v.load_url(url);
            }
        }

        let mut tab = SimulatedTab::new(id, url, view);
        tab.info.loading = !is_internal_no_view && url != NEW_TAB;
        tab.info.title = if url == SETTINGS {
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
            format!("Tab {id}")
        };

        self.tabs.push(tab);

        if select || self.active == 0 {
            self.active = id;
        }
        Ok(id)
    }

    pub fn record_closed_tab(&mut self, url: &str, title: &str) {
        if !url.starts_with("papyrus:") && url != "about:blank" && !url.is_empty() {
            self.closed_tabs.push((url.to_string(), title.to_string()));
            if self.closed_tabs.len() > 50 {
                self.closed_tabs.remove(0);
            }
        }
    }

    /// Exact implementation of `Browser::close_tab` from `src/main.rs:691-717`
    pub fn close_tab(&mut self, mut id: u64) -> Result<(), String> {
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
        // Dropping matching SimulatedTab calls close_view via Drop!
        self.tabs.retain(|t| t.info.id != id);
        if let Some(next) = next {
            self.active = next;
        } else {
            self.active = 0;
            self.new_tab(NEW_TAB, true)?;
        }
        Ok(())
    }

    /// Exact implementation of `Browser::navigate` from `src/main.rs:639-684`
    pub fn navigate(&mut self, url: &str) -> Result<(), String> {
        let index = self
            .tabs
            .iter()
            .position(|t| t.info.id == self.active)
            .ok_or("Không tìm thấy tab.")?;

        if [
            SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
        ]
        .contains(&url)
        {
            self.tabs[index].close_view();
            self.tabs[index].info.url = url.to_string();
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
                self.tabs[index].view = Some(self.tracker.allocate_view()?);
            }
            let view = self.tabs[index].view.as_ref().expect("created view");
            if url == NEW_TAB {
                view.load_html("<html>NewTab</html>")
                    .map_err(|e| format!("Không thể mở tab mới: {e}"))?;
                self.tabs[index].info.title = "Tab mới".into();
                self.tabs[index].info.url = NEW_TAB.into();
                self.tabs[index].info.loading = false;
                self.tabs[index].info.error = None;
            } else {
                view.load_url(url)
                    .map_err(|e| format!("Không thể điều hướng đến {url}: {e}"))?;
                self.tabs[index].info.url = url.to_string();
                self.tabs[index].info.loading = true;
                self.tabs[index].info.error = None;
            }
        }
        Ok(())
    }

    /// Exact implementation of `EventMessage::ProcessFailed` handling from `src/main.rs:1613-1619`
    pub fn on_process_failed(&mut self, id: u64, _kind: i32, err: String) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
            tab.info.loading = false;
            tab.info.error = Some(err);
        }
    }

    /// Exact implementation of `Command::Reload` from `src/main.rs:950-989`
    pub fn reload_active(&mut self) -> Result<(), String> {
        let active_id = self.active;
        let mut navigate_fallback = None;
        let mut recreate_info = None;

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == active_id) {
            tab.info.error = None;
            tab.info.loading = true;
            if let Some(view) = &tab.view {
                if let Err(e) = view.reload() {
                    eprintln!("[Papyrus] Reload failed ({e}), recreating view for tab {active_id}");
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
            match self.tracker.allocate_view() {
                Ok(new_view) => {
                    let _ = new_view.load_url(&url);
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                        tab.view = Some(new_view);
                    }
                }
                Err(err) => {
                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                        tab.info.error = Some(format!("Không thể khởi động lại trang: {err}"));
                        tab.info.loading = false;
                    }
                }
            }
        }
        Ok(())
    }

    /// Exact implementation of context menu reload (action 1) from `src/main.rs:1360-1386`
    pub fn context_menu_reload(&mut self, id: u64) -> Result<(), String> {
        let mut recreate_info = None;
        let mut navigate_fallback = None;

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
            tab.info.error = None;
            tab.info.loading = true;
            if let Some(view) = &tab.view {
                if let Err(e) = view.reload() {
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
            && let Ok(new_view) = self.tracker.allocate_view()
        {
            let _ = new_view.load_url(&url);
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == tab_id) {
                tab.view = Some(new_view);
            }
        }
        Ok(())
    }

    /// Exact implementation of Action 4 (Close other tabs) from `src/main.rs:1395-1411`
    pub fn close_other_tabs(&mut self, id: u64) {
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
    }

    /// Exact implementation of Action 5 (Close tabs to right) from `src/main.rs:1412-1439`
    pub fn close_tabs_to_right(&mut self, id: u64) {
        if let Some(pos) = self.tabs.iter().position(|t| t.info.id == id) {
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
        }
    }

    pub fn pin_tab(&mut self, id: u64) {
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
        }
    }

    pub fn reset_data(&mut self) {
        self.tabs.clear();
        let _ = self.new_tab(NEW_TAB, true);
    }
}

// ============================================================================
// SUITE 1: TAB LIFECYCLE RESOURCE TEARDOWN & IDEMPOTENCY ORACLE
// ============================================================================

#[test]
fn test_tab_close_view_idempotency_and_drop_safety() {
    let tracker = ControllerTracker::new();
    let view = tracker.allocate_view().unwrap();
    let view_id = view.id;

    assert_eq!(tracker.live_view_count(), 1);

    let mut tab = SimulatedTab::new(1, "https://example.com", Some(view));

    // 1. Explicit close_view
    tab.close_view();
    assert!(tab.view.is_none());
    assert_eq!(tracker.live_view_count(), 0);
    assert_eq!(
        *tracker.close_counts.lock().unwrap().get(&view_id).unwrap(),
        1
    );

    // 2. Repeated calls to close_view must be safe and idempotent
    tab.close_view();
    tab.close_view();
    assert_eq!(
        *tracker.close_counts.lock().unwrap().get(&view_id).unwrap(),
        1
    );

    // 3. Dropping the tab must not call close again
    drop(tab);
    assert_eq!(
        *tracker.close_counts.lock().unwrap().get(&view_id).unwrap(),
        1
    );
}

#[test]
fn test_tab_implicit_drop_closes_view() {
    let tracker = ControllerTracker::new();
    let view = tracker.allocate_view().unwrap();
    let view_id = view.id;

    assert_eq!(tracker.live_view_count(), 1);

    {
        let _tab = SimulatedTab::new(2, "https://rust-lang.org", Some(view));
        assert_eq!(tracker.live_view_count(), 1);
        // _tab goes out of scope and drops here
    }

    assert_eq!(tracker.live_view_count(), 0);
    assert_eq!(
        *tracker.close_counts.lock().unwrap().get(&view_id).unwrap(),
        1
    );
}

#[test]
fn test_rapid_tab_creation_and_teardown_zero_leaks() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    // 1 initial tab
    assert_eq!(browser.tabs.len(), 1);
    assert_eq!(tracker.live_view_count(), 1);

    // Open 99 more tabs (reaching limit of 100)
    let mut opened_ids = vec![];
    for i in 2..=100 {
        let url = format!("https://site-{i}.example.com");
        let id = browser
            .new_tab(&url, false)
            .expect("Must open up to 100 tabs");
        opened_ids.push(id);
    }
    assert_eq!(browser.tabs.len(), 100);
    assert_eq!(tracker.live_view_count(), 100);

    // Close tabs in reverse order (LIFO) down to 1
    for id in opened_ids.into_iter().rev() {
        browser.close_tab(id).expect("Close tab must succeed");
    }

    assert_eq!(browser.tabs.len(), 1);
    assert_eq!(tracker.live_view_count(), 1);

    // Close the last remaining tab (triggers automatic spawn of 1 newtab)
    let last_id = browser.tabs[0].info.id;
    browser
        .close_tab(last_id)
        .expect("Closing last tab must auto-spawn newtab");

    assert_eq!(browser.tabs.len(), 1);
    assert_eq!(tracker.live_view_count(), 1);

    // Verify all 100 closed views received close() exactly once
    let counts = tracker.close_counts.lock().unwrap();
    assert_eq!(counts.len(), 100);
    for (view_id, count) in counts.iter() {
        assert_eq!(*count, 1, "View {view_id} must be closed exactly once");
    }
}

#[test]
fn test_100_tab_limit_boundary_enforcement() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    for i in 2..=100 {
        let id = browser.new_tab(&format!("https://test{i}.com"), false);
        assert!(id.is_ok());
    }
    assert_eq!(browser.tabs.len(), 100);
    assert_eq!(tracker.live_view_count(), 100);

    // Tab 101, 102, 103 must be rejected without allocating orphan views
    let live_before = tracker.live_view_count();
    for _ in 0..5 {
        let res = browser.new_tab("https://rejected.com", false);
        assert!(res.is_err(), "Must reject tabs exceeding limit 100");
    }

    assert_eq!(
        tracker.live_view_count(),
        live_before,
        "No orphan views allocated"
    );
    assert_eq!(browser.tabs.len(), 100);
}

// ============================================================================
// SUITE 2: INTERNAL URL CONTROLLER TEARDOWN INVARIANTS (papyrus://*)
// ============================================================================

#[test]
fn test_internal_urls_controller_teardown_all_schemes() {
    let internal_urls = [
        (SETTINGS, "Cài đặt"),
        (HISTORY, "Lịch sử"),
        (BOOKMARKS, "Dấu trang"),
        (DOWNLOADS, "Tệp tải xuống"),
        (PASSWORDS, "Mật khẩu"),
        (EXTENSIONS, "Kho tiện ích"),
    ];

    for (url, expected_title) in internal_urls {
        let tracker = ControllerTracker::new();
        let mut browser = SimulatedBrowser::new(tracker.clone());

        // 1. Navigate to external page
        browser.navigate("https://example.com").unwrap();
        assert_eq!(tracker.live_view_count(), 1);
        let external_view_id = browser.tabs[0].view.as_ref().unwrap().id;

        // 2. Navigate to internal URL -> Must trigger close_view()
        browser.navigate(url).unwrap();

        assert!(
            browser.tabs[0].view.is_none(),
            "Tab on {url} must not retain an active controller"
        );
        assert_eq!(
            tracker.live_view_count(),
            0,
            "Internal page {url} must release controller immediately"
        );
        assert_eq!(
            *tracker
                .close_counts
                .lock()
                .unwrap()
                .get(&external_view_id)
                .unwrap(),
            1,
            "Previous controller must be closed"
        );
        assert_eq!(browser.tabs[0].info.url, url);
        assert_eq!(browser.tabs[0].info.title, expected_title);
        assert!(
            !browser.tabs[0].info.loading,
            "Internal pages are not loading"
        );
        assert!(
            browser.tabs[0].info.error.is_none(),
            "Error must be cleared"
        );

        // 3. Navigate back to external page -> Must re-initialize view
        browser.navigate("https://news.ycombinator.com").unwrap();
        assert!(
            browser.tabs[0].view.is_some(),
            "Returning to web page must recreate view"
        );
        assert_eq!(tracker.live_view_count(), 1);
        assert!(
            browser.tabs[0].info.loading,
            "Web page navigation must set loading=true"
        );
        assert_eq!(browser.tabs[0].info.url, "https://news.ycombinator.com");
    }
}

#[test]
fn test_internal_url_to_internal_url_transitions_are_noops_on_view() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    // Settings -> History -> Bookmarks -> Passwords
    browser.navigate(SETTINGS).unwrap();
    assert!(browser.tabs[0].view.is_none());
    assert_eq!(tracker.live_view_count(), 0);

    browser.navigate(HISTORY).unwrap();
    assert!(browser.tabs[0].view.is_none());
    assert_eq!(tracker.live_view_count(), 0);
    assert_eq!(browser.tabs[0].info.title, "Lịch sử");

    browser.navigate(BOOKMARKS).unwrap();
    assert!(browser.tabs[0].view.is_none());
    assert_eq!(tracker.live_view_count(), 0);
    assert_eq!(browser.tabs[0].info.title, "Dấu trang");

    browser.navigate(PASSWORDS).unwrap();
    assert!(browser.tabs[0].view.is_none());
    assert_eq!(tracker.live_view_count(), 0);
    assert_eq!(browser.tabs[0].info.title, "Mật khẩu");
}

#[test]
fn test_papyrus_newtab_vs_internal_pages_distinction() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    // NEW_TAB has an active WebView
    assert_eq!(browser.tabs[0].info.url, NEW_TAB);
    assert!(browser.tabs[0].view.is_some());
    assert_eq!(tracker.live_view_count(), 1);

    // Switch to SETTINGS: tears down view
    browser.navigate(SETTINGS).unwrap();
    assert!(browser.tabs[0].view.is_none());
    assert_eq!(tracker.live_view_count(), 0);

    // Switch back to NEW_TAB: recreates view
    browser.navigate(NEW_TAB).unwrap();
    assert!(browser.tabs[0].view.is_some());
    assert_eq!(tracker.live_view_count(), 1);
    assert_eq!(browser.tabs[0].info.title, "Tab mới");
    assert!(!browser.tabs[0].info.loading);
}

#[test]
fn test_ping_pong_internal_external_navigation_fuzzing() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    for i in 0..200 {
        if i % 2 == 0 {
            browser.navigate(&format!("https://page-{i}.com")).unwrap();
            assert!(browser.tabs[0].view.is_some());
            assert_eq!(tracker.live_view_count(), 1);
        } else {
            let internal = match i % 6 {
                1 => SETTINGS,
                3 => HISTORY,
                5 => BOOKMARKS,
                _ => DOWNLOADS,
            };
            browser.navigate(internal).unwrap();
            assert!(browser.tabs[0].view.is_none());
            assert_eq!(tracker.live_view_count(), 0);
        }
    }
}

// ============================================================================
// SUITE 3: CRASH SIMULATION (ProcessFailed) & EVENT LOOP ORACLE
// ============================================================================

fn classify_process_failed_kind(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> (bool, &'static str) {
    match kind {
        COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED => {
            (true, "Tiến trình WebView2 đã kết thúc đột ngột.")
        }
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED => (
            true,
            "Tiến trình hiển thị trang web đã bị dừng đột ngột (Crash).",
        ),
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE => {
            (true, "Trang web không phản hồi (Unresponsive).")
        }
        COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED => {
            (true, "Khung hiển thị (iframe) của trang web bị lỗi dừng.")
        }
        COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED => (
            false,
            "Tiến trình đồ họa (GPU) gặp sự cố và đang tự khôi phục.",
        ),
        COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED => {
            (false, "Tiến trình tiện ích WebView2 gặp sự cố.")
        }
        _ => (true, "Tiến trình nội bộ WebView2 gặp sự cố không xác định."),
    }
}

fn format_process_failed_reason(reason: COREWEBVIEW2_PROCESS_FAILED_REASON) -> &'static str {
    match reason {
        COREWEBVIEW2_PROCESS_FAILED_REASON_CRASHED => "Sự cố crash",
        COREWEBVIEW2_PROCESS_FAILED_REASON_OUT_OF_MEMORY => "Hết bộ nhớ (OOM)",
        COREWEBVIEW2_PROCESS_FAILED_REASON_TERMINATED => "Bị dừng bởi hệ thống",
        COREWEBVIEW2_PROCESS_FAILED_REASON_UNRESPONSIVE => "Treo / Không phản hồi",
        _ => "Khác",
    }
}

#[test]
fn test_crash_simulation_process_failed_fatal_kinds() {
    let fatal_kinds = [
        (
            COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
            "kết thúc đột ngột",
        ),
        (
            COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
            "Crash",
        ),
        (
            COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE,
            "Unresponsive",
        ),
        (
            COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
            "iframe",
        ),
        (
            COREWEBVIEW2_PROCESS_FAILED_KIND_UNKNOWN_PROCESS_EXITED,
            "không xác định",
        ),
    ];

    for (kind, snippet) in fatal_kinds {
        let (is_fatal, desc) = classify_process_failed_kind(kind);
        assert!(is_fatal, "Kind {:?} must be classified fatal", kind.0);
        assert!(desc.contains(snippet));

        let tracker = ControllerTracker::new();
        let mut browser = SimulatedBrowser::new(tracker);

        let id = browser.new_tab("https://heavy-site.com", true).unwrap();
        assert!(browser.tabs[1].info.loading);
        assert!(browser.tabs[1].info.error.is_none());

        // Simulate fatal crash event
        let user_msg = format!("{desc} Nhấn Tải lại để khôi phục.");
        browser.on_process_failed(id, kind.0, user_msg.clone());

        let tab = browser.tabs.iter().find(|t| t.info.id == id).unwrap();
        assert!(!tab.info.loading, "Crash event must clear loading flag");
        assert_eq!(
            tab.info.error,
            Some(user_msg),
            "Error banner must be populated"
        );
    }
}

#[test]
fn test_crash_simulation_non_fatal_kinds_gpu_utility() {
    let non_fatal = [
        COREWEBVIEW2_PROCESS_FAILED_KIND_GPU_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_UTILITY_PROCESS_EXITED,
    ];

    for kind in non_fatal {
        let (is_fatal, desc) = classify_process_failed_kind(kind);
        assert!(
            !is_fatal,
            "GPU/Utility crashes must NOT be fatal to the user"
        );
        assert!(desc.contains("GPU") || desc.contains("tiện ích"));
    }
}

#[test]
fn test_crash_event_for_already_closed_tab_is_safe_noop() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker);

    let id = browser.new_tab("https://transient.com", true).unwrap();
    browser.close_tab(id).unwrap();

    // Event arrives delayed from background thread for closed tab
    browser.on_process_failed(id, 2, "Late crash message".into());

    // Must not panic, must not corrupt browser state
    assert_eq!(browser.tabs.len(), 1);
    assert_eq!(browser.tabs[0].info.error, None);
}

#[test]
fn test_crash_reasons_formatting_completeness() {
    assert_eq!(
        format_process_failed_reason(COREWEBVIEW2_PROCESS_FAILED_REASON_CRASHED),
        "Sự cố crash"
    );
    assert_eq!(
        format_process_failed_reason(COREWEBVIEW2_PROCESS_FAILED_REASON_OUT_OF_MEMORY),
        "Hết bộ nhớ (OOM)"
    );
    assert_eq!(
        format_process_failed_reason(COREWEBVIEW2_PROCESS_FAILED_REASON_TERMINATED),
        "Bị dừng bởi hệ thống"
    );
    assert_eq!(
        format_process_failed_reason(COREWEBVIEW2_PROCESS_FAILED_REASON_UNRESPONSIVE),
        "Treo / Không phản hồi"
    );
    assert_eq!(
        format_process_failed_reason(COREWEBVIEW2_PROCESS_FAILED_REASON(999)),
        "Khác"
    );
}

// ============================================================================
// SUITE 4: SELF-HEALING RELOAD STATE MACHINE & RECOVERY INVARIANTS
// ============================================================================

#[test]
fn test_self_healing_reload_healthy_controller() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    browser.navigate("https://healthy.com").unwrap();
    let initial_view_id = browser.tabs[0].view.as_ref().unwrap().id;

    // Normal reload
    browser.reload_active().unwrap();

    assert_eq!(
        browser.tabs[0].view.as_ref().unwrap().id,
        initial_view_id,
        "Healthy controller is reused without recreation"
    );
    assert_eq!(
        *tracker
            .reload_counts
            .lock()
            .unwrap()
            .get(&initial_view_id)
            .unwrap(),
        1
    );
    assert!(browser.tabs[0].info.loading);
    assert!(browser.tabs[0].info.error.is_none());
}

#[test]
fn test_self_healing_reload_dead_controller_recreation() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    browser.navigate("https://flaky-site.org").unwrap();
    let dead_view_id = browser.tabs[0].view.as_ref().unwrap().id;

    // Simulate crash
    browser.on_process_failed(browser.active, 2, "Renderer crashed!".into());
    assert_eq!(browser.tabs[0].info.error, Some("Renderer crashed!".into()));
    assert!(!browser.tabs[0].info.loading);

    // Mark the dead view controller as failing reload calls (e.g. RPC disconnected)
    tracker.fail_reload_ids.lock().unwrap().insert(dead_view_id);

    // User triggers reload
    browser.reload_active().unwrap();

    // 1. Old dead view controller must be closed
    assert_eq!(
        *tracker
            .close_counts
            .lock()
            .unwrap()
            .get(&dead_view_id)
            .unwrap(),
        1,
        "Old dead view controller must be closed upon failed reload"
    );

    // 2. A new view controller must be allocated
    let new_view_id = browser.tabs[0].view.as_ref().unwrap().id;
    assert_ne!(new_view_id, dead_view_id, "New view must have a new id");
    assert_eq!(tracker.live_view_count(), 1, "Exactly 1 live view remains");

    // 3. Tab state recovered
    assert_eq!(browser.tabs[0].info.url, "https://flaky-site.org");
    assert!(browser.tabs[0].info.loading, "Tab must be loading");
    assert!(
        browser.tabs[0].info.error.is_none(),
        "Error must be cleared"
    );
}

#[test]
fn test_self_healing_reload_unrecoverable_creation_failure() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    browser.navigate("https://doomed.com").unwrap();
    let dead_view_id = browser.tabs[0].view.as_ref().unwrap().id;
    tracker.fail_reload_ids.lock().unwrap().insert(dead_view_id);

    // Simulate OS failure where new views cannot be created (OOM / broken runtime)
    tracker.fail_create_views.store(true, Ordering::SeqCst);

    // Reload fails both on reload and recreate
    browser.reload_active().unwrap();

    let tab = &browser.tabs[0];
    assert!(
        tab.view.is_none(),
        "Failed view creation leaves view as None"
    );
    assert!(
        !tab.info.loading,
        "Loading must be false on unrecoverable failure"
    );
    assert!(
        tab.info
            .error
            .as_ref()
            .unwrap()
            .contains("Không thể khởi động lại trang"),
        "Clear error message presented to user"
    );
}

#[test]
fn test_self_healing_reload_on_internal_page_is_safe_navigate() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    browser.navigate(SETTINGS).unwrap();
    assert!(browser.tabs[0].view.is_none());

    // Reloading an internal page (where tab.view is None)
    browser.reload_active().unwrap();

    assert!(
        browser.tabs[0].view.is_none(),
        "View remains None for internal page"
    );
    assert_eq!(browser.tabs[0].info.url, SETTINGS);
    assert_eq!(browser.tabs[0].info.title, "Cài đặt");
    assert!(!browser.tabs[0].info.loading);
}

#[test]
fn test_context_menu_reload_background_crashed_tab() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    let bg_id = browser.new_tab("https://background.org", false).unwrap();
    assert_eq!(browser.active, 1); // active tab is tab 1

    let bg_view_id = browser.tabs[1].view.as_ref().unwrap().id;
    tracker.fail_reload_ids.lock().unwrap().insert(bg_view_id);

    // Simulate crash on background tab
    browser.on_process_failed(bg_id, 2, "Background tab crash".into());

    // Right-click reload on background tab
    browser.context_menu_reload(bg_id).unwrap();

    // Verify background tab self-healed without shifting active tab
    assert_eq!(browser.active, 1, "Active tab must not change");
    let new_bg_view_id = browser.tabs[1].view.as_ref().unwrap().id;
    assert_ne!(new_bg_view_id, bg_view_id, "Background view recreated");
    assert_eq!(
        *tracker
            .close_counts
            .lock()
            .unwrap()
            .get(&bg_view_id)
            .unwrap(),
        1,
        "Dead background view closed"
    );
    assert_eq!(tracker.live_view_count(), 2);
}

// ============================================================================
// SUITE 5: BATCH TAB CLOSURES & CONTROLLER TEARDOWN INTEGRITY
// ============================================================================

#[test]
fn test_batch_close_others_controller_cleanup() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    // Create 20 tabs: 5 pinned, 15 unpinned
    for i in 2..=20 {
        let _ = browser.new_tab(&format!("https://tab{i}.com"), false);
    }
    for i in 1..=5 {
        browser.pin_tab(i);
    }

    assert_eq!(browser.tabs.len(), 20);
    assert_eq!(tracker.live_view_count(), 20);

    // Choose unpinned tab 10 and close others
    browser.close_other_tabs(10);

    // Expected: 5 pinned tabs + tab 10 remain (total 6 tabs)
    assert_eq!(browser.tabs.len(), 6);
    assert_eq!(tracker.live_view_count(), 6);

    // Exactly 14 controllers were closed
    assert_eq!(tracker.close_counts.lock().unwrap().len(), 14);
    for count in tracker.close_counts.lock().unwrap().values() {
        assert_eq!(*count, 1, "Each closed tab controller closed exactly once");
    }
}

#[test]
fn test_batch_close_tabs_to_right_controller_cleanup() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    for i in 2..=20 {
        let _ = browser.new_tab(&format!("https://tab{i}.com"), false);
    }

    // Tab 15 is unpinned; tabs 16-20 are to its right (5 tabs)
    browser.close_tabs_to_right(15);

    assert_eq!(browser.tabs.len(), 15);
    assert_eq!(tracker.live_view_count(), 15);

    // 5 controllers closed
    assert_eq!(tracker.close_counts.lock().unwrap().len(), 5);
}

#[test]
fn test_reset_data_clear_all_tabs_controller_cleanup() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    for i in 2..=30 {
        let _ = browser.new_tab(&format!("https://data{i}.org"), false);
    }
    assert_eq!(tracker.live_view_count(), 30);

    // Reset data clears all tabs and spawns 1 new tab
    browser.reset_data();

    assert_eq!(browser.tabs.len(), 1);
    assert_eq!(tracker.live_view_count(), 1);
    assert_eq!(tracker.close_counts.lock().unwrap().len(), 30);
    for count in tracker.close_counts.lock().unwrap().values() {
        assert_eq!(*count, 1);
    }
}

// ============================================================================
// SUITE 6: ADVERSARIAL STRESS & CONCURRENT CRASH FUZZING (10,000 OPS)
// ============================================================================

#[test]
fn test_concurrency_stress_tab_lifecycle_threads() {
    let tracker = ControllerTracker::new();
    let browser_mutex = Arc::new(Mutex::new(SimulatedBrowser::new(tracker.clone())));

    let mut handles = vec![];
    for t in 0..8 {
        let b = browser_mutex.clone();
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let mut guard = b.lock().unwrap();
                let op = (t * 100 + i) % 5;
                match op {
                    0 => {
                        let _ = guard.new_tab(&format!("https://thread-{t}-{i}.com"), false);
                    }
                    1 => {
                        let _ = guard.navigate("https://thread-nav.com");
                    }
                    2 => {
                        if guard.tabs.len() > 2 {
                            let victim_id = guard.tabs[1].info.id;
                            let _ = guard.close_tab(victim_id);
                        }
                    }
                    3 => {
                        let _ = guard.navigate(SETTINGS);
                    }
                    _ => {
                        let _ = guard.reload_active();
                    }
                }
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let guard = browser_mutex.lock().unwrap();
    // Invariant: live views must equal number of tabs with Some(view)
    let expected_views = guard.tabs.iter().filter(|t| t.view.is_some()).count();
    assert_eq!(tracker.live_view_count(), expected_views);
}

#[test]
fn test_adversarial_fuzzing_crash_storm_and_recovery_10000_ops() {
    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    // Deterministic pseudo-random LCG
    let mut rng: u64 = 0x00C0_FFEE_FEED_F00D;
    let mut next_rand = || {
        rng = rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        rng
    };

    for step in 0..10_000 {
        let action = next_rand() % 9;

        match action {
            0 => {
                // NewTab
                let url = format!("https://rand-{}.org", next_rand() % 500);
                let _ = browser.new_tab(&url, next_rand() % 2 == 0);
            }
            1 => {
                // Navigate web
                let url = format!("https://nav-{}.net", next_rand() % 1000);
                let _ = browser.navigate(&url);
            }
            2 => {
                // Navigate internal
                let internals = [
                    SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
                ];
                let chosen = internals[(next_rand() as usize) % internals.len()];
                let _ = browser.navigate(chosen);
            }
            3 => {
                // Close tab
                if !browser.tabs.is_empty() {
                    let idx = (next_rand() as usize) % browser.tabs.len();
                    let id = browser.tabs[idx].info.id;
                    let _ = browser.close_tab(id);
                }
            }
            4 => {
                // Simulate crash (ProcessFailed) on random tab or random id
                let target_id = if !browser.tabs.is_empty() && next_rand() % 3 != 0 {
                    let idx = (next_rand() as usize) % browser.tabs.len();
                    browser.tabs[idx].info.id
                } else {
                    next_rand() % 10000 // possibly non-existent
                };
                browser.on_process_failed(target_id, 2, "Sudden crash".into());
            }
            5 => {
                // Reload active tab (healthy or broken)
                if next_rand() % 5 == 0 {
                    // Mark active view as failing reload to force self-healing recreation
                    if let Some(tab) = browser.tabs.iter().find(|t| t.info.id == browser.active)
                        && let Some(view) = &tab.view
                    {
                        tracker.fail_reload_ids.lock().unwrap().insert(view.id);
                    }
                }
                let _ = browser.reload_active();
            }
            6 => {
                // Close other tabs
                if !browser.tabs.is_empty() {
                    let idx = (next_rand() as usize) % browser.tabs.len();
                    let id = browser.tabs[idx].info.id;
                    browser.close_other_tabs(id);
                }
            }
            7 => {
                // Close tabs to right
                if !browser.tabs.is_empty() {
                    let idx = (next_rand() as usize) % browser.tabs.len();
                    let id = browser.tabs[idx].info.id;
                    browser.close_tabs_to_right(id);
                }
            }
            _ => {
                // Pin/unpin tab
                if !browser.tabs.is_empty() {
                    let idx = (next_rand() as usize) % browser.tabs.len();
                    let id = browser.tabs[idx].info.id;
                    browser.pin_tab(id);
                }
            }
        }

        // Invariant checks every 500 steps
        if step % 500 == 0 {
            // 1. Tab count within limits
            assert!(browser.tabs.len() <= 100, "Tab count <= 100");
            assert!(!browser.tabs.is_empty(), "Never 0 tabs");

            // 2. Active view count matches tabs with views
            let expected_views = browser.tabs.iter().filter(|t| t.view.is_some()).count();
            assert_eq!(
                tracker.live_view_count(),
                expected_views,
                "Invariant: Live views must strictly match tabs with views at step {step}"
            );

            // 3. All internal tabs have no view
            for t in &browser.tabs {
                if [
                    SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
                ]
                .contains(&t.info.url.as_str())
                {
                    assert!(
                        t.view.is_none(),
                        "Internal page {} must have no view",
                        t.info.url
                    );
                }
            }

            // 4. Active tab exists
            assert!(
                browser.tabs.iter().any(|t| t.info.id == browser.active),
                "Active tab must exist"
            );

            // 5. Pinned tabs partition
            let pinned_count = browser.tabs.iter().take_while(|t| t.info.pinned).count();
            let total_pinned = browser.tabs.iter().filter(|t| t.info.pinned).count();
            assert_eq!(
                pinned_count, total_pinned,
                "Pinned tabs partition invariant must hold at step {step}"
            );
        }
    }
}

// ============================================================================
// SUITE 7: ADVERSARIAL CRITIC AUDIT: CONTEXT MENU RELOAD VS COMMAND::RELOAD
// ============================================================================

#[test]
fn test_critic_audit_context_menu_reload_creation_failure_divergence() {
    // This test formally validates the behavioral difference between Command::Reload
    // (src/main.rs:979-985) and Context Menu Reload (src/main.rs:1379-1386)
    // when allocate_view (create_view) fails.

    let tracker = ControllerTracker::new();
    let mut browser = SimulatedBrowser::new(tracker.clone());

    let id = browser.new_tab("https://audit-target.com", true).unwrap();
    let view_id = browser.tabs[1].view.as_ref().unwrap().id;

    // Simulate dead view controller + OS creation failure
    tracker.fail_reload_ids.lock().unwrap().insert(view_id);
    tracker.fail_create_views.store(true, Ordering::SeqCst);

    // Call context_menu_reload
    browser.context_menu_reload(id).unwrap();

    let tab = browser.tabs.iter().find(|t| t.info.id == id).unwrap();

    // Observation: tab.view was closed via close_view()
    assert!(tab.view.is_none(), "Dead view was torn down");
    assert_eq!(
        *tracker.close_counts.lock().unwrap().get(&view_id).unwrap(),
        1,
        "Closed dead view"
    );

    // Note: In context_menu_reload (main.rs:1365), tab.info.loading is set to true.
    // If create_view fails, Command::Reload resets loading=false and sets error=Some(...),
    // whereas Context Menu reload skips without error assignment because of `&& let Ok(new_view)`.
    // Both survive safely without crashing, but Command::Reload provides user feedback.
}
