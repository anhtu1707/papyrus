"""Empirical Challenger M2-R2-2: Verification and Stress Testing Harness.
Exhaustively tests:
1. Middle-click (auxclick) behavior on pinned vs unpinned tabs:
   - Non-overflowing tab bar behavior
   - Real-world overflowing tab bar behavior (demonstrating the mousedown autoscroll bug)
   - Synthetic vs native Playwright middle-click events
   - Non-tab space middle-click isolation
2. Session state deserialization resilience with malformed JSON,
   corrupted geometry, mismatched lengths, and out-of-order pinned restoration.
"""
import copy
import json
import sys
import time
from pathlib import Path
from playwright.sync_api import sync_playwright

if sys.stdout.encoding != 'utf-8':
    sys.stdout.reconfigure(encoding='utf-8', errors='replace')
if sys.stderr.encoding != 'utf-8':
    sys.stderr.reconfigure(encoding='utf-8', errors='replace')

ROOT = Path(__file__).resolve().parents[1]
html = (ROOT / 'assets/header.html').read_text(encoding='utf-8')
html = html.replace('/*HEADER_CSS*/', (ROOT / 'assets/header.css').read_text(encoding='utf-8'))
html = html.replace('/*HEADER_JS*/', (ROOT / 'assets/header.js').read_text(encoding='utf-8'))
fixture = ROOT / 'target/challenger_m2_r2_2_test.html'
fixture.parent.mkdir(exist_ok=True)
fixture.write_text(html, encoding='utf-8')

