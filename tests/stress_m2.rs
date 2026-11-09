#![allow(dead_code, unused_imports)]

#[path = "../src/model.rs"]
mod model;

use model::{
    BOOKMARKS, Command, DOWNLOADS, EXTENSIONS, HISTORY, NEW_TAB, PASSWORDS, Preferences, SETTINGS,
    SavedState, TabInfo, parse_command,
};

// ============================================================================
// SIMULATION HELPERS FOR TAB STATE MACHINE & ORACLE
// ============================================================================

#[derive(Clone, Debug, PartialEq, Eq)]
struct MockTab {
    id: u64,
    url: String,
    title: String,
    pinned: bool,
    muted: bool,
}

struct TabManager {
    tabs: Vec<MockTab>,
    closed_tabs: Vec<(String, String)>,
    active: u64,
    next_id: u64,
}

impl TabManager {
    fn new() -> Self {
        Self {
            tabs: vec![],
            closed_tabs: vec![],
            active: 0,
            next_id: 1,
        }
    }

    fn try_new_tab(&mut self, url: &str, select: bool) -> Result<u64, String> {
        if self.tabs.len() >= 100 {
            return Err("Đã đạt giới hạn 100 tab. Hãy đóng bớt tab trước.".into());
        }
        let id = self.next_id;
        self.next_id += 1;
        self.tabs.push(MockTab {
            id,
            url: url.to_string(),
            title: format!("Tab {id}"),
            pinned: false,
            muted: false,
        });
        if select || self.active == 0 {
            self.active = id;
        }
        Ok(id)
    }

    fn new_tab(&mut self, url: &str, select: bool) -> u64 {
        self.try_new_tab(url, select).unwrap_or(0)
    }

    fn record_closed_tab(&mut self, url: &str, title: &str) {
        if !url.starts_with("papyrus:") && url != "about:blank" && !url.is_empty() {
            self.closed_tabs.push((url.to_string(), title.to_string()));
            if self.closed_tabs.len() > 50 {
                self.closed_tabs.remove(0);
            }
        }
    }

    fn close_tab(&mut self, mut id: u64) {
        if id == 0 {
            id = self.active;
        }
        if let Some(pos) = self.tabs.iter().position(|t| t.id == id) {
            let url = self.tabs[pos].url.clone();
            let title = self.tabs[pos].title.clone();
            self.record_closed_tab(&url, &title);
            let next = model::next_active_after_close(
                &self.tabs.iter().map(|t| t.id).collect::<Vec<_>>(),
                self.active,
                id,
            );
            self.tabs.retain(|t| t.id != id);
            if let Some(next) = next {
                self.active = next;
            } else if !self.tabs.is_empty() {
                self.active = self.tabs[0].id;
            } else {
                self.active = 0;
            }
        }
    }

    /// Exact implementation of Command::PinTab from src/main.rs:1088-1107
    fn pin_tab(&mut self, id: u64) {
        let target_id = if id == 0 { self.active } else { id };
        if let Some(pos) = self.tabs.iter().position(|t| t.id == target_id) {
            let is_now_pinned = !self.tabs[pos].pinned;
            self.tabs[pos].pinned = is_now_pinned;

            let other_pinned_count = self
                .tabs
                .iter()
                .enumerate()
                .filter(|&(i, t)| i != pos && t.pinned)
                .count();

            let tab = self.tabs.remove(pos);
            self.tabs.insert(other_pinned_count, tab);
        }
    }

    /// Exact implementation of Action 4 (Close other tabs) from src/main.rs:1320-1335
    fn close_other_tabs(&mut self, id: u64) {
        let to_record: Vec<(String, String)> = self
            .tabs
            .iter()
            .filter(|t| t.id != id && !t.pinned)
            .map(|t| (t.url.clone(), t.title.clone()))
            .collect();
        for (url, title) in to_record {
            self.record_closed_tab(&url, &title);
        }
        self.active = id;
        self.tabs.retain(|t| t.id == id || t.pinned);
    }

    /// Exact implementation of Action 5 (Close tabs to right) from src/main.rs:1336-1364
    fn close_tabs_to_right(&mut self, id: u64) {
        let tab_pos = self.tabs.iter().position(|t| t.id == id);
        if let Some(pos) = tab_pos {
            let to_record: Vec<(String, String)> = self
                .tabs
                .iter()
                .skip(pos + 1)
                .filter(|t| !t.pinned)
                .map(|t| (t.url.clone(), t.title.clone()))
                .collect();
            for (url, title) in to_record {
                self.record_closed_tab(&url, &title);
            }
            let to_close: Vec<u64> = self
                .tabs
                .iter()
                .skip(pos + 1)
                .filter(|t| !t.pinned)
                .map(|t| t.id)
                .collect();
            if to_close.contains(&self.active) {
                self.active = id;
            }
            self.tabs.retain(|t| !to_close.contains(&t.id));
        }
    }

    /// Exact implementation of Command::DuplicateTab from src/main.rs:1119-1138
    fn try_duplicate_tab(&mut self, id: u64) -> Result<u64, String> {
        let target_id = if id == 0 { self.active } else { id };
        if let Some(source_idx) = self.tabs.iter().position(|t| t.id == target_id) {
            let url = self.tabs[source_idx].url.clone();
            let is_pinned = self.tabs[source_idx].pinned;
            let new_id = self.try_new_tab(&url, true)?;
            if let Some(created_idx) = self.tabs.iter().position(|t| t.id == new_id) {
                let mut tab = self.tabs.remove(created_idx);
                if is_pinned {
                    tab.pinned = true;
                }
                let insert_idx = (source_idx + 1).min(self.tabs.len());
                self.tabs.insert(insert_idx, tab);
            }
            self.active = new_id;
            Ok(new_id)
        } else {
            Ok(0)
        }
    }

    fn duplicate_tab(&mut self, id: u64) -> u64 {
        self.try_duplicate_tab(id).unwrap_or(0)
    }

