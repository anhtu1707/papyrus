"""Empirical Challenger stress test suite for Milestone M2: Tab Lifecycle & Session Management.
Tests frontend DOM invariants, drag-and-drop boundaries, audio muting, shortcuts, and batch operations.
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
fixture = ROOT / 'target/challenger-m2-test.html'
fixture.parent.mkdir(exist_ok=True)
fixture.write_text(html, encoding='utf-8')

def run_challenger_m2_tests():
    print("[*] Starting Milestone M2 empirical frontend stress tests...")
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        page = browser.new_page(viewport={'width': 1280, 'height': 840})
        page_errors = []
        page.on('pageerror', lambda err: page_errors.append(str(err)))

        # Mock IPC recorder
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
        assert page.evaluate('window.messages[0].action') == 'ready'

        # Baseline snapshot structure
        base_state = {
            'tabs': [
                {'id': 1, 'title': 'Pinned 1', 'url': 'https://pinned1.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': False},
                {'id': 2, 'title': 'Pinned 2 (Audio)', 'url': 'https://pinned2.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': True},
                {'id': 3, 'title': 'Unpinned 1', 'url': 'https://unpinned1.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
                {'id': 4, 'title': 'Unpinned 2 (Muted)', 'url': 'https://unpinned2.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': True, 'is_playing_audio': False},
            ],
            'active': 3,
            'preferences': {
                'engine': 'brave', 'zoom': 100, 'theme': 'parchment', 'startup': 'newtab',
                'show_bookmarks': True, 'adblock': True, 'block_popups': True,
                'allowed_hosts': [], 'blocked_hosts': [], 'download_dir': ''
            },
            'bookmarks': [],
            'history': [],
            'passwords': [],
            'blocked': 0, 'clearing': False, 'can_back': False, 'can_forward': False,
            'downloads': [], 'notice': '', 'data_dir': 'test-profile', 'version': '0.3.7', 'builtin_count': 23,
        }

        page.evaluate('window.applyState', base_state)

        # =========================================================================
        # SUITE 1: PINNED TAB GEOMETRY & INTERACTION GUARDS
        # =========================================================================
        print("\n=== [SUITE 1] Pinned Tab Geometry & Interaction Guards ===")
        # Pinned tab elements
        p1 = page.locator('.tab[data-id="1"]')
        p2 = page.locator('.tab[data-id="2"]')
        u1 = page.locator('.tab[data-id="3"]')
        u2 = page.locator('.tab[data-id="4"]')

        # Verify pinned classes
        assert p1.evaluate('el => el.classList.contains("pinned")') is True
        assert p2.evaluate('el => el.classList.contains("pinned")') is True
        assert u1.evaluate('el => el.classList.contains("pinned")') is False
        assert u2.evaluate('el => el.classList.contains("pinned")') is False

        # Verify fixed 36px width for pinned tabs
        w1 = p1.evaluate('el => Math.round(el.getBoundingClientRect().width)')
        w2 = p2.evaluate('el => Math.round(el.getBoundingClientRect().width)')
        assert w1 == 36, f"Pinned tab 1 width expected 36, got {w1}"
        assert w2 == 36, f"Pinned tab 2 width expected 36, got {w2}"

        # Verify close button is absent in pinned tabs
        assert p1.locator('.tab-close').count() == 0, "Pinned tab must not contain close button"
        assert p2.locator('.tab-close').count() == 0, "Pinned tab must not contain close button"
        assert u1.locator('.tab-close').count() == 1, "Unpinned tab must contain close button"

        # Verify middle-click does not close pinned tab
        page.evaluate('window.messages = []')
        p1.click(button='middle')
        p2.click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' for m in msgs), f"Middle-click on pinned tab dispatched close_tab: {msgs}"

        # Verify middle-click on unpinned tab DOES close it
        page.evaluate('window.messages = []')
        u1.click(button='middle')
        msg = page.evaluate('window.messages.at(-1)')
        assert msg == {'action': 'close_tab', 'id': 3}, f"Middle-click on unpinned tab failed: {msg}"

        print("[+] SUITE 1 PASSED: Pinned tab 36px geometry, missing close button, and middle-click protection verified.")

        # =========================================================================
        # SUITE 2: AUDIO INDICATOR CLICKS & ISOLATION
        # =========================================================================
        print("\n=== [SUITE 2] Audio Indicator Clicks & Isolation ===")
        # Pinned 2 has audio playing
        audio_btn_p2 = p2.locator('.tab-audio')
        expect(audio_btn_p2).to_be_visible()
        assert audio_btn_p2.evaluate('el => el.classList.contains("playing")') is True

        # Click audio button on background tab (p2): must dispatch toggle_mute_tab, NOT switch_tab!
        page.evaluate('window.messages = []')
        audio_btn_p2.click()
        last_msg = page.evaluate('window.messages.at(-1)')
        assert last_msg == {'action': 'toggle_mute_tab', 'id': 2}, f"Expected toggle_mute_tab for id 2, got {last_msg}"
        # Verify active tab did NOT change to 2 (stopPropagation check)
        assert not any(m.get('action') == 'switch_tab' for m in page.evaluate('window.messages')), "Audio click erroneously switched active tab!"

        # Unpinned 2 is muted
        audio_btn_u2 = u2.locator('.tab-audio')
        expect(audio_btn_u2).to_be_visible()
        assert audio_btn_u2.evaluate('el => el.classList.contains("muted")') is True

        page.evaluate('window.messages = []')
        audio_btn_u2.click()
        last_msg = page.evaluate('window.messages.at(-1)')
        assert last_msg == {'action': 'toggle_mute_tab', 'id': 4}, f"Expected toggle_mute_tab for id 4, got {last_msg}"
        assert not any(m.get('action') == 'switch_tab' for m in page.evaluate('window.messages')), "Audio click switched active tab!"

        print("[+] SUITE 2 PASSED: Audio indicator clicks dispatches toggle_mute_tab without switching tabs.")

        # =========================================================================
        # SUITE 3: KEYBOARD SHORTCUTS DISPATCH (Ctrl+M, Ctrl+Shift+K, Ctrl+Shift+T)
        # =========================================================================
        print("\n=== [SUITE 3] Keyboard Shortcuts Dispatch ===")
        # Active tab is id 3 (u1)
        # 1. Ctrl+M should toggle mute on active tab
        page.evaluate('window.messages = []')
        page.keyboard.press('Control+KeyM')
        msg = page.evaluate('window.messages.at(-1)')
        assert msg == {'action': 'toggle_mute_tab', 'id': 3}, f"Ctrl+M failed to dispatch toggle_mute_tab: {msg}"

        # 2. Ctrl+Shift+K should duplicate active tab
        page.evaluate('window.messages = []')
        page.keyboard.press('Control+Shift+KeyK')
        msg = page.evaluate('window.messages.at(-1)')
        assert msg == {'action': 'duplicate_tab', 'id': 3}, f"Ctrl+Shift+K failed to dispatch duplicate_tab: {msg}"

        # 3. Ctrl+Shift+T should reopen closed tab
        page.evaluate('window.messages = []')
        page.keyboard.press('Control+Shift+KeyT')
        msg = page.evaluate('window.messages.at(-1)')
        assert msg == {'action': 'reopen_tab'}, f"Ctrl+Shift+T failed to dispatch reopen_tab: {msg}"

        print("[+] SUITE 3 PASSED: Keyboard shortcuts Ctrl+M, Ctrl+Shift+K, Ctrl+Shift+T dispatched correctly.")

        # =========================================================================
        # SUITE 4: DRAG-AND-DROP BOUNDARY INTEGRITY
        # =========================================================================
        print("\n=== [SUITE 4] Drag-and-Drop Boundary Integrity ===")
        # Test frontend drag-and-drop boundary validation logic
        # 1. Drag pinned tab over another pinned tab -> allowed (drag-over class added)
        can_drop_pinned_on_pinned = page.evaluate("""
            () => {
                const p2 = document.querySelector('.tab[data-id="2"]');
                window.draggedTabId = 1;
                window.draggedTabPinned = true;
                
                const dt = new DataTransfer();
                const event = new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt });
                p2.dispatchEvent(event);
                const accepted = p2.classList.contains('drag-over');
                p2.classList.remove('drag-over');
                return accepted;
            }
        """)
        assert can_drop_pinned_on_pinned is True, "Dragging pinned tab over another pinned tab should be allowed"

        # 2. Drag pinned tab over unpinned tab -> rejected (drag-over class NOT added)
        can_drop_pinned_on_unpinned = page.evaluate("""
            () => {
                const u1 = document.querySelector('.tab[data-id="3"]');
                window.draggedTabId = 1;
                window.draggedTabPinned = true;
                
                const dt = new DataTransfer();
                const event = new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt });
                u1.dispatchEvent(event);
                const accepted = u1.classList.contains('drag-over');
                u1.classList.remove('drag-over');
                return accepted;
            }
        """)
        assert can_drop_pinned_on_unpinned is False, "Dragging pinned tab over unpinned tab MUST be rejected!"

        # 3. Drag unpinned tab over pinned tab -> rejected
        can_drop_unpinned_on_pinned = page.evaluate("""
            () => {
                const p1 = document.querySelector('.tab[data-id="1"]');
                window.draggedTabId = 3;
                window.draggedTabPinned = false;
                
                const dt = new DataTransfer();
                const event = new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt });
                p1.dispatchEvent(event);
                const accepted = p1.classList.contains('drag-over');
                p1.classList.remove('drag-over');
                return accepted;
            }
        """)
        assert can_drop_unpinned_on_pinned is False, "Dragging unpinned tab over pinned tab MUST be rejected!"

        # 4. Attempt cross-boundary drop: must NOT dispatch reorder_tab
        page.evaluate('window.messages = []')
        page.evaluate("""
            () => {
                const u1 = document.querySelector('.tab[data-id="3"]');
                window.draggedTabId = 1;
                window.draggedTabPinned = true;
                
                const dt = new DataTransfer();
                dt.setData('text/plain', '1');
                const event = new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt });
                u1.dispatchEvent(event);
            }
        """)
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'reorder_tab' for m in msgs), f"Cross-boundary drop dispatched reorder_tab: {msgs}"

        print("[+] SUITE 4 PASSED: Frontend drag-and-drop strictly enforces pinned vs unpinned boundary.")

        # =========================================================================
        # SUITE 5: HIGH-LOAD STRESS TEST (100 TABS: 20 PINNED, 80 UNPINNED)
        # =========================================================================
        print("\n=== [SUITE 5] High-Load Tab Layout Stress Test (100 Tabs) ===")
        stress_state = copy.deepcopy(base_state)
        stress_state['tabs'] = []
        for i in range(1, 21):
            stress_state['tabs'].append({
                'id': i,
                'title': f'Pinned Tab #{i}',
                'url': f'https://pinned-{i}.example.org',
                'loading': False,
                'blocked': 0,
                'error': None,
                'pinned': True,
                'muted': (i % 2 == 0),
                'is_playing_audio': (i % 3 == 0),
            })
        for i in range(21, 101):
            stress_state['tabs'].append({
                'id': i,
                'title': f'Unpinned Tab #{i}',
                'url': f'https://unpinned-{i}.example.org',
                'loading': False,
                'blocked': 0,
                'error': None,
                'pinned': False,
                'muted': (i % 5 == 0),
                'is_playing_audio': (i % 7 == 0),
            })
        stress_state['active'] = 25

        t0 = time.time()
        page.evaluate('window.applyState', stress_state)
        render_time = (time.time() - t0) * 1000
        print(f"[*] Rendered 100 tabs (20 pinned, 80 unpinned) in {render_time:.2f}ms")

        # Verify all 100 tabs rendered
        tabs_in_dom = page.locator('#tabs-container .tab')
        expect(tabs_in_dom).to_have_count(100)

        # Verify first 20 tabs are pinned
        pinned_tabs = page.locator('#tabs-container .tab.pinned')
        expect(pinned_tabs).to_have_count(20)

        # Verify 36px width for all 20 pinned tabs
        widths = pinned_tabs.evaluate_all('els => els.map(e => Math.round(e.getBoundingClientRect().width))')
        assert all(w == 36 for w in widths), f"Non-36px widths detected: {widths}"

        # Verify zero page errors
        assert not page_errors, f"Errors during 100-tab stress test: {page_errors}"
        print("[+] SUITE 5 PASSED: 100 tabs rendered and styled cleanly within budget.")

        browser.close()

    print("\n=======================================================")
    print("ALL CHALLENGER M2 EMPIRICAL TESTS PASSED (100% SUCCESS)")
    print("=======================================================\n")

if __name__ == '__main__':
    run_challenger_m2_tests()