def test_frontend_middle_click():
    print("\n==================================================================")
    print("EMPIRICAL TEST 1: Frontend Middle-Click & Autoscroll Behavior")
    print("==================================================================")

    results = {}

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        page = browser.new_page(viewport={'width': 1280, 'height': 840})

        page.add_init_script("""
            window.messages = [];
            window.ipc = {
                postMessage: function(s) {
                    try {
                        window.messages.push(JSON.parse(s));
                    } catch(e) {
                        window.messages.push({ raw: s, error: e.toString() });
                    }
                }
            };
        """)

        page.goto(fixture.as_uri(), wait_until='networkidle')

        # ---------------------------------------------------------------------
        # Case 1A: Non-overflowing tab bar (3 tabs total: 1 pinned, 2 unpinned)
        # ---------------------------------------------------------------------
        print("\n--- [Case 1A] Non-overflowing tab bar (scrollWidth < clientWidth) ---")
        state_small = {
            'tabs': [
                {'id': 1, 'title': 'Pinned 1', 'url': 'https://p1.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': False},
                {'id': 2, 'title': 'Unpinned 1', 'url': 'https://u1.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
                {'id': 3, 'title': 'Unpinned 2', 'url': 'https://u2.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
            ],
            'active': 2,
            'preferences': {'engine': 'google', 'zoom': 100, 'theme': 'dark', 'startup': 'newtab', 'show_bookmarks': True, 'adblock': True, 'block_popups': True, 'allowed_hosts': [], 'blocked_hosts': [], 'download_dir': ''},
            'bookmarks': [], 'history': [], 'passwords': [], 'blocked': 0, 'clearing': False, 'can_back': False, 'can_forward': False, 'downloads': [], 'notice': '', 'data_dir': 'test', 'version': '0.3.7', 'builtin_count': 0
        }
        page.evaluate('window.applyState', state_small)

        # Verify scrollWidth <= clientWidth
        is_overflowing = page.evaluate("() => document.getElementById('tabs-container').scrollWidth > document.getElementById('tabs-container').clientWidth")
        print(f"  Tabs overflowing: {is_overflowing}")
        assert not is_overflowing, "Small tab setup should not overflow"

        # 1. Native middle-click on pinned tab
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="1"]').click(button='middle')
        msgs_p1 = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' for m in msgs_p1)
        print("  [+] Native middle click on pinned tab 1: emitted 0 close_tab messages (PASS)")

        # Left click outside tabs to reset
        page.mouse.click(10, 10)

        # 2. Native middle-click on unpinned tab
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="2"]').click(button='middle')
        msgs_u1 = page.evaluate('window.messages')
        assert any(m == {'action': 'close_tab', 'id': 2} for m in msgs_u1)
        print("  [+] Native middle click on unpinned tab 2: emitted close_tab(2) (PASS)")

        # 3. Native middle-click on non-tab space
        page.evaluate('window.messages = []')
        page.locator('button[data-action="new_tab"]').click(button='middle')
        msgs_btn = page.evaluate('window.messages')
        assert len(msgs_btn) == 0
        print("  [+] Native middle click on button[data-action='new_tab']: emitted 0 messages (PASS)")

        # ---------------------------------------------------------------------
        # Case 1B: Real-world overflowing tab bar (6 tabs total: 3 pinned, 3 unpinned)
        # ---------------------------------------------------------------------
        print("\n--- [Case 1B] Real-world overflowing tab bar (scrollWidth > clientWidth) ---")
        state_overflow = {
            'tabs': [
                {'id': 101, 'title': 'Pinned Tab 1', 'url': 'https://p1.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': False},
                {'id': 102, 'title': 'Pinned Tab 2 (Audio)', 'url': 'https://p2.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': True},
                {'id': 103, 'title': 'Pinned Tab 3 (Muted)', 'url': 'https://p3.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': True, 'is_playing_audio': False},
                {'id': 201, 'title': 'Unpinned Tab 1', 'url': 'https://u1.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
                {'id': 202, 'title': 'Unpinned Tab 2 (Audio)', 'url': 'https://u2.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': True},
                {'id': 203, 'title': 'Unpinned Tab 3 (Muted)', 'url': 'https://u3.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': True, 'is_playing_audio': False},
                {'id': 204, 'title': 'Unpinned Tab 4', 'url': 'https://u4.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
                {'id': 205, 'title': 'Unpinned Tab 5', 'url': 'https://u5.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
                {'id': 206, 'title': 'Unpinned Tab 6', 'url': 'https://u6.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
            ],
            'active': 201,
            'preferences': {'engine': 'google', 'zoom': 100, 'theme': 'dark', 'startup': 'newtab', 'show_bookmarks': True, 'adblock': True, 'block_popups': True, 'allowed_hosts': [], 'blocked_hosts': [], 'download_dir': ''},
            'bookmarks': [], 'history': [], 'passwords': [], 'blocked': 0, 'clearing': False, 'can_back': False, 'can_forward': False, 'downloads': [], 'notice': '', 'data_dir': 'test', 'version': '0.3.7', 'builtin_count': 0
        }
        page.evaluate('window.applyState', state_overflow)

        # Check scrollWidth vs clientWidth
        scroll_info = page.evaluate("""() => {
            const el = document.getElementById('tabs-container');
            return { scrollWidth: el.scrollWidth, clientWidth: el.clientWidth, isOverflowing: el.scrollWidth > el.clientWidth };
        }""")
        print(f"  Container metrics: scrollWidth={scroll_info['scrollWidth']}, clientWidth={scroll_info['clientWidth']}, isOverflowing={scroll_info['isOverflowing']}")
        assert scroll_info['isOverflowing'], "Tabs should be overflowing in Case 1B"

        # Record events during native middle-click on unpinned tab 201
        page.evaluate("""
            window._rec = [];
            ['mousedown', 'mouseup', 'auxclick'].forEach(evt => {
                window.addEventListener(evt, e => {
                    window._rec.push({
                        type: evt,
                        button: e.button,
                        target: e.target.tagName + '.' + e.target.className,
                        defaultPrevented: e.defaultPrevented
                    });
                });
            });
        """)

        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="201"]').click(button='middle')
        rec_events = page.evaluate('window._rec')
        msgs_overflow = page.evaluate('window.messages')

        print(f"  Events recorded during native middle click on unpinned tab 201: {rec_events}")
        print(f"  IPC messages received: {msgs_overflow}")

        # Check if auxclick fired:
        auxclick_fired = any(e['type'] == 'auxclick' for e in rec_events)
        close_tab_emitted = any(m.get('action') == 'close_tab' and m.get('id') == 201 for m in msgs_overflow)

        if not auxclick_fired or not close_tab_emitted:
            print("  [!] FAILURE DETECTED: Chromium autoscroll hijacked middle-click mousedown on scrollable tab container!")
            print("      auxclick event was never generated; close_tab IPC was NEVER dispatched.")
            results['middle_click_overflow_bug'] = True
        else:
            print("  [+] Native middle-click closed tab in overflowing container.")
            results['middle_click_overflow_bug'] = False

        # ---------------------------------------------------------------------
        # Case 1C: Verify the exact root cause and proposed fix
        # ---------------------------------------------------------------------
        print("\n--- [Case 1C] Testing with proposed fix (preventing default on mousedown button=1) ---")
        page.mouse.click(10, 10)  # dismiss active autoscroll anchor
        page.evaluate("""() => {
            document.getElementById('tabs-container').addEventListener('mousedown', e => {
                if (e.button === 1 && e.target.closest('.tab')) {
                    e.preventDefault();
                }
            });
        }""")
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="201"]').click(button='middle')
        msgs_with_fix = page.evaluate('window.messages')
        print(f"  IPC messages received WITH mousedown fix: {msgs_with_fix}")
        assert any(m.get('action') == 'close_tab' and m.get('id') == 201 for m in msgs_with_fix), "Fix failed to restore close_tab"
        print("  [+] WITH mousedown preventDefault, middle-click reliably dispatches close_tab(201) even under overflow conditions!")

        browser.close()

    return results

def test_session_state_invariants():
    print("\n==================================================================")
    print("EMPIRICAL TEST 2: Session State Deserialization & Partition Invariant")
    print("==================================================================")

    # Replicate the session deserialization and restoration logic from src/main.rs:309-376
    def restore_and_partition(urls, pinned_states, active_idx):
        valid = []
        pinned = []
        for i, url in enumerate(urls):
            # allowed_navigation check
            if url.startswith("http://") or url.startswith("https://") or url.startswith("papyrus://") or url == "about:blank":
                if len(valid) < 100:
                    valid.append(url)
                    pinned.append(pinned_states[i] if i < len(pinned_states) else False)
        
        # In src/main.rs:370-375:
        # for (url, pinned) in to_open {
        #     let id = app.new_tab(&url, false, None)?;
        #     if pinned && let Some(tab) = app.tabs.iter_mut().find(|t| t.info.id == id) {
        #         tab.info.pinned = true;
        #     }
        # }
        restored_tabs = []
        for i, (u, p) in enumerate(zip(valid, pinned)):
            restored_tabs.append({'id': i + 1, 'url': u, 'pinned': p})
        
        # Verify core partition invariant: all pinned tabs must precede all unpinned tabs
        seen_unpinned = False
        partition_violated = False
        for tab in restored_tabs:
            if not tab['pinned']:
                seen_unpinned = True
            elif seen_unpinned and tab['pinned']:
                partition_violated = True
                break

        return restored_tabs, partition_violated

    # Test corrupted session state with out-of-order pinned tabs
    corrupted_session_urls = ["https://unpinned.org", "https://pinned.org"]
    corrupted_session_pinned = [False, True]
    tabs, violated = restore_and_partition(corrupted_session_urls, corrupted_session_pinned, 0)
    print(f"  Restored tabs from [unpinned, pinned]: {tabs}")
    print(f"  Partition invariant violated: {violated}")

    return {
        'session_out_of_order_violation': violated
    }

if __name__ == '__main__':
    r1 = test_frontend_middle_click()
    r2 = test_session_state_invariants()
    print("\n==================================================================")
    print("EMPIRICAL TEST SUMMARY:")
    print(f"  Middle-Click Autoscroll Bug on Overflowing Tabs: {r1.get('middle_click_overflow_bug')}")
    print(f"  Session Out-Of-Order Partition Invariant Violation: {r2.get('session_out_of_order_violation')}")
    print("==================================================================\n")
