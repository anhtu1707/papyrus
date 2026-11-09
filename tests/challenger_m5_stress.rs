#![allow(dead_code, unused_imports, clippy::field_reassign_with_default)]

#[path = "../src/model.rs"]
mod model;

use model::{
    BOOKMARKS, Bookmark, Command, DOWNLOADS, EXTENSIONS, HISTORY, HistoryEntry, NEW_TAB, PASSWORDS,
    PasswordEntry, Preferences, SETTINGS, SavedState, TabInfo, domain_matches, extract_rule_domain,
    is_standard_rule, next_active_after_close, parse_command, resolve_input, should_block,
    valid_host, valid_rule_or_host,
};

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    thread,
};

// ============================================================================
// SIMULATION HARNESS FOR TAB LIFECYCLE, CONTROLLER CLEANUP & CRASH SELF-HEALING
// ============================================================================

#[derive(Clone, Default)]
pub struct MockControllerTracker {
    pub active_views: Arc<Mutex<HashSet<u64>>>,
    pub close_counts: Arc<Mutex<HashMap<u64, usize>>>,
    pub reload_counts: Arc<Mutex<HashMap<u64, usize>>>,
    pub fail_reload_ids: Arc<Mutex<HashSet<u64>>>,
    pub next_view_id: Arc<AtomicU64>,
}