    /// Exact implementation of Command::ReorderTab from src/main.rs:1396-1408
    fn reorder_tab(&mut self, from: u64, to: u64) -> bool {
        if from != to {
            let from_pos = self.tabs.iter().position(|t| t.id == from);
            let to_pos = self.tabs.iter().position(|t| t.id == to);
            if let (Some(f), Some(t)) = (from_pos, to_pos)
                && self.tabs[f].pinned == self.tabs[t].pinned
            {
                let tab = self.tabs.remove(f);
                self.tabs.insert(t, tab);
                return true;
            }
        }
        false
    }

    /// Reopen / restore tab from closed_tabs stack (src/main.rs:1079-1087)
    fn reopen_tab(&mut self) -> Option<u64> {
        if let Some((url, _)) = self.closed_tabs.pop() {
            Some(self.new_tab(&url, true))
        } else {
            None
        }
    }

    /// Invariant check: all pinned tabs MUST precede all unpinned tabs.
    /// Returns Ok(()) if valid, or Err(violation_description) if violated.
    fn verify_pinned_invariant(&self) -> Result<(), String> {
        let mut seen_unpinned = false;
        for (idx, tab) in self.tabs.iter().enumerate() {
            if tab.pinned {
                if seen_unpinned {
                    return Err(format!(
                        "Invariant Violated: Pinned tab id={} found at index {} AFTER unpinned tabs! Current layout: {:?}",
                        tab.id,
                        idx,
                        self.tabs
                            .iter()
                            .map(|t| (t.id, t.pinned))
                            .collect::<Vec<_>>()
                    ));
                }
            } else {
                seen_unpinned = true;
            }
        }
        Ok(())
    }
}

// ============================================================================
// SUITE 1: IPC Payload Security (M2 Commands)
// ============================================================================

#[test]
fn test_ipc_m2_commands_valid_parsing() {
    let valid_cases = [
        (
            "pin_tab",
            r#"{"action":"pin_tab","id":1}"#,
            Command::PinTab { id: 1 },
        ),
        (
            "pin_tab_zero",
            r#"{"action":"pin_tab","id":0}"#,
            Command::PinTab { id: 0 },
        ),
        (
            "pin_tab_max",
            r#"{"action":"pin_tab","id":18446744073709551615}"#,
            Command::PinTab {
                id: 18446744073709551615,
            },
        ),
        (
            "toggle_mute_tab",
            r#"{"action":"toggle_mute_tab","id":42}"#,
            Command::ToggleMuteTab { id: 42 },
        ),
        (
            "toggle_mute_tab_zero",
            r#"{"action":"toggle_mute_tab","id":0}"#,
            Command::ToggleMuteTab { id: 0 },
        ),
        (
            "duplicate_tab",
            r#"{"action":"duplicate_tab","id":100}"#,
            Command::DuplicateTab { id: 100 },
        ),
        (
            "duplicate_tab_zero",
            r#"{"action":"duplicate_tab","id":0}"#,
            Command::DuplicateTab { id: 0 },
        ),
        (
            "reopen_tab",
            r#"{"action":"reopen_tab"}"#,
            Command::ReopenTab,
        ),
        (
            "restore_tab",
            r#"{"action":"restore_tab"}"#,
            Command::RestoreTab,
        ),
    ];

    for (name, payload, expected) in valid_cases {
        let parsed = parse_command("about:blank", payload);
        assert!(
            parsed.is_some(),
            "Valid payload for {name} failed to parse: {payload}"
        );
        match (parsed.unwrap(), expected) {
            (Command::PinTab { id: a }, Command::PinTab { id: b }) => assert_eq!(a, b),
            (Command::ToggleMuteTab { id: a }, Command::ToggleMuteTab { id: b }) => {
                assert_eq!(a, b)
            }
            (Command::DuplicateTab { id: a }, Command::DuplicateTab { id: b }) => {
                assert_eq!(a, b)
            }
            (Command::ReopenTab, Command::ReopenTab) => {}
            (Command::RestoreTab, Command::RestoreTab) => {}
            (other, exp) => panic!("Mismatch for {name}: got {other:?}, expected {exp:?}"),
        }
    }
}

#[test]
fn test_ipc_m2_extra_field_injection_strictly_rejected() {
    let injected_cases = [
        // Extra admin/privilege flags
        r#"{"action":"pin_tab","id":1,"admin":true}"#,
        r#"{"action":"pin_tab","id":1,"is_admin":1}"#,
        r#"{"action":"pin_tab","id":1,"extra":"payload"}"#,
        r#"{"action":"toggle_mute_tab","id":1,"mute":true}"#,
        r#"{"action":"toggle_mute_tab","id":1,"volume":100}"#,
        r#"{"action":"duplicate_tab","id":1,"times":5}"#,
        r#"{"action":"duplicate_tab","id":1,"target":"new_window"}"#,
        r#"{"action":"reopen_tab","id":1}"#,
        r#"{"action":"reopen_tab","url":"https://evil.com"}"#,
        r#"{"action":"reopen_tab","admin":true}"#,
        r#"{"action":"restore_tab","id":1}"#,
        r#"{"action":"restore_tab","url":"https://evil.com"}"#,
        r#"{"action":"restore_tab","force":true}"#,
        // Prototype pollution vectors
        r#"{"action":"pin_tab","id":1,"__proto__":{"polluted":true}}"#,
        r#"{"action":"toggle_mute_tab","id":1,"constructor":{}}"#,
        r#"{"action":"reopen_tab","__proto__":{}}"#,
        r#"{"action":"restore_tab","prototype":{}}"#,
    ];

    for payload in injected_cases {
        assert!(
            parse_command("about:blank", payload).is_none(),
            "Injected payload MUST be rejected: {payload}"
        );
    }
}

