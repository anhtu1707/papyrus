"""Empirical Adversarial Challenge Harness M2-R3-2: Middle-Click Autoscroll Suppression & Overflow DOM.
Challenger 2 (teamwork_challenger_m2_r3_2)

Exhaustively verifies:
1. High tab count (10+ tabs) with overflowing container (scrollWidth > clientWidth).
2. Native middle-click pointer events on unpinned tabs -> MUST emit close_tab IPC.
3. Native middle-click pointer events on pinned tabs -> MUST NOT emit close_tab IPC
   and MUST NOT trigger Chromium autoscroll anchor (immediate responsiveness without dismiss clicks).
4. Middle-click targeting deeply nested child elements (.tab-title, .tab-fav, .tab-audio, svg, path).
5. Left-click drag-and-drop interaction is NOT disrupted by the mousedown button=1 handler.
6. Right-click context menu interaction is NOT disrupted on tabs, child elements, or empty space.
7. Middle-click while horizontally scrolled (scrollLeft > 0) resolves correct tab delegates.
"""
import copy
import sys
import time
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

if sys.stdout.encoding != 'utf-8':
    sys.stdout.reconfigure(encoding='utf-8', errors='replace')
if sys.stderr.encoding != 'utf-8':
    sys.stderr.reconfigure(encoding='utf-8', errors='replace')

ROOT = Path(__file__).resolve().parents[1]
html = (ROOT / 'assets/header.html').read_text(encoding='utf-8')
html = html.replace('/*HEADER_CSS*/', (ROOT / 'assets/header.css').read_text(encoding='utf-8'))
html = html.replace('/*HEADER_JS*/', (ROOT / 'assets/header.js').read_text(encoding='utf-8'))
fixture = ROOT / 'target/challenger_m2_r3_2_test.html'
fixture.parent.mkdir(exist_ok=True)
fixture.write_text(html, encoding='utf-8')

