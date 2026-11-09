#![allow(dead_code, unused_imports)]

#[path = "../src/model.rs"]
mod model;

use model::{
    Bookmark, Command, HistoryEntry, NEW_TAB, PasswordEntry, Preferences, SavedState, TabInfo,
    allowed_navigation, parse_command,
};
use std::fs;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromRect};

fn is_rect_visible_on_any_monitor(x: i32, y: i32, w: u32, h: u32) -> bool {
    let rect = RECT {
        left: x,
        top: y,
        right: x.saturating_add(w as i32),
        bottom: y.saturating_add(h as i32),
    };
    let hmon = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) };
    !hmon.0.is_null()
}

// ============================================================================
// SUITE 1: SavedState JSON Deserialization & Backward Compatibility Fuzzing
// ============================================================================

#[test]
fn test_saved_state_empty_and_legacy_schemas() {
    // 1. Empty JSON object
    let empty_json = "{}";
    let state: SavedState =
        serde_json::from_str(empty_json).expect("Empty JSON must deserialize to default");
    assert!(state.window_x.is_none());
    assert!(state.window_y.is_none());
    assert!(state.window_width.is_none());
    assert!(state.window_height.is_none());
    assert!(!state.window_maximized);
    assert!(state.last_session_pinned.is_empty());
    assert!(state.last_session_urls.is_empty());
    assert_eq!(state.last_session_active, 0);

    // 2. M0 Legacy Schema: only preferences and bookmarks
    let legacy_m0 = r#"{
        "preferences": {
            "engine": "google",
            "zoom": 100,
            "theme": "kraft",
            "startup": "newtab",
            "show_bookmarks": true,
            "adblock": true,
            "block_popups": true,
            "allowed_hosts": [],
            "blocked_hosts": [],
            "download_dir": ""
        },
        "bookmarks": [
            {"title": "Google", "url": "https://google.com/"}
        ]
    }"#;
    let state_m0: SavedState =
        serde_json::from_str(legacy_m0).expect("M0 legacy schema must deserialize cleanly");
    assert_eq!(state_m0.bookmarks.len(), 1);
    assert_eq!(state_m0.bookmarks[0].title, "Google");
    assert!(state_m0.window_x.is_none());
    assert!(state_m0.window_y.is_none());
    assert!(state_m0.last_session_pinned.is_empty());
    assert!(!state_m0.window_maximized);

    // 3. M1 Legacy Schema: contains restored_urls, passwords, history, but lacks window bounds and pinned state
    let legacy_m1 = r#"{
        "preferences": {
            "engine": "brave",
            "zoom": 125,
            "theme": "dark",
            "startup": "restore",
            "show_bookmarks": false,
            "adblock": true,
            "block_popups": true,
            "allowed_hosts": ["example.com"],
            "blocked_hosts": ["ads.example.com"],
            "download_dir": "C:/Downloads"
        },
        "bookmarks": [],
        "history": [{"title": "Old Tab", "url": "https://example.org/", "time": 1000}],
        "passwords": [{"id": 1, "url": "https://login.example.com", "username": "user", "password": "pwd", "updated": 2000}],
        "restored_urls": ["https://example.org/1", "https://example.org/2"],
        "restored_active": 1,
        "last_session_urls": ["https://example.org/1", "https://example.org/2"],
        "last_session_active": 1
    }"#;
    let state_m1: SavedState =
        serde_json::from_str(legacy_m1).expect("M1 legacy schema must deserialize cleanly");
    assert_eq!(state_m1.preferences.engine, "brave");
    assert_eq!(state_m1.restored_urls.len(), 2);
    assert_eq!(state_m1.last_session_urls.len(), 2);
    assert_eq!(state_m1.last_session_active, 1);
    assert!(
        state_m1.last_session_pinned.is_empty(),
        "Missing last_session_pinned must default to empty vec"
    );
    assert!(
        state_m1.window_x.is_none(),
        "Missing window_x must default to None"
    );
    assert!(
        state_m1.window_y.is_none(),
        "Missing window_y must default to None"
    );
    assert!(state_m1.window_width.is_none());
    assert!(state_m1.window_height.is_none());
    assert!(!state_m1.window_maximized);

    // 4. Schema with unknown future keys (forward compatibility)
    let future_schema = r#"{
        "window_x": 100,
        "window_y": 200,
        "window_width": 1280,
        "window_height": 840,
        "window_maximized": true,
        "last_session_pinned": [true, false],
        "last_session_urls": ["https://a.com", "https://b.com"],
        "future_tab_groups": [{"name": "work"}],
        "experimental_flags": {"ai_copilot": true}
    }"#;
    let state_future: SavedState = serde_json::from_str(future_schema)
        .expect("Future schema with unknown keys must deserialize smoothly");
    assert_eq!(state_future.window_x, Some(100));
    assert_eq!(state_future.window_y, Some(200));
    assert_eq!(state_future.window_width, Some(1280));
    assert_eq!(state_future.window_height, Some(840));
    assert!(state_future.window_maximized);
    assert_eq!(state_future.last_session_pinned, vec![true, false]);
}