#[test]
fn test_ipc_m2_negative_overflow_and_type_confusion_rejected() {
    let invalid_id_cases = [
        // Negative IDs
        r#"{"action":"pin_tab","id":-1}"#,
        r#"{"action":"pin_tab","id":-999999999999}"#,
        r#"{"action":"toggle_mute_tab","id":-1}"#,
        r#"{"action":"toggle_mute_tab","id":-42}"#,
        r#"{"action":"duplicate_tab","id":-1}"#,
        r#"{"action":"duplicate_tab","id":-100}"#,
        // Overflow IDs (beyond u64::MAX)
        r#"{"action":"pin_tab","id":18446744073709551616}"#,
        r#"{"action":"pin_tab","id":9999999999999999999999999999999999999999}"#,
        r#"{"action":"toggle_mute_tab","id":18446744073709551616}"#,
        r#"{"action":"duplicate_tab","id":18446744073709551616}"#,
        // Type confusion: String instead of u64
        r#"{"action":"pin_tab","id":"1"}"#,
        r#"{"action":"pin_tab","id":"0"}"#,
        r#"{"action":"toggle_mute_tab","id":"42"}"#,
        r#"{"action":"duplicate_tab","id":"10"}"#,
        // Type confusion: Float instead of u64
        r#"{"action":"pin_tab","id":1.5}"#,
        r#"{"action":"pin_tab","id":1.0}"#,
        r#"{"action":"toggle_mute_tab","id":2.5}"#,
        r#"{"action":"duplicate_tab","id":3.14159}"#,
        // Type confusion: Null / Bool / Array / Object
        r#"{"action":"pin_tab","id":null}"#,
        r#"{"action":"pin_tab","id":true}"#,
        r#"{"action":"pin_tab","id":false}"#,
        r#"{"action":"pin_tab","id":[1]}"#,
        r#"{"action":"pin_tab","id":{"val":1}}"#,
        r#"{"action":"toggle_mute_tab","id":null}"#,
        r#"{"action":"toggle_mute_tab","id":[]}"#,
        r#"{"action":"duplicate_tab","id":null}"#,
        r#"{"action":"duplicate_tab","id":{}}"#,
        // Missing required 'id' parameter
        r#"{"action":"pin_tab"}"#,
        r#"{"action":"toggle_mute_tab"}"#,
        r#"{"action":"duplicate_tab"}"#,
    ];

    for payload in invalid_id_cases {
        assert!(
            parse_command("about:blank", payload).is_none(),
            "Invalid ID payload MUST be rejected: {payload}"
        );
    }
}

// ============================================================================
// SUITE 2: Tab Pinning & Unpinning State Transitions & Invariant Verification
// ============================================================================

#[test]
fn test_pin_tab_invariants_across_multiple_tabs() {
    let mut tm = TabManager::new();
    let id1 = tm.new_tab("https://example.com/1", true);
    let id2 = tm.new_tab("https://example.com/2", false);
    let id3 = tm.new_tab("https://example.com/3", false);
    let id4 = tm.new_tab("https://example.com/4", false);
    let id5 = tm.new_tab("https://example.com/5", false);

    // Initial state: 5 unpinned tabs
    assert_eq!(tm.tabs.len(), 5);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 1. Pin middle tab (id3)
    tm.pin_tab(id3);
    assert!(tm.verify_pinned_invariant().is_ok());
    assert_eq!(tm.tabs[0].id, id3);
    assert!(tm.tabs[0].pinned);

    // 2. Pin last tab (id5)
    tm.pin_tab(id5);
    assert!(tm.verify_pinned_invariant().is_ok());
    assert_eq!(tm.tabs[0].id, id3);
    assert_eq!(tm.tabs[1].id, id5);
    assert!(tm.tabs[0].pinned);
    assert!(tm.tabs[1].pinned);
    assert!(!tm.tabs[2].pinned);

    // 3. Pin first unpinned tab (id1)
    tm.pin_tab(id1);
    assert!(tm.verify_pinned_invariant().is_ok());
    assert_eq!(tm.tabs[0].id, id3);
    assert_eq!(tm.tabs[1].id, id5);
    assert_eq!(tm.tabs[2].id, id1);
    assert!(tm.tabs[0].pinned);
    assert!(tm.tabs[1].pinned);
    assert!(tm.tabs[2].pinned);

    // 4. Pin remaining unpinned tabs (id2, id4) -> all tabs pinned
    tm.pin_tab(id2);
    tm.pin_tab(id4);
    assert!(tm.verify_pinned_invariant().is_ok());
    assert!(tm.tabs.iter().all(|t| t.pinned));

    // 5. Unpin first pinned tab (id3)
    tm.pin_tab(id3);
    assert!(tm.verify_pinned_invariant().is_ok());
    // id3 should now be unpinned and placed after all remaining 4 pinned tabs
    assert!(!tm.tabs.iter().find(|t| t.id == id3).unwrap().pinned);
    assert_eq!(tm.tabs.last().unwrap().id, id3);

    // 6. Unpin middle pinned tab (id1)
    tm.pin_tab(id1);
    assert!(tm.verify_pinned_invariant().is_ok());
    assert!(!tm.tabs.iter().find(|t| t.id == id1).unwrap().pinned);

    // 7. Unpin all remaining tabs
    for tab_id in [id5, id2, id4] {
        tm.pin_tab(tab_id);
        assert!(tm.verify_pinned_invariant().is_ok());
    }
    assert!(tm.tabs.iter().all(|t| !t.pinned));
}

#[test]
fn test_pin_tab_randomized_stress_fuzzing_10000_operations() {
    let mut tm = TabManager::new();
    let count = 20;
    let mut ids = Vec::new();
    for i in 0..count {
        ids.push(tm.new_tab(&format!("https://example.org/{i}"), false));
    }

    // Pseudo-random LCG generator for deterministic stress testing
    let mut lcg_state: u64 = 0x12345678abcdef01;
    let mut rand = || -> u64 {
        lcg_state = lcg_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        lcg_state
    };

    // Run 10,000 randomized pin/unpin operations
    for op in 0..10_000 {
        let target_idx = (rand() as usize) % ids.len();
        let target_id = ids[target_idx];

        tm.pin_tab(target_id);

        if let Err(err) = tm.verify_pinned_invariant() {
            panic!("Step {op} FAILED during randomized PinTab fuzzing: {err}");
        }
    }
}