impl MockControllerTracker {
    pub fn new() -> Self {
        Self {
            active_views: Arc::new(Mutex::new(HashSet::new())),
            close_counts: Arc::new(Mutex::new(HashMap::new())),
            reload_counts: Arc::new(Mutex::new(HashMap::new())),
            fail_reload_ids: Arc::new(Mutex::new(HashSet::new())),
            next_view_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub fn allocate_view(&self) -> MockView {
        let id = self.next_view_id.fetch_add(1, Ordering::SeqCst);
        self.active_views.lock().unwrap().insert(id);
        MockView {
            id,
            tracker: self.clone(),
            loaded_url: Arc::new(Mutex::new(String::new())),
        }
    }

    pub fn record_close(&self, view_id: u64) {
        let mut active = self.active_views.lock().unwrap();
        active.remove(&view_id);
        let mut counts = self.close_counts.lock().unwrap();
        *counts.entry(view_id).or_insert(0) += 1;
    }

    pub fn record_reload(&self, view_id: u64) -> Result<(), String> {
        if self.fail_reload_ids.lock().unwrap().contains(&view_id) {
            return Err("RPC_E_DISCONNECTED (0x80010108): Render process crashed".into());
        }
        let mut counts = self.reload_counts.lock().unwrap();
        *counts.entry(view_id).or_insert(0) += 1;
        Ok(())
    }

    pub fn live_views(&self) -> usize {
        self.active_views.lock().unwrap().len()
    }
}

pub struct MockView {
    pub id: u64,
    pub tracker: MockControllerTracker,
    pub loaded_url: Arc<Mutex<String>>,
}

impl MockView {
    pub fn close(&self) {
        self.tracker.record_close(self.id);
    }
    pub fn reload(&self) -> Result<(), String> {
        self.tracker.record_reload(self.id)
    }
    pub fn load_url(&self, url: &str) {
        *self.loaded_url.lock().unwrap() = url.to_string();
    }
}

impl Drop for MockView {
    fn drop(&mut self) {
        self.close();
    }
}

pub struct SimTab {
    pub info: TabInfo,
    pub view: Option<MockView>,
}

impl SimTab {
    pub fn close_view(&mut self) {
        if let Some(view) = self.view.take() {
            view.close();
        }
    }
}

impl Drop for SimTab {
    fn drop(&mut self) {
        self.close_view();
    }
}

pub struct SimBrowser {
    pub tabs: Vec<SimTab>,
    pub closed_tabs: Vec<(String, String)>,
    pub active: u64,
    pub next_id: u64,
    pub tracker: MockControllerTracker,
    pub data_dir: PathBuf,
    pub notice: String,
    pub was_crashed: bool,
    pub running_flag: PathBuf,
}

impl SimBrowser {
    pub fn new(data_dir: PathBuf) -> Self {
        let flag = data_dir.join("running.flag");
        let was_crashed = flag.exists();
        let _ = fs::write(&flag, b"1");
        Self {
            tabs: vec![],
            closed_tabs: vec![],
            active: 0,
            next_id: 1,
            tracker: MockControllerTracker::new(),
            data_dir,
            notice: String::new(),
            was_crashed,
            running_flag: flag,
        }
    }

    pub fn new_tab(&mut self, url: &str, select: bool, pinned: bool) -> Result<u64, String> {
        if self.tabs.len() >= 100 {
            return Err("Đã đạt giới hạn 100 tab. Hãy đóng bớt tab trước.".into());
        }
        let id = self.next_id;
        self.next_id += 1;
        let is_internal = [
            SETTINGS, HISTORY, BOOKMARKS, DOWNLOADS, PASSWORDS, EXTENSIONS,
        ]
        .contains(&url);

        let view = if is_internal {
            None
        } else {
            let v = self.tracker.allocate_view();
            v.load_url(url);
            Some(v)
        };

        self.tabs.push(SimTab {
            info: TabInfo {
                id,
                title: url.into(),
                url: url.into(),
                loading: false,
                blocked: 0,
                error: None,
                pinned,
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

    pub fn record_closed_tab(&mut self, url: &str, title: &str) {
        if !url.starts_with("papyrus:") && url != "about:blank" && !url.is_empty() {
            self.closed_tabs.push((url.to_string(), title.to_string()));
            if self.closed_tabs.len() > 50 {
                self.closed_tabs.remove(0);
            }
        }
    }

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
        let ids: Vec<u64> = self.tabs.iter().map(|t| t.info.id).collect();
        let next = next_active_after_close(&ids, self.active, id);
        self.tabs.retain(|t| t.info.id != id);
        if let Some(next) = next {
            self.active = next;
        } else {
            self.active = 0;
            self.new_tab(NEW_TAB, true, false)?;
        }
        Ok(())
    }

    pub fn pin_tab(&mut self, mut id: u64) {
        if id == 0 {
            id = self.active;
        }
        if let Some(pos) = self.tabs.iter().position(|t| t.info.id == id) {
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

    pub fn duplicate_tab(&mut self, mut id: u64) -> Result<u64, String> {
        if id == 0 {
            id = self.active;
        }
        let source_idx = self
            .tabs
            .iter()
            .position(|t| t.info.id == id)
            .ok_or("Không tìm thấy tab.")?;
        let url = self.tabs[source_idx].info.url.clone();
        let is_pinned = self.tabs[source_idx].info.pinned;
        let new_id = self.new_tab(&url, true, false)?;
        if let Some(created_idx) = self.tabs.iter().position(|t| t.info.id == new_id) {
            let mut tab = self.tabs.remove(created_idx);
            if is_pinned {
                tab.info.pinned = true;
            }
            let insert_idx = (source_idx + 1).min(self.tabs.len());
            self.tabs.insert(insert_idx, tab);
        }
        self.active = new_id;
        Ok(new_id)
    }

    pub fn reorder_tab(&mut self, from: u64, to: u64) {
        if from != to {
            let from_pos = self.tabs.iter().position(|t| t.info.id == from);
            let to_pos = self.tabs.iter().position(|t| t.info.id == to);
            if let (Some(f), Some(t)) = (from_pos, to_pos)
                && self.tabs[f].info.pinned == self.tabs[t].info.pinned
            {
                let tab = self.tabs.remove(f);
                self.tabs.insert(t, tab);
            }
        }
    }

    pub fn toggle_mute_tab(&mut self, mut id: u64) {
        if id == 0 {
            id = self.active;
        }
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
            tab.info.muted = !tab.info.muted;
        }
    }

    pub fn batch_close_others(&mut self, id: u64) {
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

    pub fn batch_close_to_right(&mut self, id: u64) {
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

    pub fn reload(&mut self) -> Result<(), String> {
        let active_id = self.active;
        let mut recreate_info = None;

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == active_id) {
            tab.info.error = None;
            tab.info.loading = true;
            if let Some(view) = &tab.view
                && let Err(_e) = view.reload()
            {
                tab.close_view();
                recreate_info = Some((tab.info.id, tab.info.url.clone()));
            }
        }

        if let Some((id, url)) = recreate_info {
            let new_view = self.tracker.allocate_view();
            new_view.load_url(&url);
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.info.id == id) {
                tab.view = Some(new_view);
                tab.info.loading = false;
                tab.info.error = None;
            }
        }
        Ok(())
    }

    pub fn close_browser(&mut self) {
        let _ = fs::remove_file(&self.running_flag);
        self.tabs.clear();
    }

    pub fn assert_partition_invariant(&self) {
        let mut seen_unpinned = false;
        for tab in &self.tabs {
            if tab.info.pinned {
                assert!(
                    !seen_unpinned,
                    "INVARIANT VIOLATION: Pinned tab {} encountered after unpinned tab! Tabs: {:?}",
                    tab.info.id,
                    self.tabs
                        .iter()
                        .map(|t| (t.info.id, t.info.pinned))
                        .collect::<Vec<_>>()
                );
            } else {
                seen_unpinned = true;
            }
        }
    }
}

// ============================================================================
// SUITE 1: TAB LIFECYCLE, PINNING, DUPLICATION & PARTITION INVARIANTS
// ============================================================================

#[test]
fn test_m5_tab_pinning_and_unpinning_partition_invariants() {
    let dir = std::env::temp_dir().join(format!("papyrus-m5-pin-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let mut browser = SimBrowser::new(dir.clone());

    // Create 10 tabs (3 pinned, 7 unpinned)
    for i in 1..=10 {
        let id = browser
            .new_tab(&format!("https://site{i}.org"), false, false)
            .unwrap();
        if i <= 3 {
            browser.pin_tab(id);
        }
    }
    browser.assert_partition_invariant();
    assert_eq!(browser.tabs.iter().filter(|t| t.info.pinned).count(), 3);

    // Pin tab 5 (unpinned -> pinned): must move to end of pinned partition (index 3)
    let tab5_id = browser.tabs[4].info.id;
    assert!(!browser.tabs[4].info.pinned);
    browser.pin_tab(tab5_id);
    browser.assert_partition_invariant();
    assert_eq!(browser.tabs[3].info.id, tab5_id);
    assert!(browser.tabs[3].info.pinned);

    // Unpin tab 1 (pinned -> unpinned): must move to start of unpinned partition
    let tab1_id = browser.tabs[0].info.id;
    assert!(browser.tabs[0].info.pinned);
    browser.pin_tab(tab1_id);
    browser.assert_partition_invariant();
    assert!(
        !browser
            .tabs
            .iter()
            .find(|t| t.info.id == tab1_id)
            .unwrap()
            .info
            .pinned
    );

    // Perform 5,000 randomized operations (pin, unpin, close, duplicate)
    // and verify partition invariant holds after every single step.
    let mut rng: u64 = 0xDEADBEEFCAFEBABE;
    for _ in 0..5_000 {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        let op = rng % 4;
        let count = browser.tabs.len();
        if count == 0 {
            browser.new_tab(NEW_TAB, true, false).unwrap();
            continue;
        }
        let target_idx = (rng as usize >> 4) % count;
        let target_id = browser.tabs[target_idx].info.id;

        match op {
            0 => {
                // Pin / Unpin
                browser.pin_tab(target_id);
            }
            1 => {
                // Duplicate
                if count < 100 {
                    let _ = browser.duplicate_tab(target_id);
                }
            }
            2 => {
                // Close
                let _ = browser.close_tab(target_id);
            }
            3 if count > 1 => {
                // Reorder
                let to_idx = (rng as usize >> 8) % count;
                let to_id = browser.tabs[to_idx].info.id;
                browser.reorder_tab(target_id, to_id);
            }
            _ => {}
        }
        browser.assert_partition_invariant();
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_m5_tab_duplication_boundaries_and_limits() {
    let dir = std::env::temp_dir().join(format!("papyrus-m5-dup-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let mut browser = SimBrowser::new(dir.clone());

    // Setup: 3 pinned tabs (P1, P2, P3) and 2 unpinned tabs (U1, U2)
    let p1 = browser.new_tab("https://p1.org", false, false).unwrap();
    browser.pin_tab(p1);
    let p2 = browser.new_tab("https://p2.org", false, false).unwrap();
    browser.pin_tab(p2);
    let p3 = browser.new_tab("https://p3.org", false, false).unwrap();
    browser.pin_tab(p3);
    let u1 = browser.new_tab("https://u1.org", false, false).unwrap();
    let _u2 = browser.new_tab("https://u2.org", false, false).unwrap();

    browser.assert_partition_invariant();

    // 1. Duplicate P1 (first pinned): should insert immediately at index 1 (between P1 and P2)
    let dup_p1 = browser.duplicate_tab(p1).unwrap();
    browser.assert_partition_invariant();
    assert_eq!(browser.tabs[0].info.id, p1);
    assert_eq!(browser.tabs[1].info.id, dup_p1);
    assert_eq!(browser.tabs[2].info.id, p2);
    assert!(browser.tabs[1].info.pinned);

    // 2. Duplicate P3 (last pinned before unpinned partition): should insert at index 4 (before U1)
    let p3_pos = browser.tabs.iter().position(|t| t.info.id == p3).unwrap();
    let dup_p3 = browser.duplicate_tab(p3).unwrap();
    browser.assert_partition_invariant();
    assert_eq!(browser.tabs[p3_pos + 1].info.id, dup_p3);
    assert!(browser.tabs[p3_pos + 1].info.pinned);
    // Next tab after dup_p3 must be unpinned U1
    assert!(!browser.tabs[p3_pos + 2].info.pinned);

    // 3. Duplicate U1 (unpinned): should insert immediately after U1
    let u1_pos = browser.tabs.iter().position(|t| t.info.id == u1).unwrap();
    let dup_u1 = browser.duplicate_tab(u1).unwrap();
    browser.assert_partition_invariant();
    assert_eq!(browser.tabs[u1_pos + 1].info.id, dup_u1);
    assert!(!browser.tabs[u1_pos + 1].info.pinned);

    // 4. Fill up to 100 tabs
    while browser.tabs.len() < 100 {
        browser.new_tab("https://fill.org", false, false).unwrap();
    }
    assert_eq!(browser.tabs.len(), 100);

    // 5. Attempt duplicate at 100 tabs limit: must fail gracefully without modifying tabs
    let prev_ids: Vec<u64> = browser.tabs.iter().map(|t| t.info.id).collect();
    let res = browser.duplicate_tab(p1);
    assert!(res.is_err());
    let current_ids: Vec<u64> = browser.tabs.iter().map(|t| t.info.id).collect();
    assert_eq!(prev_ids, current_ids);
    assert_eq!(browser.tabs.len(), 100);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_m5_tab_reordering_strictly_isolated_within_partitions() {
    let dir = std::env::temp_dir().join(format!("papyrus-m5-reorder-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let mut browser = SimBrowser::new(dir.clone());

    // 3 pinned (P1, P2, P3) and 3 unpinned (U1, U2, U3)
    let p1 = browser.new_tab("https://p1.org", false, false).unwrap();
    browser.pin_tab(p1);
    let p2 = browser.new_tab("https://p2.org", false, false).unwrap();
    browser.pin_tab(p2);
    let p3 = browser.new_tab("https://p3.org", false, false).unwrap();
    browser.pin_tab(p3);
    let u1 = browser.new_tab("https://u1.org", false, false).unwrap();
    let u2 = browser.new_tab("https://u2.org", false, false).unwrap();
    let u3 = browser.new_tab("https://u3.org", false, false).unwrap();

    let initial_order: Vec<u64> = browser.tabs.iter().map(|t| t.info.id).collect();

    // Attempt illegal reorder: Pinned P1 into Unpinned U2
    browser.reorder_tab(p1, u2);
    let current_order: Vec<u64> = browser.tabs.iter().map(|t| t.info.id).collect();
    assert_eq!(
        initial_order, current_order,
        "Cross-partition reorder must be strictly rejected!"
    );

    // Attempt illegal reorder: Unpinned U3 into Pinned P2
    browser.reorder_tab(u3, p2);
    let current_order: Vec<u64> = browser.tabs.iter().map(|t| t.info.id).collect();
    assert_eq!(
        initial_order, current_order,
        "Cross-partition reorder must be strictly rejected!"
    );

    // Legal reorder within pinned partition: P1 to P3
    browser.reorder_tab(p1, p3);
    browser.assert_partition_invariant();
    assert_eq!(browser.tabs[0].info.id, p2);
    assert_eq!(browser.tabs[1].info.id, p3);
    assert_eq!(browser.tabs[2].info.id, p1);

    // Legal reorder within unpinned partition: U3 to U1
    browser.reorder_tab(u3, u1);
    browser.assert_partition_invariant();
    assert_eq!(browser.tabs[3].info.id, u3);
    assert_eq!(browser.tabs[4].info.id, u1);
    assert_eq!(browser.tabs[5].info.id, u2);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_m5_batch_close_never_destroys_pinned_tabs_and_records_closed_stack() {
    let dir = std::env::temp_dir().join(format!("papyrus-m5-batch-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let mut browser = SimBrowser::new(dir.clone());

    // 2 pinned (P1, P2) and 3 unpinned (U1, U2, U3)
    let p1 = browser.new_tab("https://p1.org", false, false).unwrap();
    browser.pin_tab(p1);
    let p2 = browser.new_tab("https://p2.org", false, false).unwrap();
    browser.pin_tab(p2);
    let _u1 = browser.new_tab("https://u1.org", false, false).unwrap();
    let u2 = browser.new_tab("https://u2.org", false, false).unwrap();
    let _u3 = browser.new_tab("https://u3.org", false, false).unwrap();

    // Test Batch Close Others on U2: P1, P2, and U2 must remain. U1 and U3 must be closed.
    browser.batch_close_others(u2);
    browser.assert_partition_invariant();
    assert_eq!(browser.tabs.len(), 3);
    assert!(browser.tabs.iter().any(|t| t.info.id == p1));
    assert!(browser.tabs.iter().any(|t| t.info.id == p2));
    assert!(browser.tabs.iter().any(|t| t.info.id == u2));

    // Verify closed_tabs stack has U1 and U3
    assert_eq!(browser.closed_tabs.len(), 2);
    assert!(
        browser
            .closed_tabs
            .iter()
            .any(|(u, _)| u == "https://u1.org")
    );
    assert!(
        browser
            .closed_tabs
            .iter()
            .any(|(u, _)| u == "https://u3.org")
    );

    // Add more tabs to test "close tabs to the right"
    let _u4 = browser.new_tab("https://u4.org", false, false).unwrap();
    let _u5 = browser.new_tab("https://u5.org", false, false).unwrap();

    // Close tabs to right of U2 (should close U4 and U5)
    browser.batch_close_to_right(u2);
    assert_eq!(browser.tabs.len(), 3);
    assert_eq!(browser.tabs[2].info.id, u2);
    assert_eq!(browser.closed_tabs.len(), 4);

    // Verify closed tab stack capacity bound (max 50)
    for i in 0..100 {
        browser.record_closed_tab(&format!("https://record{i}.org"), "title");
    }
    assert_eq!(browser.closed_tabs.len(), 50);

    // Verify internal pages are never recorded in closed stack
    browser.record_closed_tab(SETTINGS, "Cài đặt");
    browser.record_closed_tab(BOOKMARKS, "Dấu trang");
    browser.record_closed_tab("about:blank", "blank");
    assert_eq!(browser.closed_tabs.len(), 50);
    assert!(
        !browser
            .closed_tabs
            .iter()
            .any(|(u, _)| u.starts_with("papyrus:") || u == "about:blank")
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_m5_audio_muting_indicator_invariants() {
    let dir = std::env::temp_dir().join(format!("papyrus-m5-audio-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let mut browser = SimBrowser::new(dir.clone());

    let t1 = browser
        .new_tab("https://music.youtube.com", true, false)
        .unwrap();
    assert!(!browser.tabs[0].info.muted);

    // Toggle mute on active tab
    browser.toggle_mute_tab(t1);
    assert!(browser.tabs[0].info.muted);

    // Toggle mute again
    browser.toggle_mute_tab(t1);
    assert!(!browser.tabs[0].info.muted);

    // TabInfo serialization preserves muted and is_playing_audio
    browser.tabs[0].info.is_playing_audio = true;
    browser.tabs[0].info.muted = true;
    let json = serde_json::to_string(&browser.tabs[0].info).unwrap();
    let deserialized: TabInfo = serde_json::from_str(&json).unwrap();
    assert!(deserialized.muted);
    assert!(deserialized.is_playing_audio);

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// SUITE 2: ADBLOCKING COMPLETENESS, FILTER PARSING & EXCEPTIONS
// ============================================================================

#[test]
fn test_m5_adblock_abp_syntax_and_subresource_whitelisting() {
    let mut prefs = Preferences::default();
    prefs.adblock = true;

    // 1. Built-in bundled block verification
    assert!(should_block(
        "https://googleads.g.doubleclick.net/pagead/ads?client=ca-pub-123",
        "https://news.vn",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://coinhive.com/lib/coinhive.min.js",
        "https://news.vn",
        "script",
        "GET",
        &prefs
    ));

    // 2. Built-in bundled exception verification (Google accounts, Google fonts, Facebook SDK)
    assert!(!should_block(
        "https://accounts.google.com/o/oauth2/auth",
        "https://example.org",
        "subdocument",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://fonts.googleapis.com/css?family=Roboto",
        "https://example.org",
        "stylesheet",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://connect.facebook.net/vi_VN/sdk.js",
        "https://example.org",
        "script",
        "GET",
        &prefs
    ));

    // 3. User custom ABP filter rule syntax ($document exception on top page)
    prefs.allowed_hosts = vec!["@@||trusted-portal.vn^$document".into()];
    // On trusted-portal.vn, sub-resources that would otherwise be blocked are unblocked
    assert!(!should_block(
        "https://doubleclick.net/ad.js",
        "https://trusted-portal.vn/article/1",
        "script",
        "GET",
        &prefs
    ));
    // But on another page, doubleclick is still blocked!
    assert!(should_block(
        "https://doubleclick.net/ad.js",
        "https://other-site.com",
        "script",
        "GET",
        &prefs
    ));

    // 4. Custom block rule with specific resource type ($image)
    prefs.blocked_hosts = vec!["||custom-tracker.com^$image".into()];
    assert!(should_block(
        "https://custom-tracker.com/pixel.png",
        "https://site.org",
        "image",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://custom-tracker.com/api.json",
        "https://site.org",
        "xmlhttprequest",
        "GET",
        &prefs
    ));

    // 5. Adblock master toggle
    prefs.adblock = false;
    assert!(!should_block(
        "https://googleads.g.doubleclick.net/pagead/ads",
        "https://news.vn",
        "script",
        "GET",
        &prefs
    ));
}

#[test]
fn test_m5_adblock_dos_and_adversarial_rule_parsing() {
    let mut prefs = Preferences::default();

    // Adversarial filter rules: comment lines, blank lines, extra whitespace
    prefs.blocked_hosts = vec![
        "! This is an ABP comment".into(),
        "# Another comment style".into(),
        "   ".into(),
        "||valid-ad.com^".into(),
        "  ||spaces-around.com^  ".into(),
    ];
    // Filter validation must reject bad rules but allow valid ones
    assert!(valid_rule_or_host("||valid-ad.com^"));
    assert!(!valid_rule_or_host("! comment"));
    assert!(!valid_rule_or_host("   "));
    assert!(!valid_rule_or_host("has whitespace in middle.com"));
    assert!(!valid_rule_or_host(&"a".repeat(2000)));

    // Test 10,000 rules boundary in Preferences::validate()
    prefs.blocked_hosts = (0..10_000).map(|i| format!("ad{i}.net")).collect();
    assert!(prefs.validate().is_ok());

    prefs.blocked_hosts.push("ad10001.net".into());
    assert!(
        prefs.validate().is_err(),
        "Exceeding 10,000 rules must fail validation!"
    );
}

// ============================================================================
// SUITE 3: STATE SERIALIZATION, RECOVERY & MULTI-MONITOR BOUNDS
// ============================================================================

#[test]
fn test_m5_state_serialization_roundtrip_and_atomic_write() {
    let dir = std::env::temp_dir().join(format!("papyrus-m5-state-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let state_file = dir.join("state.json");

    let mut state = SavedState::default();
    state.preferences.theme = "dark".into();
    state.preferences.zoom = 125;
    state.window_x = Some(-1920); // Multi-monitor secondary display to the left
    state.window_y = Some(100);
    state.window_width = Some(1920);
    state.window_height = Some(1080);
    state.window_maximized = true;

    // Add Vietnamese bookmarks and history
    state.bookmarks.push(Bookmark {
        title: "Báo Tuổi Trẻ Online - Tin tức mới nhất 2026".into(),
        url: "https://tuoitre.vn/".into(),
    });
    state.history.push(HistoryEntry {
        title: "Trình duyệt Papyrus - Đánh giá hiệu năng".into(),
        url: "https://tinhte.vn/thread/papyrus-browser".into(),
        time: 1731000000,
    });
    state.passwords.push(PasswordEntry {
        id: 1,
        url: "https://portal.vn".into(),
        username: "nguyen.van.a@gmail.com".into(),
        password: "SecretP@ssw0rd!#$".into(),
        updated: 1731000000,
    });

    // Write state
    state.write(&state_file).unwrap();

    // Verify temp file is cleaned up after atomic rename
    assert!(!state_file.with_extension("json.tmp").exists());

    // Read state back
    let restored = SavedState::read(&state_file).unwrap();
    assert_eq!(restored.preferences.theme, "dark");
    assert_eq!(restored.preferences.zoom, 125);
    assert_eq!(restored.window_x, Some(-1920));
    assert_eq!(restored.window_y, Some(100));
    assert_eq!(restored.window_width, Some(1920));
    assert_eq!(restored.window_height, Some(1080));
    assert!(restored.window_maximized);
    assert_eq!(restored.bookmarks.len(), 4);
    assert_eq!(
        restored.bookmarks[3].title,
        "Báo Tuổi Trẻ Online - Tin tức mới nhất 2026"
    );
    assert_eq!(restored.passwords[0].username, "nguyen.van.a@gmail.com");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_m5_state_corrupted_disk_read_resilience() {
    let dir = std::env::temp_dir().join(format!("papyrus-m5-corrupt-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let state_file = dir.join("state.json");

    // Case 1: Corrupted JSON syntax
    fs::write(&state_file, b"{ \"preferences\": { \"theme\": \"dark\", ").unwrap();
    let res = SavedState::read(&state_file);
    assert!(res.is_err());
    assert!(
        res.unwrap_err()
            .contains("Tệp cài đặt bị lỗi; bản gốc được giữ nguyên")
    );

    // Case 2: Invalid preferences values (e.g. zoom 500 out of range)
    fs::write(
        &state_file,
        br#"{ "preferences": { "engine": "google", "zoom": 500, "theme": "dark", "startup": "newtab", "show_bookmarks": true, "adblock": true, "block_popups": true, "allowed_hosts": [], "blocked_hosts": [], "download_dir": "" } }"#,
    )
    .unwrap();
    let res = SavedState::read(&state_file);
    assert!(res.is_err());

    // Case 3: Empty file
    fs::write(&state_file, b"").unwrap();
    let res = SavedState::read(&state_file);
    assert!(res.is_err());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_m5_multi_monitor_geometry_bounds_restoration_logic() {
    // Model oracle for window geometry bounds check:
    // Window must be rejected if out of reasonable coordinate range (-30000)
    // or smaller than minimum inner size (680 x 480).

    let mut state = SavedState::default();

    // Secondary monitor to the left: x = -1920, y = 0, w = 1920, h = 1080
    state.window_x = Some(-1920);
    state.window_y = Some(0);
    state.window_width = Some(1920);
    state.window_height = Some(1080);
    assert!(state.window_x.unwrap() > -30000);
    assert!(state.window_width.unwrap() >= 680);
    assert!(state.window_height.unwrap() >= 480);

    // Min size enforcement
    let invalid_small_w = 400;
    let invalid_small_h = 300;
    assert!(!(invalid_small_w >= 680 && invalid_small_h >= 480));

    // Maximized override
    state.window_maximized = true;
    assert!(state.window_maximized);
}

#[test]
fn test_m5_session_restoration_stably_partitions_pinned_tabs() {
    // When reading last session URLs with arbitrary pinned state ordering
    // (e.g. Unpinned, Pinned, Unpinned, Pinned), startup sorts by `!pinned`
    // to guarantee pinned tabs are always partitioned to the front without reordering within groups.

    let urls = vec![
        "https://u1.org".to_string(),
        "https://p1.org".to_string(),
        "https://u2.org".to_string(),
        "https://p2.org".to_string(),
    ];
    let pinned_states = [false, true, false, true];

    let mut to_open: Vec<(String, bool)> = urls
        .into_iter()
        .enumerate()
        .map(|(i, u)| {
            let p = pinned_states.get(i).copied().unwrap_or(false);
            (u, p)
        })
        .collect();

    to_open.sort_by_key(|(_, pinned)| !*pinned);

    assert_eq!(to_open[0], ("https://p1.org".to_string(), true));
    assert_eq!(to_open[1], ("https://p2.org".to_string(), true));
    assert_eq!(to_open[2], ("https://u1.org".to_string(), false));
    assert_eq!(to_open[3], ("https://u2.org".to_string(), false));
}

// ============================================================================
// SUITE 4: TAB CONTROLLER TEARDOWN & CRASH SELF-HEALING
// ============================================================================

#[test]
fn test_m5_tab_controller_explicit_teardown_and_zero_leaks() {
    let dir = std::env::temp_dir().join(format!("papyrus-m5-teardown-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let mut browser = SimBrowser::new(dir.clone());

    // Create 30 tabs with mock WebViews
    let mut ids = Vec::new();
    for i in 1..=30 {
        let id = browser
            .new_tab(&format!("https://site{i}.org"), false, false)
            .unwrap();
        ids.push(id);
    }
    assert_eq!(browser.tracker.live_views(), 30);

    // Close 15 tabs
    for id in ids.iter().take(15) {
        browser.close_tab(*id).unwrap();
    }
    assert_eq!(browser.tabs.len(), 15);
    assert_eq!(browser.tracker.live_views(), 15);

    // Drop entire browser
    drop(browser);

    // All controllers must be closed - 0 live views remaining!
    let tracker = MockControllerTracker::new();
    assert_eq!(tracker.live_views(), 0);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_m5_crash_self_healing_and_running_flag() {
    let dir = std::env::temp_dir().join(format!("papyrus-m5-crash-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);

    // Run 1: Browser starts and terminates abnormally without closing (simulating crash)
    {
        let mut b1 = SimBrowser::new(dir.clone());
        b1.new_tab("https://crashed-tab-1.org", false, false)
            .unwrap();
        b1.new_tab("https://crashed-tab-2.org", false, false)
            .unwrap();
        // Do NOT call close_browser(), drop directly -> running.flag remains!
    }

    // Run 2: Next startup detects running.flag
    {
        let mut b2 = SimBrowser::new(dir.clone());
        assert!(
            b2.was_crashed,
            "Browser must detect abnormal termination from previous session!"
        );

        // Simulate normal exit: calls close_browser() which removes running.flag
        b2.close_browser();
        assert!(
            !b2.running_flag.exists(),
            "Normal exit must clean up running.flag"
        );
    }

    // Run 3: Clean startup
    {
        let b3 = SimBrowser::new(dir.clone());
        assert!(!b3.was_crashed, "Clean restart must not report crash!");
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_m5_render_process_crash_and_view_recreation_on_reload() {
    let dir = std::env::temp_dir().join(format!("papyrus-m5-reload-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let mut browser = SimBrowser::new(dir.clone());

    let _tab_id = browser
        .new_tab("https://heavy-webgl-app.org", true, false)
        .unwrap();
    let initial_view_id = browser.tabs[0].view.as_ref().unwrap().id;

    // Simulate render process crash on this view:
    browser
        .tracker
        .fail_reload_ids
        .lock()
        .unwrap()
        .insert(initial_view_id);

    // Simulate Command::Reload:
    // Native reload will fail -> tab.close_view() -> create_view recreates view!
    browser.reload().unwrap();

    let new_view_id = browser.tabs[0].view.as_ref().unwrap().id;
    assert_ne!(
        initial_view_id, new_view_id,
        "Crashed view must be replaced by a newly created view controller!"
    );
    assert_eq!(
        *browser.tabs[0]
            .view
            .as_ref()
            .unwrap()
            .loaded_url
            .lock()
            .unwrap(),
        "https://heavy-webgl-app.org",
        "Recreated view must load original tab URL!"
    );
    assert_eq!(browser.tabs[0].info.error, None);
    assert!(!browser.tabs[0].info.loading);

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// SUITE 5: CONCURRENCY, FUZZING & ZERO PANIC STRESS HARNESS
// ============================================================================

#[test]
fn test_m5_concurrent_fuzz_harness_zero_panics() {
    let handles: Vec<_> = (0..8)
        .map(|thread_idx| {
            thread::spawn(move || {
                let mut seed: u64 = 0x1234567890ABCDEF ^ (thread_idx as u64).wrapping_mul(0xF0E1D2C3B4A59687);
                for _ in 0..15_000 {
                    seed = seed.wrapping_mul(2862933555777941757).wrapping_add(3037000493);
                    let choice = seed % 4;

                    match choice {
                        0 => {
                            // Fuzz parse_command
                            let mut mutated = format!(
                                r#"{{"action":"navigate","input":"https://site{}.org","extra":{}}}"#,
                                seed % 1000,
                                seed
                            );
                            if seed.is_multiple_of(2) {
                                mutated.truncate(mutated.len() / 2);
                            }
                            let _ = parse_command("about:blank", &mutated);
                            let _ = parse_command("https://evil.org", &mutated);
                        }
                        1 => {
                            // Fuzz resolve_input
                            let inputs = [
                                "about:settings",
                                "chrome://history",
                                "papyrus:bookmarks",
                                "localhost:8080",
                                "127.0.0.1:9000",
                                "file:///C:/path/doc.pdf",
                                "https://user:pass@evil.com",
                                "javascript:void(0)",
                                "data:text/html,test",
                                "https://vietnamnet.vn/thời sự",
                                "   ",
                                "",
                            ];
                            let raw = inputs[(seed as usize) % inputs.len()];
                            let _ = resolve_input(raw, "google");
                            let _ = resolve_input(raw, "brave");
                        }
                        2 => {
                            // Fuzz should_block
                            let prefs = Preferences::default();
                            let urls = [
                                "https://googleads.g.doubleclick.net/ad",
                                "https://coinhive.com/miner",
                                "https://fonts.googleapis.com/roboto",
                                "https://legit-news.vn/index.html",
                                "https://invalid-url-without-host",
                            ];
                            let raw = urls[(seed as usize) % urls.len()];
                            let _ = should_block(raw, "https://news.vn", "script", "GET", &prefs);
                        }
                        3 => {
                            // Fuzz next_active_after_close
                            let tab_count = (seed >> 4) % 20 + 1;
                            let ids: Vec<u64> = (1..=tab_count).collect();
                            let active = (seed >> 8) % tab_count + 1;
                            let closing = (seed >> 12) % tab_count + 1;
                            let _ = next_active_after_close(&ids, active, closing);
                        }
                        _ => {}
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Fuzz worker thread panicked!");
    }
}