def build_overflow_state():
    # 4 pinned tabs, 11 unpinned tabs (15 total), long titles and varied audio states
    tabs = [
        # Pinned tabs (id: 1..4)
        {'id': 1, 'title': 'Pinned Mail', 'url': 'https://mail.google.com/', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': False},
        {'id': 2, 'title': 'Pinned Music Playing', 'url': 'https://music.youtube.com/', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': True},
        {'id': 3, 'title': 'Pinned Podcast Muted', 'url': 'https://podcasts.google.com/', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': True, 'is_playing_audio': False},
        {'id': 4, 'title': 'Pinned Loading Tab', 'url': 'https://example.org/loading', 'loading': True, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': False},
        
        # Unpinned tabs (id: 101..111)
        {'id': 101, 'title': 'Dashboard - Vietnamese Kho Lưu Trữ Tài Liệu Đầy Đủ', 'url': 'https://docs.google.com/', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        {'id': 102, 'title': 'Unpinned YouTube Live Stream Audio', 'url': 'https://youtube.com/watch?v=live', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': True},
        {'id': 103, 'title': 'Unpinned Spotify Muted Track', 'url': 'https://open.spotify.com/track/123', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': True, 'is_playing_audio': False},
        {'id': 104, 'title': 'GitHub - Papyrus Browser Repository Issues and PRs', 'url': 'https://github.com/papyrus/browser', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        {'id': 105, 'title': 'Rust Standard Library std::sync::atomic documentation', 'url': 'https://doc.rust-lang.org/std/sync/atomic/', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        {'id': 106, 'title': 'Hacker News - Y Combinator Discussion Feed', 'url': 'https://news.ycombinator.com/', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        {'id': 107, 'title': 'Stack Overflow - How to prevent middle click scroll in Chrome', 'url': 'https://stackoverflow.com/questions/123', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        {'id': 108, 'title': 'Papyrus Internal Settings', 'url': 'papyrus://settings', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        {'id': 109, 'title': 'Papyrus Internal Bookmarks Manager', 'url': 'papyrus://bookmarks', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        {'id': 110, 'title': 'MDN Web Docs - MouseEvent button property and auxclick', 'url': 'https://developer.mozilla.org/en-US/docs/Web/API/Element/auxclick_event', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        {'id': 111, 'title': 'Last Tab in Heavy Overflowing List', 'url': 'https://example.com/last', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
    ]
    return {
        'tabs': tabs,
        'active': 101,
        'preferences': {'engine': 'google', 'zoom': 100, 'theme': 'kraft', 'startup': 'newtab', 'show_bookmarks': True, 'adblock': True, 'block_popups': True, 'allowed_hosts': [], 'blocked_hosts': [], 'download_dir': ''},
        'bookmarks': [],
        'blocked': 0,
        'clearing': False,
        'can_back': False,
        'can_forward': False,
        'downloads': [],
        'notice': '',
        'data_dir': 'test-profile',
        'version': '0.3.7',
        'builtin_count': 0,
    }

def run_adversarial_suite():
    print("=" * 75)
    print("STARTING CHALLENGER M2-R3-2 EMPIRICAL ADVERSARIAL TEST SUITE")
    print("=" * 75)

    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        # Use a standard window width (1100px) so 15 tabs strongly overflow
        page = browser.new_page(viewport={'width': 1100, 'height': 800})

        page_errors = []
        page.on('pageerror', lambda err: page_errors.append(str(err)))

        # Instrument IPC and native event recorder
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
            window.event_log = [];
            ['mousedown', 'mouseup', 'auxclick', 'click', 'contextmenu', 'dragstart', 'dragover', 'drop'].forEach(type => {
                window.addEventListener(type, e => {
                    window.event_log.push({
                        type: type,
                        button: e.button,
                        defaultPrevented: e.defaultPrevented,
                        targetClass: e.target.className || e.target.tagName,
                        targetTag: e.target.tagName
                    });
                }, false);
            });
        """)

        page.goto(fixture.as_uri(), wait_until='networkidle')
        assert page.evaluate('window.messages[0].action') == 'ready', "App ready failed"

        # Apply state with 15 tabs
        state = build_overflow_state()
        page.evaluate('window.applyState', state)
        time.sleep(0.05)

        # ---------------------------------------------------------------------
        # TEST 1: Physical DOM Overflow Verification (scrollWidth > clientWidth)
        # ---------------------------------------------------------------------
        print("\n[TEST 1] Verifying physical DOM overflow metrics...")
        metrics = page.evaluate("""() => {
            const el = document.getElementById('tabs-container');
            return {
                scrollWidth: el.scrollWidth,
                clientWidth: el.clientWidth,
                tabCount: el.querySelectorAll('.tab').length,
                isOverflowing: el.scrollWidth > el.clientWidth
            };
        }""")
        print(f"  Container Metrics: {metrics}")
        assert metrics['tabCount'] == 15, f"Expected 15 tabs, found {metrics['tabCount']}"
        assert metrics['isOverflowing'], f"Tabs container MUST physically overflow! {metrics}"
        assert metrics['scrollWidth'] > metrics['clientWidth'] + 200, (
            f"Expected strong overflow (>200px), got {metrics['scrollWidth'] - metrics['clientWidth']}px"
        )
        print("  [+] TEST 1 PASS: High tab count (15 tabs) heavily overflows tabs container.")

        # ---------------------------------------------------------------------
        # TEST 2: Native Middle-Click on Unpinned Tabs in Overflowing Bar
        # ---------------------------------------------------------------------
        print("\n[TEST 2] Testing native middle-click on unpinned tabs in overflowing container...")
        test_unpinned_ids = [101, 102, 103, 104, 105]
        for tab_id in test_unpinned_ids:
            page.evaluate('window.messages = []; window.event_log = [];')
            locator = page.locator(f'.tab[data-id="{tab_id}"]')
            locator.click(button='middle')

            msgs = page.evaluate('window.messages')
            events = page.evaluate('window.event_log')
            close_msgs = [m for m in msgs if m.get('action') == 'close_tab']

            assert len(close_msgs) == 1, f"Expected exactly 1 close_tab for tab {tab_id}, got {msgs}"
            assert close_msgs[0]['id'] == tab_id, f"Expected close_tab id {tab_id}, got {close_msgs[0]['id']}"

            # Check event chain: mousedown MUST be defaultPrevented
            md_events = [e for e in events if e['type'] == 'mousedown' and e['button'] == 1]
            assert len(md_events) >= 1, f"Missing mousedown event for tab {tab_id}"
            assert md_events[0]['defaultPrevented'] is True, f"mousedown button=1 MUST be defaultPrevented! {md_events}"

            # Check auxclick event fired
            aux_events = [e for e in events if e['type'] == 'auxclick' and e['button'] == 1]
            assert len(aux_events) >= 1, f"auxclick MUST fire when autoscroll is suppressed! {events}"

        print(f"  [+] TEST 2 PASS: Native middle-click closed {len(test_unpinned_ids)} unpinned tabs cleanly.")

        # ---------------------------------------------------------------------
        # TEST 3: Native Middle-Click on Pinned Tabs -> ZERO close_tab, NO Autoscroll Anchor
        # ---------------------------------------------------------------------
        print("\n[TEST 3] Testing native middle-click on pinned tabs protection & anchor suppression...")
        test_pinned_ids = [1, 2, 3, 4]
        for tab_id in test_pinned_ids:
            page.evaluate('window.messages = []; window.event_log = [];')
            locator = page.locator(f'.tab[data-id="{tab_id}"]')
            locator.click(button='middle')

            msgs = page.evaluate('window.messages')
            events = page.evaluate('window.event_log')

            # 1. MUST NOT emit close_tab
            close_msgs = [m for m in msgs if m.get('action') == 'close_tab']
            assert len(close_msgs) == 0, f"Middle-click on pinned tab {tab_id} emitted close_tab: {msgs}"

            # 2. mousedown button=1 MUST be defaultPrevented (preventing Chromium autoscroll anchor)
            md_events = [e for e in events if e['type'] == 'mousedown' and e['button'] == 1]
            assert len(md_events) >= 1, f"Missing mousedown event for pinned tab {tab_id}"
            assert md_events[0]['defaultPrevented'] is True, (
                f"mousedown button=1 on pinned tab {tab_id} MUST be defaultPrevented to block autoscroll anchor!"
            )

            # 3. CRITICAL EMPIRICAL PROOF of NO Autoscroll Anchor:
            # If autoscroll anchor were engaged by Chromium, the immediate next left-click would be consumed
            # by Chromium to dismiss the anchor (resulting in zero IPC action).
            # Here we immediately left-click the pinned tab. It MUST emit 'switch_tab' on the VERY FIRST click!
            page.evaluate('window.messages = [];')
            locator.locator('.tab-select').click(button='left')
            switch_msgs = page.evaluate('window.messages')
            assert any(m.get('action') == 'switch_tab' and m.get('id') == tab_id for m in switch_msgs), (
                f"Immediate left click after middle click failed! Autoscroll anchor was active and swallowed the click! {switch_msgs}"
            )

        print("  [+] TEST 3 PASS: Pinned tabs never close on middle-click, and autoscroll anchor is strictly suppressed.")

        # ---------------------------------------------------------------------
        # TEST 4: Deep Middle-Click Targeting on Tab Child Elements
        # ---------------------------------------------------------------------
        print("\n[TEST 4] Testing middle-click on tab child elements (.tab-title, .tab-fav, .tab-audio, svg)...")

        # 4.1 Child element: .tab-title on unpinned tab 106
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="106"] .tab-title').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert any(m.get('action') == 'close_tab' and m.get('id') == 106 for m in msgs), (
            f"Middle-click on .tab-title failed to close tab: {msgs}"
        )

        # 4.2 Child element: .tab-fav (favicon img) on unpinned tab 107
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="107"] .tab-fav').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert any(m.get('action') == 'close_tab' and m.get('id') == 107 for m in msgs), (
            f"Middle-click on unpinned .tab-fav failed to close tab: {msgs}"
        )

        # 4.3 Child element: .tab-fav on pinned tab 1 -> MUST NOT emit close_tab
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="1"] .tab-fav').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' for m in msgs), (
            f"Middle-click on pinned .tab-fav improperly emitted close_tab: {msgs}"
        )

        # 4.4 Child element: .tab-audio on unpinned playing tab 102 -> MUST emit close_tab
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="102"] .tab-audio').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert any(m.get('action') == 'close_tab' and m.get('id') == 102 for m in msgs), (
            f"Middle-click on unpinned .tab-audio failed to close tab: {msgs}"
        )

        # 4.5 Child element: .tab-audio on pinned playing tab 2 -> MUST NOT emit close_tab
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="2"] .tab-audio').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' for m in msgs), (
            f"Middle-click on pinned .tab-audio improperly emitted close_tab: {msgs}"
        )

        # 4.6 Child element: .tab-audio on pinned muted tab 3 -> MUST NOT emit close_tab
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="3"] .tab-audio').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' for m in msgs), (
            f"Middle-click on pinned muted .tab-audio improperly emitted close_tab: {msgs}"
        )

        # 4.7 Child element: SVG icon inside .tab-audio on pinned tab 2 -> MUST NOT emit close_tab
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="2"] .tab-audio svg').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' for m in msgs), (
            f"Middle-click on pinned .tab-audio svg improperly emitted close_tab: {msgs}"
        )

        print("  [+] TEST 4 PASS: All child elements (.tab-title, .tab-fav, .tab-audio, svg) resolve delegation correctly.")

        # ---------------------------------------------------------------------
        # TEST 5: Middle-Click under Scrolled Conditions (scrollLeft > 0)
        # ---------------------------------------------------------------------
        print("\n[TEST 5] Testing middle-click under horizontal scrolled conditions...")
        # Scroll container 300px to the right
        page.evaluate("() => { document.getElementById('tabs-container').scrollLeft = 300; }")
        scroll_pos = page.evaluate("() => document.getElementById('tabs-container').scrollLeft")
        assert scroll_pos > 0, f"Failed to scroll tabs container: {scroll_pos}"

        # Middle click tab 108 which is now in view
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="108"]').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert any(m.get('action') == 'close_tab' and m.get('id') == 108 for m in msgs), (
            f"Middle-click while scrolled failed to emit close_tab(108): {msgs}"
        )

        # Middle click pinned tab 1 while container is scrolled
        # (Pinned tabs are at the left, so scroll container back or click pinned tab 1 directly)
        page.evaluate("() => { document.getElementById('tabs-container').scrollLeft = 0; }")
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="1"]').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' for m in msgs), "Pinned tab closed after scroll reset!"

        print("  [+] TEST 5 PASS: Middle-click operates reliably across scroll offsets.")

        # ---------------------------------------------------------------------
        # TEST 6: Non-Interference: Left-Click Drag-and-Drop & Normal Clicks
        # ---------------------------------------------------------------------
        print("\n[TEST 6] Testing non-interference: left-click Drag & Drop and tab switching...")

        # 6.1 Verify left-click mousedown (button=0) is NEVER defaultPrevented
        page.evaluate('window.messages = []; window.event_log = [];')
        page.locator('.tab[data-id="104"]').click(button='left')
        events = page.evaluate('window.event_log')
        left_md = [e for e in events if e['type'] == 'mousedown' and e['button'] == 0]
        assert len(left_md) >= 1, "Missing left-click mousedown"
        assert left_md[0]['defaultPrevented'] is False, (
            "Left-click mousedown MUST NOT be defaultPrevented! If prevented, text selection and drag will break."
        )

        # 6.2 Test dragstart event dispatch on tab
        drag_fired = page.evaluate("""() => {
            const el = document.querySelector('.tab[data-id="104"]');
            const dt = new DataTransfer();
            const ev = new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: dt });
            el.dispatchEvent(ev);
            return el.classList.contains('dragging') && window.draggedTabPinned === false;
        }""")
        assert drag_fired, "dragstart event was disrupted on unpinned tab!"

        # Clean up drag state
        page.evaluate("""() => {
            const el = document.querySelector('.tab[data-id="104"]');
            el.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true }));
        }""")

        # 6.3 Test full drag-and-drop between unpinned tabs (104 -> 105)
        page.evaluate('window.messages = []')
        page.evaluate("""() => {
            const fromEl = document.querySelector('.tab[data-id="104"]');
            const toEl = document.querySelector('.tab[data-id="105"]');
            const dt = new DataTransfer();
            fromEl.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: dt }));
            toEl.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt }));
            toEl.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt }));
            fromEl.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer: dt }));
        }""")
        msgs = page.evaluate('window.messages')
        assert any(m == {'action': 'reorder_tab', 'from': 104, 'to': 105} for m in msgs), (
            f"Left-click DnD between unpinned tabs failed: {msgs}"
        )

        # 6.4 Test full drag-and-drop between pinned tabs (1 -> 2)
        page.evaluate('window.messages = []')
        page.evaluate("""() => {
            const fromEl = document.querySelector('.tab[data-id="1"]');
            const toEl = document.querySelector('.tab[data-id="2"]');
            const dt = new DataTransfer();
            fromEl.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: dt }));
            toEl.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt }));
            toEl.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt }));
            fromEl.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer: dt }));
        }""")
        msgs = page.evaluate('window.messages')
        assert any(m == {'action': 'reorder_tab', 'from': 1, 'to': 2} for m in msgs), (
            f"Left-click DnD between pinned tabs failed: {msgs}"
        )

        # 6.5 ADVERSARIAL: Cross-boundary Drag Pinned (1) -> Unpinned (105) MUST NOT reorder
        page.evaluate('window.messages = []')
        page.evaluate("""() => {
            const fromEl = document.querySelector('.tab[data-id="1"]');
            const toEl = document.querySelector('.tab[data-id="105"]');
            const dt = new DataTransfer();
            fromEl.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: dt }));
            toEl.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt }));
            toEl.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt }));
            fromEl.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer: dt }));
        }""")
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'reorder_tab' for m in msgs), (
            f"Cross-boundary DnD improperly emitted reorder_tab: {msgs}"
        )

        print("  [+] TEST 6 PASS: Left-click Drag & Drop and tab switching operate completely unaffected.")

        # ---------------------------------------------------------------------
        # TEST 7: Non-Interference: Right-Click Context Menu
        # ---------------------------------------------------------------------
        print("\n[TEST 7] Testing non-interference: right-click contextmenu...")

        # 7.1 Right click on unpinned tab 109
        page.evaluate('window.messages = []; window.event_log = [];')
        page.locator('.tab[data-id="109"]').click(button='right')
        msgs = page.evaluate('window.messages')
        events = page.evaluate('window.event_log')
        assert any(m.get('action') == 'show_tab_menu' and m.get('id') == 109 for m in msgs), (
            f"Right-click contextmenu on tab 109 failed to emit show_tab_menu: {msgs}"
        )
        right_md = [e for e in events if e['type'] == 'mousedown' and e['button'] == 2]
        assert len(right_md) >= 1, "Missing right-click mousedown"
        assert right_md[0]['defaultPrevented'] is False, "Right-click mousedown MUST NOT be defaultPrevented!"

        # 7.2 Right click on pinned tab 1
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="1"]').click(button='right')
        msgs = page.evaluate('window.messages')
        assert any(m.get('action') == 'show_tab_menu' and m.get('id') == 1 for m in msgs), (
            f"Right-click contextmenu on pinned tab 1 failed: {msgs}"
        )

        # 7.3 Right click on child element .tab-title of tab 110
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="110"] .tab-title').click(button='right')
        msgs = page.evaluate('window.messages')
        assert any(m.get('action') == 'show_tab_menu' and m.get('id') == 110 for m in msgs), (
            f"Right-click on .tab-title failed to identify tab 110: {msgs}"
        )

        # 7.4 Right click on empty tab-container space (outside any .tab)
        page.evaluate('window.messages = []')
        # Dispatch contextmenu on #tabs-container directly
        page.evaluate("""() => {
            const container = document.getElementById('tabs-container');
            container.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 50, clientY: 20 }));
        }""")
        msgs = page.evaluate('window.messages')
        assert any(m.get('action') == 'show_tab_menu' and m.get('id') == 0 for m in msgs), (
            f"Right-click on empty container space must emit show_tab_menu with id 0: {msgs}"
        )

        print("  [+] TEST 7 PASS: Right-click contextmenu fully intact across tabs, child elements, and container.")

        # ---------------------------------------------------------------------
        # TEST 8: Non-Tab Space Middle-Click Isolation
        # ---------------------------------------------------------------------
        print("\n[TEST 8] Testing middle-click on non-tab space isolation...")

        # Middle click on 'new_tab' button
        page.evaluate('window.messages = []')
        page.locator('button[data-action="new_tab"]').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert len(msgs) == 0, f"Middle-click on new_tab button emitted unexpected messages: {msgs}"

        # Middle click on toolbar reload button
        page.evaluate('window.messages = []')
        page.locator('button[data-action="reload"]').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert len(msgs) == 0, f"Middle-click on toolbar reload button emitted unexpected messages: {msgs}"

        print("  [+] TEST 8 PASS: Non-tab space middle-click is safely ignored.")

        # Verify zero console page errors during entire execution
        assert len(page_errors) == 0, f"Unexpected page errors: {page_errors}"

        browser.close()

    print("\n" + "=" * 75)
    print("ALL ADVERSARIAL CHALLENGER M2-R3-2 TESTS COMPLETED WITH 100% SUCCESS!")
    print("=" * 75)

if __name__ == '__main__':
    run_adversarial_suite()