// ============================================================================
// SUITE 3: Batch Close Operations Accidental Protection Stress Tests
// ============================================================================

#[test]
fn test_batch_close_others_never_closes_pinned_tabs() {
    let mut tm = TabManager::new();
    let p1 = tm.new_tab("https://pinned1.org", false);
    let p2 = tm.new_tab("https://pinned2.org", false);
    let p3 = tm.new_tab("https://pinned3.org", false);
    let _u1 = tm.new_tab("https://unpinned1.org", false);
    let u2 = tm.new_tab("https://unpinned2.org", false);
    let _u3 = tm.new_tab("https://unpinned3.org", false);

    tm.pin_tab(p1);
    tm.pin_tab(p2);
    tm.pin_tab(p3);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 3);

    // Scenario A: Close other tabs from an UNPINNED tab (u2)
    // Should close u1 and u3, but retain p1, p2, p3 and u2
    tm.close_other_tabs(u2);
    assert_eq!(tm.tabs.len(), 4);
    assert!(tm.tabs.iter().any(|t| t.id == p1 && t.pinned));
    assert!(tm.tabs.iter().any(|t| t.id == p2 && t.pinned));
    assert!(tm.tabs.iter().any(|t| t.id == p3 && t.pinned));
    assert!(tm.tabs.iter().any(|t| t.id == u2 && !t.pinned));
    assert!(tm.verify_pinned_invariant().is_ok());

    // Verify u1 and u3 were recorded to closed_tabs
    assert_eq!(tm.closed_tabs.len(), 2);
    assert_eq!(tm.closed_tabs[0].0, "https://unpinned1.org");
    assert_eq!(tm.closed_tabs[1].0, "https://unpinned3.org");

    // Add more unpinned tabs
    let _u4 = tm.new_tab("https://unpinned4.org", false);
    let _u5 = tm.new_tab("https://unpinned5.org", false);

    // Scenario B: Close other tabs from a PINNED tab (p1)
    // Should close all unpinned tabs (u2, u4, u5) while retaining all pinned tabs (p1, p2, p3)
    tm.close_other_tabs(p1);
    assert_eq!(tm.tabs.len(), 3);
    assert!(tm.tabs.iter().all(|t| t.pinned));
    assert_eq!(tm.tabs[0].id, p1);
    assert_eq!(tm.tabs[1].id, p2);
    assert_eq!(tm.tabs[2].id, p3);
    assert!(tm.verify_pinned_invariant().is_ok());
}

#[test]
fn test_batch_close_tabs_to_right_never_closes_pinned_tabs() {
    let mut tm = TabManager::new();
    let p1 = tm.new_tab("https://pinned1.org", false);
    let p2 = tm.new_tab("https://pinned2.org", false);
    let _u1 = tm.new_tab("https://unpinned1.org", false);
    let _u2 = tm.new_tab("https://unpinned2.org", false);
    let _u3 = tm.new_tab("https://unpinned3.org", false);

    tm.pin_tab(p1);
    tm.pin_tab(p2);

    // Scenario A: Close tabs to right of pinned tab p1
    // p2 is to the right of p1, BUT p2 is pinned, so it MUST NOT be closed!
    // u1, u2, u3 are unpinned to the right of p1, so they should be closed.
    tm.close_tabs_to_right(p1);

    assert_eq!(tm.tabs.len(), 2);
    assert!(tm.tabs.iter().any(|t| t.id == p1 && t.pinned));
    assert!(
        tm.tabs.iter().any(|t| t.id == p2 && t.pinned),
        "Pinned tab p2 to the right of p1 MUST NOT be closed!"
    );
    assert!(tm.verify_pinned_invariant().is_ok());
    assert_eq!(tm.closed_tabs.len(), 3); // u1, u2, u3 recorded

    // Scenario B: Close tabs to right when no tabs exist to right
    let u4 = tm.new_tab("https://unpinned4.org", false);
    tm.close_tabs_to_right(u4);
    assert_eq!(tm.tabs.len(), 3); // no tabs removed
}

// ============================================================================
// SUITE 4: Tab Duplication Invariant Stress Test (Critical Bug Discovery)
// ============================================================================

#[test]
fn test_duplicate_tab_unpinned_preserves_invariant() {
    let mut tm = TabManager::new();
    let p1 = tm.new_tab("https://pinned.org", false);
    tm.pin_tab(p1);

    let u1 = tm.new_tab("https://unpinned1.org", false);
    let u2 = tm.new_tab("https://unpinned2.org", false);

    // Duplicating unpinned tab u1
    let dup_id = tm.duplicate_tab(u1);
    assert!(dup_id > 0);
    assert!(tm.verify_pinned_invariant().is_ok());

    // Duplicated tab should be inserted right after u1 (index 2)
    assert_eq!(tm.tabs[1].id, u1);
    assert_eq!(tm.tabs[2].id, dup_id);
    assert_eq!(tm.tabs[3].id, u2);
}