#[test]
fn test_saved_state_corrupted_partial_malformed_fuzz() {
    let malformed_payloads = [
        "", // completely empty
        "   ",
        "\0\0\0\0",
        "{",
        "{\"preferences\":",
        "{\"preferences\": {",
        "{\"window_x\": \"not_a_number\"}",
        "{\"window_width\": -100}", // negative number where u32 is expected
        "{\"window_height\": 99999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999}",
        "{\"window_maximized\": \"yes\"}", // string instead of bool
        "{\"last_session_pinned\": true}", // bool instead of array
        "{\"last_session_pinned\": [\"true\", 1, null]}", // invalid element types
        "{\"last_session_urls\": \"https://example.com\"}", // string instead of array
        "{\"bookmarks\": \"corrupted\"}",
        "{\"history\": null}", // null where vec is expected without serde default null handling
        "[1, 2, 3]",           // array root instead of object
        "\"string_root\"",
        "12345",
        "true",
        "null",
        "{\"preferences\": {\"zoom\": 1000000}}", // zoom violates validation
        "{\"preferences\": {\"theme\": \"unsupported_theme\"}}", // invalid theme
    ];

    for (i, payload) in malformed_payloads.iter().enumerate() {
        let res: Result<SavedState, _> = serde_json::from_str(payload);
        // Either serde rejects it, or if it deserializes (e.g. preferences zoom/theme), preferences.validate() must catch it
        if let Ok(st) = res {
            let val_res = st.preferences.validate();
            assert!(
                val_res.is_err(),
                "Malformed payload #{i} must fail either deserialization or validate(): {payload}"
            );
        }
    }
}