#[test]
fn test_duplicate_pinned_tab_preserves_invariant_oracle() {
    let mut tm = TabManager::new();
    let p1 = tm.new_tab("https://pinned1.org", false);
    let p2 = tm.new_tab("https://pinned2.org", false);
    let u1 = tm.new_tab("https://unpinned1.org", false);

    tm.pin_tab(p1);
    tm.pin_tab(p2);

    assert!(tm.verify_pinned_invariant().is_ok());
    assert_eq!(tm.tabs[0].id, p1);
    assert_eq!(tm.tabs[1].id, p2);
    assert_eq!(tm.tabs[2].id, u1);

    // Duplicating pinned tab p1 (at index 0):
    // DuplicateTab creates a new pinned tab and inserts it at source_idx + 1 = 1.
    // Pinned tab p2 is shifted to index 2.
    // The invariant ("all pinned tabs precede all unpinned tabs") is strictly preserved.
    let dup_id = tm.duplicate_tab(p1);

    assert!(
        tm.verify_pinned_invariant().is_ok(),
        "DuplicateTab on pinned tab MUST preserve invariant! Got error: {:?}",
        tm.verify_pinned_invariant().err()
    );

    // Inspect the exact layout:
    assert_eq!(tm.tabs.len(), 4);
    assert_eq!(tm.tabs[0].id, p1);
    assert!(tm.tabs[0].pinned);

    assert_eq!(tm.tabs[1].id, dup_id);
    assert!(
        tm.tabs[1].pinned,
        "Duplicated tab from a pinned tab MUST inherit pinned: true"
    );

    assert_eq!(tm.tabs[2].id, p2);
    assert!(tm.tabs[2].pinned, "Pinned tab p2 remains pinned at index 2");

    assert_eq!(tm.tabs[3].id, u1);
    assert!(
        !tm.tabs[3].pinned,
        "Unpinned tab u1 remains unpinned at index 3"
    );

    // Strict invariant assertion across all tabs
    let pinned_count = tm.tabs.iter().filter(|t| t.pinned).count();
    assert_eq!(pinned_count, 3);
    assert!(tm.tabs.iter().take(pinned_count).all(|t| t.pinned));
    assert!(tm.tabs.iter().skip(pinned_count).all(|t| !t.pinned));

    assert_eq!(tm.active, dup_id);
}

#[test]
fn test_duplicate_only_pinned_tab_with_unpinned_tabs() {
    let mut tm = TabManager::new();
    let p1 = tm.new_tab("https://pinned-single.org", false);
    tm.pin_tab(p1);
    let u1 = tm.new_tab("https://unpinned-1.org", false);
    let u2 = tm.new_tab("https://unpinned-2.org", false);
    let u3 = tm.new_tab("https://unpinned-3.org", false);

    assert_eq!(tm.tabs.len(), 4);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 1);
    assert!(tm.verify_pinned_invariant().is_ok());

    let dup_id = tm.duplicate_tab(p1);
    assert!(dup_id > 0);
    assert!(tm.verify_pinned_invariant().is_ok());
    assert_eq!(tm.tabs.len(), 5);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 2);

    // Exact order check:
    // [p1 (pinned), dup_id (pinned), u1 (unpinned), u2 (unpinned), u3 (unpinned)]
    assert_eq!(tm.tabs[0].id, p1);
    assert!(tm.tabs[0].pinned);
    assert_eq!(tm.tabs[1].id, dup_id);
    assert!(tm.tabs[1].pinned);
    assert_eq!(tm.tabs[2].id, u1);
    assert!(!tm.tabs[2].pinned);
    assert_eq!(tm.tabs[3].id, u2);
    assert!(!tm.tabs[3].pinned);
    assert_eq!(tm.tabs[4].id, u3);
    assert!(!tm.tabs[4].pinned);

    // Active tab must be the newly created duplicate
    assert_eq!(tm.active, dup_id);
    assert_eq!(tm.tabs[1].url, "https://pinned-single.org");
}

#[test]
fn test_duplicate_first_pinned_tab_when_multiple_pinned_tabs_exist() {
    let mut tm = TabManager::new();
    let p1 = tm.new_tab("https://pinned-1.org", false);
    let p2 = tm.new_tab("https://pinned-2.org", false);
    let p3 = tm.new_tab("https://pinned-3.org", false);
    tm.pin_tab(p1);
    tm.pin_tab(p2);
    tm.pin_tab(p3);
    let u1 = tm.new_tab("https://unpinned-1.org", false);
    let u2 = tm.new_tab("https://unpinned-2.org", false);

    assert_eq!(tm.tabs.len(), 5);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 3);
    assert!(tm.verify_pinned_invariant().is_ok());

    // Duplicate first pinned tab (p1 at index 0)
    let dup_p1 = tm.duplicate_tab(p1);
    assert!(dup_p1 > 0);
    assert!(tm.verify_pinned_invariant().is_ok());
    assert_eq!(tm.tabs.len(), 6);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 4);

    // Expected order: [p1, dup_p1, p2, p3, u1, u2]
    assert_eq!(tm.tabs[0].id, p1);
    assert!(tm.tabs[0].pinned);
    assert_eq!(tm.tabs[1].id, dup_p1);
    assert!(tm.tabs[1].pinned);
    assert_eq!(tm.tabs[2].id, p2);
    assert!(tm.tabs[2].pinned);
    assert_eq!(tm.tabs[3].id, p3);
    assert!(tm.tabs[3].pinned);
    assert_eq!(tm.tabs[4].id, u1);
    assert!(!tm.tabs[4].pinned);
    assert_eq!(tm.tabs[5].id, u2);
    assert!(!tm.tabs[5].pinned);
    assert_eq!(tm.active, dup_p1);
}

#[test]
fn test_duplicate_last_pinned_tab_preserves_partition_boundary() {
    let mut tm = TabManager::new();
    let p1 = tm.new_tab("https://pinned-1.org", false);
    let p2 = tm.new_tab("https://pinned-2.org", false);
    let p3 = tm.new_tab("https://pinned-3.org", false);
    tm.pin_tab(p1);
    tm.pin_tab(p2);
    tm.pin_tab(p3);
    let u1 = tm.new_tab("https://unpinned-1.org", false);
    let u2 = tm.new_tab("https://unpinned-2.org", false);

    assert_eq!(tm.tabs[2].id, p3);
    assert!(tm.tabs[2].pinned);

    // Duplicate last pinned tab (p3 at index 2)
    let dup_p3 = tm.duplicate_tab(p3);
    assert!(dup_p3 > 0);
    assert!(tm.verify_pinned_invariant().is_ok());
    assert_eq!(tm.tabs.len(), 6);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 4);

    // Expected order: [p1, p2, p3, dup_p3, u1, u2]
    assert_eq!(tm.tabs[0].id, p1);
    assert!(tm.tabs[0].pinned);
    assert_eq!(tm.tabs[1].id, p2);
    assert!(tm.tabs[1].pinned);
    assert_eq!(tm.tabs[2].id, p3);
    assert!(tm.tabs[2].pinned);
    assert_eq!(tm.tabs[3].id, dup_p3);
    assert!(tm.tabs[3].pinned);
    assert_eq!(tm.tabs[4].id, u1);
    assert!(!tm.tabs[4].pinned);
    assert_eq!(tm.tabs[5].id, u2);
    assert!(!tm.tabs[5].pinned);
    assert_eq!(tm.active, dup_p3);

    // Also test duplicating when NO unpinned tabs exist
    let mut tm_only_pinned = TabManager::new();
    let op1 = tm_only_pinned.new_tab("https://op1.org", false);
    let op2 = tm_only_pinned.new_tab("https://op2.org", false);
    tm_only_pinned.pin_tab(op1);
    tm_only_pinned.pin_tab(op2);
    let dup_op2 = tm_only_pinned.duplicate_tab(op2);
    assert_eq!(tm_only_pinned.tabs.len(), 3);
    assert!(tm_only_pinned.tabs.iter().all(|t| t.pinned));
    assert_eq!(tm_only_pinned.tabs[2].id, dup_op2);
    assert!(tm_only_pinned.verify_pinned_invariant().is_ok());
}

#[test]
fn test_duplicate_unpinned_tab_when_pinned_tabs_exist() {
    let mut tm = TabManager::new();
    let p1 = tm.new_tab("https://pinned-1.org", false);
    let p2 = tm.new_tab("https://pinned-2.org", false);
    tm.pin_tab(p1);
    tm.pin_tab(p2);
    let u1 = tm.new_tab("https://unpinned-1.org", false);
    let u2 = tm.new_tab("https://unpinned-2.org", false);
    let u3 = tm.new_tab("https://unpinned-3.org", false);

    // Duplicate first unpinned tab (u1 at index 2)
    let dup_u1 = tm.duplicate_tab(u1);
    assert!(dup_u1 > 0);
    assert!(tm.verify_pinned_invariant().is_ok());
    assert_eq!(tm.tabs.len(), 6);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 2);

    // Order: [p1, p2, u1, dup_u1, u2, u3]
    assert_eq!(tm.tabs[0].id, p1);
    assert_eq!(tm.tabs[1].id, p2);
    assert_eq!(tm.tabs[2].id, u1);
    assert!(!tm.tabs[2].pinned);
    assert_eq!(tm.tabs[3].id, dup_u1);
    assert!(!tm.tabs[3].pinned);
    assert_eq!(tm.tabs[4].id, u2);
    assert_eq!(tm.tabs[5].id, u3);

    // Duplicate last unpinned tab (u3 at index 5)
    let dup_u3 = tm.duplicate_tab(u3);
    assert!(dup_u3 > 0);
    assert!(tm.verify_pinned_invariant().is_ok());
    assert_eq!(tm.tabs.len(), 7);
    assert_eq!(tm.tabs[6].id, dup_u3);
    assert!(!tm.tabs[6].pinned);
}

#[test]
fn test_duplicate_tab_enforces_100_tab_limit_without_corruption() {
    let mut tm = TabManager::new();

    // Create 10 pinned tabs
    let mut pinned_ids = vec![];
    for i in 0..10 {
        let id = tm.new_tab(&format!("https://pinned-{i}.org"), false);
        tm.pin_tab(id);
        pinned_ids.push(id);
    }

    // Create 90 unpinned tabs to reach exactly 100 tabs
    let mut unpinned_ids = vec![];
    for i in 0..90 {
        let id = tm.new_tab(&format!("https://unpinned-{i}.org"), false);
        unpinned_ids.push(id);
    }

    assert_eq!(tm.tabs.len(), 100);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 10);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 1. Attempt to duplicate a pinned tab at limit
    let res_pin = tm.try_duplicate_tab(pinned_ids[0]);
    assert!(
        res_pin.is_err(),
        "DuplicateTab must fail when tab limit 100 is reached"
    );
    assert_eq!(
        tm.tabs.len(),
        100,
        "Tab list size must remain 100 after rejected duplicate"
    );
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 10);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 2. Attempt to duplicate an unpinned tab at limit
    let res_unpin = tm.try_duplicate_tab(unpinned_ids[0]);
    assert!(
        res_unpin.is_err(),
        "DuplicateTab on unpinned tab must fail at limit 100"
    );
    assert_eq!(tm.tabs.len(), 100);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 3. Close 1 tab -> length becomes 99
    tm.close_tab(unpinned_ids[89]);
    assert_eq!(tm.tabs.len(), 99);

    // 4. Duplicate pinned tab now succeeds and brings length back to 100
    let dup_res = tm.try_duplicate_tab(pinned_ids[0]);
    assert!(dup_res.is_ok());
    assert_eq!(tm.tabs.len(), 100);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 11);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 5. Subsequent duplicate fails again
    let next_fail = tm.try_duplicate_tab(pinned_ids[0]);
    assert!(next_fail.is_err());
    assert_eq!(tm.tabs.len(), 100);
    assert!(tm.verify_pinned_invariant().is_ok());
}