#[test]
fn test_saved_state_disk_read_corruption_resilience() {
    let temp_dir =
        std::env::temp_dir().join(format!("papyrus_challenger_m2_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).unwrap();
    let file_path = temp_dir.join("state.json");

    // 1. Non-existent file defaults cleanly
    let non_existent = temp_dir.join("does_not_exist.json");
    let res = SavedState::read(&non_existent);
    assert!(res.is_ok(), "Non-existent path must return Ok(Default)");

    // 2. Corrupted file returns Err and preserves file content untouched
    let corrupted_content = b"{\"window_x\": invalid_json_content...";
    fs::write(&file_path, corrupted_content).unwrap();
    let read_res = SavedState::read(&file_path);
    assert!(read_res.is_err(), "Corrupted state must return Err");
    let on_disk = fs::read(&file_path).unwrap();
    assert_eq!(
        on_disk, corrupted_content,
        "Original corrupted file must not be modified or overwritten by read()"
    );

    // 3. Atomic write and reload verification
    let state = SavedState {
        window_x: Some(150),
        window_y: Some(250),
        window_width: Some(1400),
        window_height: Some(900),
        window_maximized: false,
        last_session_urls: vec!["https://rust-lang.org/".into()],
        last_session_pinned: vec![true],
        ..Default::default()
    };

    assert!(
        state.write(&file_path).is_ok(),
        "Writing valid state must succeed"
    );
    let reloaded = SavedState::read(&file_path).expect("Reloading written state must succeed");
    assert_eq!(reloaded.window_x, Some(150));
    assert_eq!(reloaded.window_y, Some(250));
    assert_eq!(reloaded.window_width, Some(1400));
    assert_eq!(reloaded.window_height, Some(900));
    assert_eq!(reloaded.last_session_pinned, vec![true]);

    let _ = fs::remove_dir_all(&temp_dir);
}

// ============================================================================
// SUITE 2: Window Bounds & Monitor Geometry Fuzzing
// ============================================================================

#[test]
fn test_window_geometry_fuzzing_and_monitor_visibility() {
    // 1. Normal coordinates: must be visible on the system's active display
    let normal_visible = is_rect_visible_on_any_monitor(100, 100, 1280, 840);
    assert!(
        normal_visible,
        "Standard 100,100,1280,840 rect must be visible on active monitor"
    );

    // 2. Win32 Minimized Coordinates: (-32000, -32000)
    // When a window is minimized, Windows places it at (-32000, -32000).
    // is_rect_visible_on_any_monitor must return FALSE so it is NOT restored to minimized coords!
    let minimized_visible = is_rect_visible_on_any_monitor(-32000, -32000, 1280, 840);
    assert!(
        !minimized_visible,
        "Minimized coords (-32000, -32000) must NOT be visible on any monitor"
    );

    // 3. Extreme negative offscreen coordinates
    assert!(!is_rect_visible_on_any_monitor(-500000, -500000, 1280, 840));
    assert!(!is_rect_visible_on_any_monitor(
        i32::MIN,
        i32::MIN,
        1280,
        840
    ));

    // 4. Extreme positive offscreen coordinates
    assert!(!is_rect_visible_on_any_monitor(500000, 500000, 1280, 840));
    assert!(!is_rect_visible_on_any_monitor(
        i32::MAX - 2000,
        i32::MAX - 2000,
        1280,
        840
    ));

    // 5. Overflowing u32 dimensions (w = u32::MAX, h = u32::MAX)
    // Testing behavior when w as i32 becomes negative (-1)
    // For coords on a monitor (100, 100), Win32 GDI MonitorFromRect checks point (100, 100) and finds the monitor.
    // But for offscreen coords (-32000, -32000), even with u32::MAX it is not on any monitor.
    let offscreen_overflow = is_rect_visible_on_any_monitor(-32000, -32000, u32::MAX, u32::MAX);
    assert!(
        !offscreen_overflow,
        "Offscreen coords with u32::MAX must return false"
    );

    // 6. Huge dimensions (999999 x 999999) starting at 0, 0
    // Encompasses all monitors, so MonitorFromRect returns the monitor it intersects
    let huge_visible = is_rect_visible_on_any_monitor(0, 0, 999999, 999999);
    assert!(
        huge_visible,
        "Huge dimensions covering screen must be detected without crashing"
    );

    // 7. Size boundary clamping logic verification:
    // Browser minimum size is 680x480. Verify logic:
    let clamp_check = |w: Option<u32>, h: Option<u32>| -> (f64, f64) {
        if let (Some(w), Some(h)) = (w, h) {
            if w >= 680 && h >= 480 {
                (w as f64, h as f64)
            } else {
                (1280.0, 840.0)
            }
        } else {
            (1280.0, 840.0)
        }
    };

    assert_eq!(
        clamp_check(Some(100), Some(100)),
        (1280.0, 840.0),
        "Too small width and height must reset to 1280x840"
    );
    assert_eq!(
        clamp_check(Some(679), Some(480)),
        (1280.0, 840.0),
        "Width 679 must reset to 1280x840"
    );
    assert_eq!(
        clamp_check(Some(680), Some(479)),
        (1280.0, 840.0),
        "Height 479 must reset to 1280x840"
    );
    assert_eq!(
        clamp_check(Some(680), Some(480)),
        (680.0, 480.0),
        "Exact min size 680x480 accepted"
    );
    assert_eq!(clamp_check(Some(1920), Some(1080)), (1920.0, 1080.0));
    assert_eq!(clamp_check(None, None), (1280.0, 840.0));
}

// ============================================================================
// SUITE 3: Session State 500+ URLs and Array Length Mismatch Stress Tests
// ============================================================================

#[test]
fn test_session_state_500_tabs_and_mismatched_pinned_arrays() {
    // Helper replicating Browser::create_at session restoration logic
    let restore_session =
        |urls: &[String], pinned: &[bool], active_idx: usize| -> (Vec<String>, Vec<bool>, usize) {
            let mut valid_urls = Vec::new();
            let mut valid_pinned = Vec::new();
            for (i, url) in urls.iter().enumerate() {
                if allowed_navigation(url) && valid_urls.len() < 100 {
                    valid_urls.push(url.clone());
                    valid_pinned.push(pinned.get(i).copied().unwrap_or(false));
                }
            }
            let mut to_open: Vec<(String, bool)> =
                valid_urls.into_iter().zip(valid_pinned).collect();
            to_open.sort_by_key(|(_, pinned)| !*pinned);
            let bounded_active = if to_open.is_empty() {
                0
            } else if active_idx < to_open.len() {
                active_idx
            } else {
                0
            };
            let restored_urls = to_open.iter().map(|(u, _)| u.clone()).collect();
            let restored_pinned = to_open.iter().map(|(_, p)| *p).collect();
            (restored_urls, restored_pinned, bounded_active)
        };

    // Case 1: 500 valid URLs, but 0 pinned booleans (empty pinned array)
    let urls_500: Vec<String> = (0..500)
        .map(|i| format!("https://example.com/page/{}", i))
        .collect();
    let empty_pinned: Vec<bool> = vec![];
    let (restored_urls, restored_pinned, active) = restore_session(&urls_500, &empty_pinned, 45);

    assert_eq!(
        restored_urls.len(),
        100,
        "Must be hard-capped at 100 tabs max"
    );
    assert_eq!(
        restored_pinned.len(),
        100,
        "Pinned array must match restored URLs length"
    );
    assert!(
        restored_pinned.iter().all(|&p| !p),
        "All tabs must default to false when pinned array is empty"
    );
    assert_eq!(active, 45);

    // Case 2: 500 valid URLs, only 5 pinned booleans (first 5 true)
    let pinned_5 = vec![true, true, true, true, true];
    let (restored_urls, restored_pinned, _) = restore_session(&urls_500, &pinned_5, 0);
    assert_eq!(restored_urls.len(), 100);
    assert_eq!(restored_pinned.len(), 100);
    assert_eq!(&restored_pinned[0..5], &[true, true, true, true, true]);
    assert!(
        restored_pinned[5..].iter().all(|&p| !p),
        "Subsequent tabs past pinned length must default to false"
    );

    // Case 3: 5 valid URLs, 500 pinned booleans (alternating true/false)
    let urls_5: Vec<String> = (0..5)
        .map(|i| format!("https://example.com/tab/{}", i))
        .collect();
    let pinned_500: Vec<bool> = (0..500).map(|i| i % 2 == 0).collect();
    let (restored_urls, restored_pinned, _) = restore_session(&urls_5, &pinned_500, 0);
    assert_eq!(
        restored_urls.len(),
        5,
        "Restored length must equal valid URLs length"
    );
    assert_eq!(
        restored_pinned.len(),
        5,
        "Pinned length must equal restored URLs length"
    );
    assert_eq!(
        restored_pinned,
        vec![true, true, true, false, false],
        "All pinned tabs must precede unpinned tabs after partition sorting"
    );
    assert_eq!(
        restored_urls,
        vec![
            "https://example.com/tab/0",
            "https://example.com/tab/2",
            "https://example.com/tab/4",
            "https://example.com/tab/1",
            "https://example.com/tab/3"
        ],
        "Stable sort must preserve relative order within pinned and unpinned partitions"
    );

    // Case 4: 500 URLs where every 3rd URL is disallowed / malicious (e.g. javascript:, file:, about:blank)
    let mut mixed_urls = Vec::new();
    let mut mixed_pinned = Vec::new();
    for i in 0..500 {
        if i % 3 == 0 {
            mixed_urls.push("javascript:alert(1)".into());
            mixed_pinned.push(true);
        } else if i % 5 == 0 {
            mixed_urls.push("data:text/html,evil".into());
            mixed_pinned.push(true);
        } else {
            mixed_urls.push(format!("https://legit.example.com/{}", i));
            mixed_pinned.push(i % 2 == 1);
        }
    }
    let (restored_urls, restored_pinned, _) = restore_session(&mixed_urls, &mixed_pinned, 0);
    assert!(restored_urls.len() <= 100);
    assert_eq!(restored_urls.len(), restored_pinned.len());
    for url in &restored_urls {
        assert!(
            allowed_navigation(url),
            "Disallowed URL leaked into restored tabs: {}",
            url
        );
        assert!(!url.starts_with("javascript:"));
        assert!(!url.starts_with("data:"));
    }

    // Case 5: Active index out of bounds (active = 999999, or 450 when restored is capped at 100)
    let (_, _, bounded_active) = restore_session(&urls_500, &empty_pinned, 450);
    assert_eq!(
        bounded_active, 0,
        "Out of bounds active index must safely fall back to 0 without panicking"
    );
    let (_, _, bounded_active2) = restore_session(&urls_500, &empty_pinned, usize::MAX);
    assert_eq!(
        bounded_active2, 0,
        "usize::MAX active index must safely fall back to 0"
    );
}

#[test]
fn test_session_restoration_stably_partitions_out_of_order_pinned_tabs() {
    let partition_session = |urls: &[String], pinned: &[bool]| -> (Vec<String>, Vec<bool>) {
        let mut valid_urls = Vec::new();
        let mut valid_pinned = Vec::new();
        for (i, url) in urls.iter().enumerate() {
            if allowed_navigation(url) && valid_urls.len() < 100 {
                valid_urls.push(url.clone());
                valid_pinned.push(pinned.get(i).copied().unwrap_or(false));
            }
        }
        let mut to_open: Vec<(String, bool)> = valid_urls.into_iter().zip(valid_pinned).collect();
        to_open.sort_by_key(|(_, p)| !*p);
        let out_urls = to_open.iter().map(|(u, _)| u.clone()).collect();
        let out_pinned = to_open.iter().map(|(_, p)| *p).collect();
        (out_urls, out_pinned)
    };

    // 1. Inverted order [unpinned, pinned]
    let urls = vec!["https://unpinned.org".into(), "https://pinned.org".into()];
    let pinned = vec![false, true];
    let (res_urls, res_pinned) = partition_session(&urls, &pinned);
    assert_eq!(res_pinned, vec![true, false]);
    assert_eq!(res_urls, vec!["https://pinned.org", "https://unpinned.org"]);

    // 2. Interleaved order [U1, P1, U2, P2] -> [P1, P2, U1, U2]
    let urls = vec![
        "https://u1.org".into(),
        "https://p1.org".into(),
        "https://u2.org".into(),
        "https://p2.org".into(),
    ];
    let pinned = vec![false, true, false, true];
    let (res_urls, res_pinned) = partition_session(&urls, &pinned);
    assert_eq!(res_pinned, vec![true, true, false, false]);
    assert_eq!(
        res_urls,
        vec![
            "https://p1.org",
            "https://p2.org",
            "https://u1.org",
            "https://u2.org"
        ]
    );

    // 3. Already partitioned order remains untouched
    let urls = vec![
        "https://p1.org".into(),
        "https://p2.org".into(),
        "https://u1.org".into(),
    ];
    let pinned = vec![true, true, false];
    let (res_urls, res_pinned) = partition_session(&urls, &pinned);
    assert_eq!(res_pinned, vec![true, true, false]);
    assert_eq!(res_urls, urls);

    // 4. Strict partition invariant assertion across all elements
    let has_seen_unpinned = res_pinned.iter().position(|&p| !p);
    if let Some(first_unpinned) = has_seen_unpinned {
        assert!(
            res_pinned[first_unpinned..].iter().all(|&p| !p),
            "No pinned tab may appear after the first unpinned tab"
        );
    }
}

// ============================================================================
// SUITE 4: Backend ReorderTab Pinned Boundary Enforcement Stress Tests
// ============================================================================

#[test]
fn test_backend_reorder_tab_pinned_boundary_invariants() {
    // Simulate backend tab vector with pinned tabs at front
    struct TestTab {
        id: u64,
        pinned: bool,
    }
    let mut tabs = vec![
        TestTab {
            id: 1,
            pinned: true,
        },
        TestTab {
            id: 2,
            pinned: true,
        },
        TestTab {
            id: 3,
            pinned: false,
        },
        TestTab {
            id: 4,
            pinned: false,
        },
        TestTab {
            id: 5,
            pinned: false,
        },
    ];

    // Backend reorder logic from main.rs:884-897
    let try_reorder = |tabs: &mut Vec<TestTab>, from: u64, to: u64| -> bool {
        if let (Some(f), Some(t)) = (
            tabs.iter().position(|tab| tab.id == from),
            tabs.iter().position(|tab| tab.id == to),
        ) && f != t
            && tabs[f].pinned == tabs[t].pinned
        {
            let tab = tabs.remove(f);
            tabs.insert(t, tab);
            return true;
        }
        false
    };

    // 1. Attempt to move pinned tab (id: 1) into unpinned position (id: 4)
    assert!(
        !try_reorder(&mut tabs, 1, 4),
        "Moving pinned tab to unpinned tab must be REJECTED"
    );
    assert_eq!(tabs[0].id, 1);
    assert_eq!(tabs[1].id, 2);

    // 2. Attempt to move unpinned tab (id: 4) into pinned position (id: 1)
    assert!(
        !try_reorder(&mut tabs, 4, 1),
        "Moving unpinned tab to pinned tab must be REJECTED"
    );
    assert_eq!(tabs[0].id, 1);
    assert_eq!(tabs[3].id, 4);

    // 3. Move pinned tab (id: 1) to another pinned tab (id: 2) -> Allowed
    assert!(
        try_reorder(&mut tabs, 1, 2),
        "Reordering within pinned boundary must SUCCEED"
    );
    assert_eq!(tabs[0].id, 2);
    assert_eq!(tabs[1].id, 1);

    // 4. Move unpinned tab (id: 5) to unpinned tab (id: 3) -> Allowed
    assert!(
        try_reorder(&mut tabs, 5, 3),
        "Reordering within unpinned boundary must SUCCEED"
    );
    assert_eq!(tabs[2].id, 5);
    assert_eq!(tabs[3].id, 3);
    assert_eq!(tabs[4].id, 4);

    // 5. Non-existent IDs
    assert!(!try_reorder(&mut tabs, 999, 1));
    assert!(!try_reorder(&mut tabs, 1, 999));
    assert!(!try_reorder(&mut tabs, 1, 1)); // same tab
}

// ============================================================================
// SUITE 5: Adversarial Stress Testing of Session Restoration Invariant Sorting
// ============================================================================

/// Exact simulation of Browser::create_at session restoration logic (src/main.rs:313-379)
fn simulate_session_restore(
    urls: &[String],
    pinned_states: &[bool],
    active_idx: usize,
) -> (Vec<(String, bool)>, usize) {
    let mut valid_urls = Vec::new();
    let mut valid_pinned = Vec::new();
    for (i, url) in urls.iter().enumerate() {
        if allowed_navigation(url) && valid_urls.len() < 100 {
            valid_urls.push(url.clone());
            valid_pinned.push(pinned_states.get(i).copied().unwrap_or(false));
        }
    }

    let mut to_open: Vec<(String, bool)> = if valid_urls.is_empty() {
        vec![(NEW_TAB.into(), false)]
    } else {
        valid_urls
            .into_iter()
            .enumerate()
            .map(|(i, u)| {
                let p = valid_pinned.get(i).copied().unwrap_or(false);
                (u, p)
            })
            .collect()
    };
    to_open.sort_by_key(|(_, pinned)| !*pinned);

    let bounded_active = if to_open.is_empty() {
        0
    } else if active_idx < to_open.len() {
        active_idx
    } else {
        0
    };

    (to_open, bounded_active)
}

/// Adversarial assertion helper validating strict partition invariant and stability
fn assert_restoration_invariants(
    to_open: &[(String, bool)],
    original_urls: &[String],
    original_pinned: &[bool],
    bounded_active: usize,
) {
    assert!(!to_open.is_empty(), "Restored tabs must never be empty");
    assert!(to_open.len() <= 100, "Restored tabs must not exceed 100");
    assert!(
        bounded_active < to_open.len(),
        "Active index must always be strictly within bounds"
    );

    // 1. Strict Partition Invariant:
    // Once an unpinned tab appears, NO pinned tab may ever appear thereafter
    let mut seen_unpinned = false;
    for (idx, (url, pinned)) in to_open.iter().enumerate() {
        if *pinned {
            assert!(
                !seen_unpinned,
                "Partition Invariant VIOLATED at index {}: Pinned tab '{}' follows unpinned tab! Restored list: {:?}",
                idx, url, to_open
            );
        } else {
            seen_unpinned = true;
        }
    }

    // 2. Filter original inputs to expected valid items (up to 100)
    let mut expected_valid_pairs = Vec::new();
    for (i, url) in original_urls.iter().enumerate() {
        if allowed_navigation(url) && expected_valid_pairs.len() < 100 {
            expected_valid_pairs.push((
                url.clone(),
                original_pinned.get(i).copied().unwrap_or(false),
            ));
        }
    }

    if expected_valid_pairs.is_empty() {
        // Fallback case: exactly 1 newtab unpinned
        assert_eq!(to_open.len(), 1);
        assert_eq!(to_open[0].0, NEW_TAB);
        assert!(!to_open[0].1);
    } else {
        // 3. Stable Partition Invariant: Relative order of pinned tabs preserved
        let restored_pinned: Vec<&str> = to_open
            .iter()
            .filter(|(_, p)| *p)
            .map(|(u, _)| u.as_str())
            .collect();
        let expected_pinned: Vec<&str> = expected_valid_pairs
            .iter()
            .filter(|(_, p)| *p)
            .map(|(u, _)| u.as_str())
            .collect();
        assert_eq!(
            restored_pinned, expected_pinned,
            "Stable sort failed to preserve relative ordering of pinned tabs"
        );

        // 4. Stable Partition Invariant: Relative order of unpinned tabs preserved
        let restored_unpinned: Vec<&str> = to_open
            .iter()
            .filter(|(_, p)| !*p)
            .map(|(u, _)| u.as_str())
            .collect();
        let expected_unpinned: Vec<&str> = expected_valid_pairs
            .iter()
            .filter(|(_, p)| !*p)
            .map(|(u, _)| u.as_str())
            .collect();
        assert_eq!(
            restored_unpinned, expected_unpinned,
            "Stable sort failed to preserve relative ordering of unpinned tabs"
        );
    }
}

#[test]
fn test_adversarial_alternating_pinned_unpinned_tabs_50_and_100() {
    // 50 tabs alternating: P, U, P, U, ...
    let urls_50: Vec<String> = (0..50).map(|i| format!("https://tab-{}.org", i)).collect();
    let pinned_50: Vec<bool> = (0..50).map(|i| i % 2 == 0).collect();
    let (to_open_50, active_50) = simulate_session_restore(&urls_50, &pinned_50, 25);
    assert_eq!(to_open_50.len(), 50);
    assert_eq!(active_50, 25);
    assert_restoration_invariants(&to_open_50, &urls_50, &pinned_50, active_50);

    let pinned_count_50 = to_open_50.iter().filter(|(_, p)| *p).count();
    let unpinned_count_50 = to_open_50.iter().filter(|(_, p)| !*p).count();
    assert_eq!(pinned_count_50, 25);
    assert_eq!(unpinned_count_50, 25);
    assert!(to_open_50[..25].iter().all(|(_, p)| *p));
    assert!(to_open_50[25..].iter().all(|(_, p)| !*p));

    // 100 tabs alternating: U, P, U, P, ...
    let urls_100: Vec<String> = (0..100).map(|i| format!("https://tab-{}.org", i)).collect();
    let pinned_100: Vec<bool> = (0..100).map(|i| i % 2 == 1).collect();
    let (to_open_100, active_100) = simulate_session_restore(&urls_100, &pinned_100, 99);
    assert_eq!(to_open_100.len(), 100);
    assert_eq!(active_100, 99);
    assert_restoration_invariants(&to_open_100, &urls_100, &pinned_100, active_100);

    let pinned_count_100 = to_open_100.iter().filter(|(_, p)| *p).count();
    let unpinned_count_100 = to_open_100.iter().filter(|(_, p)| !*p).count();
    assert_eq!(pinned_count_100, 50);
    assert_eq!(unpinned_count_100, 50);
    assert!(to_open_100[..50].iter().all(|(_, p)| *p));
    assert!(to_open_100[50..].iter().all(|(_, p)| !*p));
}

#[test]
fn test_adversarial_inverted_arrays_unpinned_precede_pinned() {
    // Scenario 1: 50 unpinned followed by 50 pinned
    let mut urls = Vec::new();
    let mut pinned = Vec::new();
    for i in 0..50 {
        urls.push(format!("https://unpinned-{}.org", i));
        pinned.push(false);
    }
    for i in 0..50 {
        urls.push(format!("https://pinned-{}.org", i));
        pinned.push(true);
    }
    let (to_open, active) = simulate_session_restore(&urls, &pinned, 75);
    assert_eq!(to_open.len(), 100);
    assert_restoration_invariants(&to_open, &urls, &pinned, active);
    // Pinned must now be first 50
    assert!(
        to_open[..50]
            .iter()
            .all(|(u, p)| *p && u.starts_with("https://pinned-"))
    );
    assert!(
        to_open[50..]
            .iter()
            .all(|(u, p)| !*p && u.starts_with("https://unpinned-"))
    );

    // Scenario 2: 99 unpinned followed by exactly 1 pinned tab at the very end
    let mut urls_99_1 = Vec::new();
    let mut pinned_99_1 = Vec::new();
    for i in 0..99 {
        urls_99_1.push(format!("https://unpinned-{}.org", i));
        pinned_99_1.push(false);
    }
    urls_99_1.push("https://solitary-pinned.org".into());
    pinned_99_1.push(true);

    let (to_open_99_1, active_99_1) = simulate_session_restore(&urls_99_1, &pinned_99_1, 0);
    assert_eq!(to_open_99_1.len(), 100);
    assert_restoration_invariants(&to_open_99_1, &urls_99_1, &pinned_99_1, active_99_1);
    assert_eq!(to_open_99_1[0].0, "https://solitary-pinned.org");
    assert!(to_open_99_1[0].1);
    assert!(to_open_99_1[1..].iter().all(|(_, p)| !*p));

    // Scenario 3: 1 unpinned followed by 99 pinned tabs
    let mut urls_1_99 = vec!["https://first-unpinned.org".into()];
    let mut pinned_1_99 = vec![false];
    for i in 0..99 {
        urls_1_99.push(format!("https://pinned-{}.org", i));
        pinned_1_99.push(true);
    }
    let (to_open_1_99, active_1_99) = simulate_session_restore(&urls_1_99, &pinned_1_99, 10);
    assert_eq!(to_open_1_99.len(), 100);
    assert_restoration_invariants(&to_open_1_99, &urls_1_99, &pinned_1_99, active_1_99);
    assert!(to_open_1_99[..99].iter().all(|(_, p)| *p));
    assert_eq!(to_open_1_99[99].0, "https://first-unpinned.org");
    assert!(!to_open_1_99[99].1);
}

#[test]
fn test_adversarial_corrupted_pinned_array_lengths() {
    let urls_80: Vec<String> = (0..80).map(|i| format!("https://tab-{}.org", i)).collect();

    // 1. Pinned array is completely empty
    let empty_pinned: Vec<bool> = vec![];
    let (to_open_empty, _) = simulate_session_restore(&urls_80, &empty_pinned, 0);
    assert_eq!(to_open_empty.len(), 80);
    assert!(to_open_empty.iter().all(|(_, p)| !*p));
    assert_restoration_invariants(&to_open_empty, &urls_80, &empty_pinned, 0);

    // 2. Pinned array shorter (e.g. 5 booleans for 80 URLs)
    let short_pinned = vec![true, false, true, true, false];
    let (to_open_short, _) = simulate_session_restore(&urls_80, &short_pinned, 0);
    assert_eq!(to_open_short.len(), 80);
    // Exactly 3 pinned tabs (indices 0, 2, 3), rest are unpinned
    assert_eq!(to_open_short.iter().filter(|(_, p)| *p).count(), 3);
    assert_eq!(to_open_short[0].0, "https://tab-0.org");
    assert_eq!(to_open_short[1].0, "https://tab-2.org");
    assert_eq!(to_open_short[2].0, "https://tab-3.org");
    assert!(to_open_short[..3].iter().all(|(_, p)| *p));
    assert!(to_open_short[3..].iter().all(|(_, p)| !*p));
    assert_restoration_invariants(&to_open_short, &urls_80, &short_pinned, 0);

    // 3. Pinned array much longer (1,000 booleans for 10 URLs)
    let urls_10: Vec<String> = (0..10).map(|i| format!("https://tab-{}.org", i)).collect();
    let long_pinned: Vec<bool> = (0..1000).map(|i| i % 3 == 0).collect();
    let (to_open_long, _) = simulate_session_restore(&urls_10, &long_pinned, 0);
    assert_eq!(to_open_long.len(), 10);
    assert_restoration_invariants(&to_open_long, &urls_10, &long_pinned, 0);
    // In URLs 0..10, indices 0, 3, 6, 9 are true (4 pinned tabs)
    assert_eq!(to_open_long.iter().filter(|(_, p)| *p).count(), 4);
    assert!(to_open_long[..4].iter().all(|(_, p)| *p));
    assert!(to_open_long[4..].iter().all(|(_, p)| !*p));
}

#[test]
fn test_adversarial_empty_and_single_tab_sessions() {
    // 1. Both URLs and pinned are completely empty
    let empty_urls: Vec<String> = vec![];
    let empty_pinned: Vec<bool> = vec![];
    let (to_open_empty, active_empty) = simulate_session_restore(&empty_urls, &empty_pinned, 5);
    assert_eq!(to_open_empty.len(), 1);
    assert_eq!(to_open_empty[0].0, NEW_TAB);
    assert!(!to_open_empty[0].1);
    assert_eq!(active_empty, 0);
    assert_restoration_invariants(&to_open_empty, &empty_urls, &empty_pinned, active_empty);

    // 2. URLs with only invalid / malicious schemes
    let disallowed_urls = vec![
        "javascript:alert(1)".into(),
        "custom-protocol://secrets".into(),
        "data:text/html,malicious".into(),
        "vbscript:msgbox(1)".into(),
    ];
    let disallowed_pinned = vec![true, true, true, true];
    let (to_open_disallowed, active_disallowed) =
        simulate_session_restore(&disallowed_urls, &disallowed_pinned, 2);
    assert_eq!(to_open_disallowed.len(), 1);
    assert_eq!(to_open_disallowed[0].0, NEW_TAB);
    assert!(!to_open_disallowed[0].1);
    assert_eq!(active_disallowed, 0);
    assert_restoration_invariants(
        &to_open_disallowed,
        &disallowed_urls,
        &disallowed_pinned,
        active_disallowed,
    );

    // 3. Single tab: pinned true
    let single_pinned_url = vec!["https://example.com/single".into()];
    let single_pinned_flag = vec![true];
    let (to_open_sp, active_sp) =
        simulate_session_restore(&single_pinned_url, &single_pinned_flag, 0);
    assert_eq!(to_open_sp.len(), 1);
    assert_eq!(to_open_sp[0].0, "https://example.com/single");
    assert!(to_open_sp[0].1);
    assert_eq!(active_sp, 0);
    assert_restoration_invariants(
        &to_open_sp,
        &single_pinned_url,
        &single_pinned_flag,
        active_sp,
    );

    // 4. Single tab: pinned false
    let single_unpinned_url = vec!["https://example.com/unpinned".into()];
    let single_unpinned_flag = vec![false];
    let (to_open_su, active_su) =
        simulate_session_restore(&single_unpinned_url, &single_unpinned_flag, 0);
    assert_eq!(to_open_su.len(), 1);
    assert_eq!(to_open_su[0].0, "https://example.com/unpinned");
    assert!(!to_open_su[0].1);
    assert_eq!(active_su, 0);
    assert_restoration_invariants(
        &to_open_su,
        &single_unpinned_url,
        &single_unpinned_flag,
        active_su,
    );

    // 5. Single tab: empty pinned flags array
    let (to_open_se, active_se) = simulate_session_restore(&single_unpinned_url, &empty_pinned, 0);
    assert_eq!(to_open_se.len(), 1);
    assert_eq!(to_open_se[0].0, "https://example.com/unpinned");
    assert!(!to_open_se[0].1);
    assert_eq!(active_se, 0);
    assert_restoration_invariants(&to_open_se, &single_unpinned_url, &empty_pinned, active_se);
}

#[test]
fn test_adversarial_100_tab_limit_boundary_conditions() {
    // 1. Exactly 99 tabs
    let urls_99: Vec<String> = (0..99).map(|i| format!("https://tab-{}.org", i)).collect();
    let pinned_99: Vec<bool> = (0..99).map(|i| i % 4 == 0).collect();
    let (to_open_99, active_99) = simulate_session_restore(&urls_99, &pinned_99, 50);
    assert_eq!(to_open_99.len(), 99);
    assert_eq!(active_99, 50);
    assert_restoration_invariants(&to_open_99, &urls_99, &pinned_99, active_99);

    // 2. Exactly 100 tabs
    let urls_100: Vec<String> = (0..100).map(|i| format!("https://tab-{}.org", i)).collect();
    let pinned_100: Vec<bool> = (0..100).map(|i| i % 5 == 0).collect();
    let (to_open_100, active_100) = simulate_session_restore(&urls_100, &pinned_100, 99);
    assert_eq!(to_open_100.len(), 100);
    assert_eq!(active_100, 99);
    assert_restoration_invariants(&to_open_100, &urls_100, &pinned_100, active_100);

    // 3. Exactly 101 tabs (tab 100 is truncated)
    let urls_101: Vec<String> = (0..101).map(|i| format!("https://tab-{}.org", i)).collect();
    let mut pinned_101: Vec<bool> = vec![false; 101];
    pinned_101[100] = true; // Tab 100 is pinned, but outside the 100 limit
    let (to_open_101, active_101) = simulate_session_restore(&urls_101, &pinned_101, 100);
    assert_eq!(to_open_101.len(), 100);
    // Tab 100 was truncated, so none of the 100 restored tabs are pinned
    assert!(to_open_101.iter().all(|(_, p)| !*p));
    // active was 100, which is >= to_open.len() (100), so bounded_active resets safely to 0
    assert_eq!(active_101, 0);
    assert_restoration_invariants(&to_open_101, &urls_101, &pinned_101, active_101);

    // 4. 200 tabs with mixed valid/invalid URLs
    let mut urls_200 = Vec::new();
    let mut pinned_200 = Vec::new();
    for i in 0..200 {
        if i % 2 == 0 {
            urls_200.push("javascript:alert(0)".into());
            pinned_200.push(true);
        } else {
            urls_200.push(format!("https://legit-{}.org", i));
            pinned_200.push(i % 4 == 1);
        }
    }
    let (to_open_200, active_200) = simulate_session_restore(&urls_200, &pinned_200, 9999);
    assert_eq!(to_open_200.len(), 100);
    assert_eq!(active_200, 0);
    assert_restoration_invariants(&to_open_200, &urls_200, &pinned_200, active_200);
}

#[test]
fn test_adversarial_monte_carlo_fuzzing_permutations_1000_runs() {
    struct SimpleRng(u64);
    impl SimpleRng {
        fn new(seed: u64) -> Self {
            Self(seed)
        }
        fn next_u32(&mut self) -> u32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
            (self.0 >> 32) as u32
        }
        fn next_range(&mut self, min: usize, max: usize) -> usize {
            if min >= max {
                return min;
            }
            min + (self.next_u32() as usize % (max - min + 1))
        }
    }

    let mut rng = SimpleRng::new(0xCAFE_BABE_1234_5678);

    for run in 0..1000 {
        let url_count = rng.next_range(0, 150);
        let pinned_count = rng.next_range(0, 200);
        let active_raw = rng.next_range(0, 500);

        let mut urls = Vec::with_capacity(url_count);
        for i in 0..url_count {
            let roll = rng.next_range(0, 10);
            if roll == 0 {
                urls.push(format!("javascript:evil_{}()", i));
            } else if roll == 1 {
                urls.push("data:text/html,bad".into());
            } else {
                urls.push(format!("https://host-{}.example.com/path/{}", roll, i));
            }
        }

        let mut pinned = Vec::with_capacity(pinned_count);
        let pinned_prob = rng.next_range(0, 100) as u32;
        for _ in 0..pinned_count {
            pinned.push((rng.next_u32() % 100) < pinned_prob);
        }

        let (to_open, bounded_active) = simulate_session_restore(&urls, &pinned, active_raw);

        // Core invariant checks
        assert_restoration_invariants(&to_open, &urls, &pinned, bounded_active);

        // Additional sanity checks
        assert!(!to_open.is_empty(), "Run {} failed: empty to_open", run);
        assert!(to_open.len() <= 100, "Run {} failed: exceeded 100", run);
        assert!(
            bounded_active < to_open.len(),
            "Run {} failed: active out of bounds",
            run
        );
    }
}

#[test]
fn test_saved_state_json_corrupted_session_deserialization_roundtrip() {
    // 1. JSON with alternating pinned booleans and reversed active
    let json_corrupted = r#"{
        "preferences": {
            "engine": "duckduckgo",
            "zoom": 100,
            "theme": "system",
            "startup": "restore",
            "show_bookmarks": true,
            "adblock": true,
            "block_popups": true,
            "allowed_hosts": [],
            "blocked_hosts": [],
            "download_dir": ""
        },
        "bookmarks": [],
        "history": [],
        "passwords": [],
        "restored_urls": [
            "https://u1.com",
            "https://p1.com",
            "https://u2.com",
            "https://p2.com",
            "https://u3.com"
        ],
        "restored_active": 4,
        "last_session_urls": [
            "https://u1.com",
            "https://p1.com",
            "https://u2.com",
            "https://p2.com",
            "https://u3.com"
        ],
        "last_session_active": 4,
        "last_session_pinned": [false, true, false, true, false]
    }"#;

    let saved: SavedState = serde_json::from_str(json_corrupted).expect("Deserialization failed");
    let (to_open, active) = simulate_session_restore(
        &saved.last_session_urls,
        &saved.last_session_pinned,
        saved.last_session_active,
    );

    assert_eq!(to_open.len(), 5);
    assert_eq!(to_open[0].0, "https://p1.com");
    assert!(to_open[0].1);
    assert_eq!(to_open[1].0, "https://p2.com");
    assert!(to_open[1].1);
    assert_eq!(to_open[2].0, "https://u1.com");
    assert!(!to_open[2].1);
    assert_eq!(to_open[3].0, "https://u2.com");
    assert!(!to_open[3].1);
    assert_eq!(to_open[4].0, "https://u3.com");
    assert!(!to_open[4].1);
    assert_eq!(active, 4);
    assert_restoration_invariants(
        &to_open,
        &saved.last_session_urls,
        &saved.last_session_pinned,
        active,
    );

    // 2. JSON where last_session_pinned is completely missing (omitted field)
    let json_no_pinned = r#"{
        "restored_urls": ["https://a.com", "https://b.com"],
        "last_session_urls": ["https://a.com", "https://b.com"]
    }"#;
    let saved_no_pinned: SavedState = serde_json::from_str(json_no_pinned)
        .expect("Deserialization with missing pinned field failed");
    assert!(saved_no_pinned.last_session_pinned.is_empty());
    let (to_open_np, active_np) = simulate_session_restore(
        &saved_no_pinned.last_session_urls,
        &saved_no_pinned.last_session_pinned,
        saved_no_pinned.last_session_active,
    );
    assert_eq!(to_open_np.len(), 2);
    assert!(to_open_np.iter().all(|(_, p)| !*p));
    assert_eq!(active_np, 0);
    assert_restoration_invariants(
        &to_open_np,
        &saved_no_pinned.last_session_urls,
        &saved_no_pinned.last_session_pinned,
        active_np,
    );
}