#[test]
fn test_reordering_tabs_around_duplicates() {
    let mut tm = TabManager::new();
    let p1 = tm.new_tab("https://p1.org", false);
    let p2 = tm.new_tab("https://p2.org", false);
    tm.pin_tab(p1);
    tm.pin_tab(p2);
    let u1 = tm.new_tab("https://u1.org", false);
    let u2 = tm.new_tab("https://u2.org", false);

    // Duplicate p1 -> [p1, dup_p1, p2, u1, u2]
    let dup_p1 = tm.duplicate_tab(p1);
    assert!(tm.verify_pinned_invariant().is_ok());

    // Duplicate u1 -> [p1, dup_p1, p2, u1, dup_u1, u2]
    let dup_u1 = tm.duplicate_tab(u1);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 1. Reorder pinned duplicate: move dup_p1 (at index 1) to p1 (at index 0)
    assert!(tm.reorder_tab(dup_p1, p1));
    assert_eq!(tm.tabs[0].id, dup_p1);
    assert_eq!(tm.tabs[1].id, p1);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 2. Reorder pinned duplicate to last pinned tab p2 (index 2)
    assert!(tm.reorder_tab(dup_p1, p2));
    assert_eq!(tm.tabs[0].id, p1);
    assert_eq!(tm.tabs[1].id, p2);
    assert_eq!(tm.tabs[2].id, dup_p1);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 3. ILLEGAL: Attempt to move pinned duplicate dup_p1 into unpinned tab u1
    assert!(
        !tm.reorder_tab(dup_p1, u1),
        "Reorder across partition boundary must be REJECTED"
    );
    assert_eq!(tm.tabs[2].id, dup_p1);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 4. Reorder unpinned duplicate: move dup_u1 (at index 4) to u2 (at index 5)
    assert!(tm.reorder_tab(dup_u1, u2));
    assert_eq!(tm.tabs[4].id, u2);
    assert_eq!(tm.tabs[5].id, dup_u1);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 5. ILLEGAL: Attempt to move unpinned duplicate dup_u1 into pinned tab p2
    assert!(
        !tm.reorder_tab(dup_u1, p2),
        "Reorder across partition boundary must be REJECTED"
    );
    assert!(tm.verify_pinned_invariant().is_ok());
}

#[test]
fn test_pinning_and_unpinning_duplicated_tabs() {
    let mut tm = TabManager::new();
    let p1 = tm.new_tab("https://p1.org", false);
    tm.pin_tab(p1);
    let u1 = tm.new_tab("https://u1.org", false);
    let _u2 = tm.new_tab("https://u2.org", false);

    // 1. Duplicate pinned tab p1 -> dup_p1 is pinned
    let dup_p1 = tm.duplicate_tab(p1);
    assert!(tm.tabs.iter().find(|t| t.id == dup_p1).unwrap().pinned);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 2);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 2. Unpin the duplicated pinned tab (dup_p1)
    tm.pin_tab(dup_p1);
    assert!(!tm.tabs.iter().find(|t| t.id == dup_p1).unwrap().pinned);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 1);
    assert!(tm.verify_pinned_invariant().is_ok());
    // dup_p1 should now be at other_pinned_count = 1 (start of unpinned section)
    assert_eq!(tm.tabs[0].id, p1);
    assert_eq!(tm.tabs[1].id, dup_p1);

    // 3. Re-pin dup_p1
    tm.pin_tab(dup_p1);
    assert!(tm.tabs.iter().find(|t| t.id == dup_p1).unwrap().pinned);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 2);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 4. Duplicate unpinned tab u1 -> dup_u1 is unpinned
    let dup_u1 = tm.duplicate_tab(u1);
    assert!(!tm.tabs.iter().find(|t| t.id == dup_u1).unwrap().pinned);
    assert!(tm.verify_pinned_invariant().is_ok());

    // 5. Pin the duplicated unpinned tab (dup_u1)
    tm.pin_tab(dup_u1);
    assert!(tm.tabs.iter().find(|t| t.id == dup_u1).unwrap().pinned);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 3);
    assert!(tm.verify_pinned_invariant().is_ok());
    // All 3 pinned tabs precede unpinned tabs
    assert!(tm.tabs[0].pinned);
    assert!(tm.tabs[1].pinned);
    assert!(tm.tabs[2].pinned);
    assert!(!tm.tabs[3].pinned);

    // 6. Unpin dup_u1 back
    tm.pin_tab(dup_u1);
    assert!(!tm.tabs.iter().find(|t| t.id == dup_u1).unwrap().pinned);
    assert_eq!(tm.tabs.iter().filter(|t| t.pinned).count(), 2);
    assert!(tm.verify_pinned_invariant().is_ok());
}

#[test]
fn test_randomized_stress_tab_lifecycle_permutations_50000_operations() {
    let mut tm = TabManager::new();

    // Initialize with 5 tabs
    for i in 1..=5 {
        tm.new_tab(&format!("https://init-{i}.org"), false);
    }
    tm.pin_tab(1);
    tm.pin_tab(2);

    let mut lcg_state: u64 = 0xdeadbeefcafebabe;
    let mut rand = || -> u64 {
        lcg_state = lcg_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        lcg_state
    };

    for op in 0..50_000 {
        let action = (rand() % 6) as usize;
        let count = tm.tabs.len();

        match action {
            0 => {
                // Duplicate random tab
                if count > 0 {
                    let idx = (rand() as usize) % count;
                    let target_id = tm.tabs[idx].id;
                    let _ = tm.try_duplicate_tab(target_id);
                }
            }
            1 => {
                // Toggle pin on random tab
                if count > 0 {
                    let idx = (rand() as usize) % count;
                    let target_id = tm.tabs[idx].id;
                    tm.pin_tab(target_id);
                }
            }
            2 => {
                // Reorder random pair
                if count > 1 {
                    let f = (rand() as usize) % count;
                    let t = (rand() as usize) % count;
                    let from_id = tm.tabs[f].id;
                    let to_id = tm.tabs[t].id;
                    tm.reorder_tab(from_id, to_id);
                }
            }
            3 => {
                // Close random tab (keep at least 1)
                if count > 1 {
                    let idx = (rand() as usize) % count;
                    let target_id = tm.tabs[idx].id;
                    tm.close_tab(target_id);
                }
            }
            4 => {
                // Add new tab if below 100
                if count < 100 {
                    let r = rand();
                    tm.new_tab(&format!("https://rand-{r}.org"), false);
                }
            }
            5 => {
                // Reopen closed tab
                let _ = tm.reopen_tab();
            }
            _ => unreachable!(),
        }

        // INVARIANT CHECK AFTER EVERY OPERATION
        if let Err(e) = tm.verify_pinned_invariant() {
            panic!("Invariant violated at step {op} (action {action})! State: {e}");
        }
    }

    assert!(tm.verify_pinned_invariant().is_ok());
}

// ============================================================================
// SUITE 5: Closed Tab Stack (closed_tabs) Stress Tests
// ============================================================================

#[test]
fn test_closed_tab_stack_fifo_eviction_at_100_pushes() {
    let mut tm = TabManager::new();

    // Push 100 closed tabs
    for i in 1..=100 {
        tm.record_closed_tab(
            &format!("https://example.org/page-{i}"),
            &format!("Page {i}"),
        );
        assert!(
            tm.closed_tabs.len() <= 50,
            "Closed tab stack exceeded maximum capacity of 50! Len was {}",
            tm.closed_tabs.len()
        );
    }

    assert_eq!(
        tm.closed_tabs.len(),
        50,
        "Closed tab stack must have exactly 50 tabs after 100 pushes"
    );

    // Verify FIFO eviction: pages 1..=50 must have been evicted.
    // The remaining pages must be 51..=100.
    assert_eq!(tm.closed_tabs[0].0, "https://example.org/page-51");
    assert_eq!(tm.closed_tabs[49].0, "https://example.org/page-100");
}

#[test]
fn test_closed_tab_stack_filtering_rules() {
    let mut tm = TabManager::new();

    let filtered_urls = [
        "papyrus:settings",
        "papyrus:bookmarks",
        "papyrus:history",
        "papyrus:downloads",
        "papyrus:passwords",
        "papyrus:extensions",
        "papyrus://settings",
        "papyrus://bookmarks",
        "papyrus://newtab",
        "about:blank",
        "",
    ];

    for url in filtered_urls {
        tm.record_closed_tab(url, "Filtered Page");
        assert_eq!(
            tm.closed_tabs.len(),
            0,
            "URL '{url}' MUST NOT be recorded into closed_tabs!"
        );
    }

    // Positive case: normal web URLs must be recorded
    tm.record_closed_tab("https://example.com", "Example");
    assert_eq!(tm.closed_tabs.len(), 1);
    assert_eq!(tm.closed_tabs[0].0, "https://example.com");
}

#[test]
fn test_closed_tab_stack_lifo_restoration_order() {
    let mut tm = TabManager::new();

    // Push 5 tabs
    for i in 1..=5 {
        tm.record_closed_tab(&format!("https://site.org/{i}"), &format!("Site {i}"));
    }

    // Pop and verify LIFO order: 5, 4, 3, 2, 1
    for expected_i in (1..=5).rev() {
        let reopened_id = tm.reopen_tab().expect("Expected tab to be restored");
        let restored_tab = tm.tabs.iter().find(|t| t.id == reopened_id).unwrap();
        assert_eq!(
            restored_tab.url,
            format!("https://site.org/{expected_i}"),
            "LIFO restoration order violated!"
        );
    }

    // Stack is now empty
    assert!(tm.closed_tabs.is_empty());
    assert_eq!(tm.reopen_tab(), None);
}

// ============================================================================
// SUITE 6: SavedState Session Persistence & Geometry Stress Tests
// ============================================================================

#[test]
fn test_saved_state_pinned_tabs_and_geometry_roundtrip() {
    let state = SavedState {
        preferences: Preferences::default(),
        bookmarks: vec![],
        history: vec![],
        passwords: vec![],
        restored_urls: vec![
            "https://pinned1.org".into(),
            "https://pinned2.org".into(),
            "https://normal.org".into(),
        ],
        restored_active: 1,
        last_session_urls: vec![
            "https://pinned1.org".into(),
            "https://pinned2.org".into(),
            "https://normal.org".into(),
        ],
        last_session_active: 1,
        last_session_pinned: vec![true, true, false],
        window_x: Some(150),
        window_y: Some(250),
        window_width: Some(1400),
        window_height: Some(900),
        window_maximized: false,
    };

    let serialized = serde_json::to_string(&state).unwrap();
    let deserialized: SavedState = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.last_session_pinned, vec![true, true, false]);
    assert_eq!(deserialized.window_x, Some(150));
    assert_eq!(deserialized.window_y, Some(250));
    assert_eq!(deserialized.window_width, Some(1400));
    assert_eq!(deserialized.window_height, Some(900));
    assert!(!deserialized.window_maximized);
}

#[test]
fn test_saved_state_backward_compatibility_with_m1_legacy_json() {
    // Legacy M1 state.json without last_session_pinned and window_* fields
    let legacy_json = r#"{
        "preferences": {
            "engine": "google",
            "zoom": 100,
            "theme": "parchment",
            "startup": "newtab",
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
        "restored_urls": ["https://legacy.example.org"],
        "restored_active": 0,
        "last_session_urls": ["https://legacy.example.org"],
        "last_session_active": 0
    }"#;

    let state: SavedState = serde_json::from_str(legacy_json)
        .expect("SavedState MUST deserialize legacy M1 state without panic");

    // Missing fields should default gracefully via #[serde(default)]
    assert!(state.last_session_pinned.is_empty());
    assert_eq!(state.window_x, None);
    assert_eq!(state.window_y, None);
    assert_eq!(state.window_width, None);
    assert_eq!(state.window_height, None);
    assert!(!state.window_maximized);
}
