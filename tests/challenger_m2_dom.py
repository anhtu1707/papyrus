"""Empirical Adversarial DOM & Interaction Stress Tests for Milestone M2.
Challenger M2-2: State Serialization & DOM Challenger.
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
fixture = ROOT / 'target/challenger_m2_dom_test.html'
fixture.parent.mkdir(exist_ok=True)
fixture.write_text(html, encoding='utf-8')

# Adversarial XSS Vectors for Tab Titles
XSS_TITLE_PAYLOADS = [
    '<img src=x onerror="window.xss_injected=\'img_onerror\'">',
    '<svg onload="window.xss_injected=\'svg_onload\'">',
    '<script>window.xss_injected=\'script_tag\'</script>',
    '"><script>window.xss_injected=\'breakout_script\'</script>',
    '<iframe src="javascript:window.xss_injected=\'iframe\'"></iframe>',
    '<details open ontoggle="window.xss_injected=\'details\'">',
    '<math><mtext><table><mglyph><style><!--</style><img src=x onerror="window.xss_injected=\'math\'">',
    '"><a href="javascript:void(0)" onclick="window.xss_injected=\'a_click\'">Click</a>',
    '{{constructor.constructor(\'window.xss_injected="template"\')()}}',
]

def run_tests():
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        page = browser.new_page(viewport={'width': 1280, 'height': 840})
        
        errors = []
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.add_init_script("""
            window.messages = [];
            window.ipc = {
                postMessage: s => window.messages.push(JSON.parse(s))
            };
        """)
        
        page.goto(fixture.as_uri(), wait_until='networkidle')
        assert page.evaluate('window.messages[0].action') == 'ready'

        # ====================================================================
        # TEST 1: Adversarial HTML/SVG Injection in Pinned & Unpinned Tab Titles
        # ====================================================================
        print("[TEST 1] Testing adversarial HTML/SVG injection into tab titles...")
        
        # Construct tabs with all adversarial XSS vectors
        xss_tabs = []
        for i, payload in enumerate(XSS_TITLE_PAYLOADS):
            # Alternate pinned and unpinned
            is_pinned = (i % 2 == 0)
            xss_tabs.append({
                'id': 100 + i,
                'title': payload,
                'url': f'https://example{i}.org/',
                'loading': False,
                'blocked': 0,
                'error': None,
                'pinned': is_pinned,
                'muted': False,
                'is_playing_audio': False
            })

        xss_state = {
            'tabs': xss_tabs,
            'active': 100,
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

        page.evaluate('window.applyState', xss_state)
        time.sleep(0.1)

        # 1. Verify zero XSS execution occurred
        assert page.evaluate('window.xss_injected === undefined'), f"XSS executed: {page.evaluate('window.xss_injected')}"

        # 2. Verify pinned tabs have NO `.tab-title` span element
        for i, tab in enumerate(xss_tabs):
            tab_id = tab['id']
            tab_el = page.locator(f'.tab[data-id="{tab_id}"]')
            expect(tab_el).to_be_visible()
            
            if tab['pinned']:
                # Pinned tab must have .pinned class
                assert 'pinned' in (tab_el.get_attribute('class') or '')
                # Pinned tab must NOT have .tab-title
                assert tab_el.locator('.tab-title').count() == 0, f"Pinned tab {tab_id} has .tab-title!"
                # Pinned tab must NOT have .tab-close
                assert tab_el.locator('.tab-close').count() == 0, f"Pinned tab {tab_id} has .tab-close!"
                # Title attribute on .tab-select should store the payload safely as a string
                assert tab_el.locator('.tab-select').get_attribute('title') == tab['title']
            else:
                # Unpinned tab must have .tab-title with safe textContent
                title_span = tab_el.locator('.tab-title')
                expect(title_span).to_have_text(tab['title'])
                # Must have close button
                assert tab_el.locator('.tab-close').count() == 1

        # 3. Ensure no unescaped rogue images or svgs exist in the tab container
        rogue_imgs = page.locator('#tabs-container img:not(.tab-fav)').count()
        assert rogue_imgs == 0, f"Found {rogue_imgs} injected <img> tags in #tabs-container!"
        
        print("[TEST 1] PASS: All HTML/SVG title injection payloads safely sanitized.")

        # ====================================================================
        # TEST 2: Audio Icon Button Click Event Bubbling / stopPropagation
        # ====================================================================
        print("[TEST 2] Testing audio icon button click event bubbling & stopPropagation...")
        
        audio_tabs = [
            {'id': 40, 'title': 'Pinned Tab (Playing)', 'url': 'https://pinned.org/stream', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': True},
            {'id': 50, 'title': 'Pinned Tab (Muted)', 'url': 'https://pinned.org/mute', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': True, 'is_playing_audio': False},
            {'id': 10, 'title': 'Active Tab (Playing)', 'url': 'https://music.org/play', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': True},
            {'id': 20, 'title': 'Background Tab (Playing)', 'url': 'https://radio.org/live', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': True},
            {'id': 30, 'title': 'Background Tab (Muted)', 'url': 'https://podcasts.org/ep1', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': True, 'is_playing_audio': False},
        ]
        
        audio_state = copy.deepcopy(xss_state)
        audio_state['tabs'] = audio_tabs
        audio_state['active'] = 10 # Tab 10 is active
        page.evaluate('window.applyState', audio_state)

        # 2.1 Click audio button on Active Tab (10)
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="10"] .tab-audio').click()
        msgs = page.evaluate('window.messages')
        assert len(msgs) == 1, f"Expected 1 IPC message, got {msgs}"
        assert msgs[0] == {'action': 'toggle_mute_tab', 'id': 10}

        # 2.2 CRITICAL: Click audio button on INACTIVE Background Tab (20)
        # Verify: speaker click must NOT switch tab or close tab!
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="20"] .tab-audio').click()
        msgs = page.evaluate('window.messages')
        assert len(msgs) == 1, f"Expected exactly 1 IPC message, got: {msgs}"
        assert msgs[0] == {'action': 'toggle_mute_tab', 'id': 20}
        # Explicitly assert no 'switch_tab' or 'close_tab'
        assert not any(m.get('action') == 'switch_tab' for m in msgs), "Clicking audio icon switched active tab!"
        assert not any(m.get('action') == 'close_tab' for m in msgs), "Clicking audio icon closed tab!"

        # 2.3 Click audio button on INACTIVE Muted Tab (30)
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="30"] .tab-audio').click()
        msgs = page.evaluate('window.messages')
        assert len(msgs) == 1
        assert msgs[0] == {'action': 'toggle_mute_tab', 'id': 30}
        assert not any(m.get('action') in ('switch_tab', 'close_tab') for m in msgs)

        # 2.4 Click audio button on Pinned Tab (40)
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="40"] .tab-audio').click()
        msgs = page.evaluate('window.messages')
        assert len(msgs) == 1
        assert msgs[0] == {'action': 'toggle_mute_tab', 'id': 40}
        assert not any(m.get('action') in ('switch_tab', 'close_tab') for m in msgs)

        # 2.5 Rapid repeated clicking on audio button (10 rapid clicks)
        page.evaluate('window.messages = []')
        audio_btn = page.locator('.tab[data-id="50"] .tab-audio')
        for _ in range(10):
            audio_btn.click()
        msgs = page.evaluate('window.messages')
        assert len(msgs) == 10
        assert all(m == {'action': 'toggle_mute_tab', 'id': 50} for m in msgs)

        print("[TEST 2] PASS: Audio button stopPropagation strictly prevents tab switching and tab closing.")

        # ====================================================================
        # TEST 3: Middle-Clicking Pinned Tabs Protection
        # ====================================================================
        print("[TEST 3] Testing middle-clicking pinned tabs protection...")
        middle_state = copy.deepcopy(xss_state)
        middle_state['tabs'] = [
            {'id': 40, 'title': 'Pinned 40', 'url': 'https://pinned.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': True},
            {'id': 50, 'title': 'Pinned 50', 'url': 'https://pinned2.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': True, 'is_playing_audio': False},
            {'id': 20, 'title': 'Unpinned 20', 'url': 'https://unpinned.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        ]
        middle_state['active'] = 20
        page.evaluate('window.applyState', middle_state)

        # 3.1 Native middle click on Pinned Tab (40) root element
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="40"]').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' and m.get('id') == 40 for m in msgs), "Middle clicking pinned tab root emitted close_tab!"

        # 3.2 Native middle click on Pinned Tab (40) tab-select button
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="40"] .tab-select').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' and m.get('id') == 40 for m in msgs), "Middle clicking pinned tab-select emitted close_tab!"

        # 3.3 Native middle click on Pinned Tab (40) favicon
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="40"] .tab-fav').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' and m.get('id') == 40 for m in msgs), "Middle clicking pinned tab-fav emitted close_tab!"

        # 3.4 Native middle click on Pinned Tab (50) audio button
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="50"] .tab-audio').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' and m.get('id') == 50 for m in msgs), "Middle clicking pinned tab audio emitted close_tab!"

        # Dismiss Chromium autoscroll anchor mode by left-clicking top-left empty header space
        page.mouse.click(10, 10)

        # 3.5 Compare with unpinned tab: Native middle-clicking unpinned Tab (20) MUST emit close_tab
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="20"]').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert any(m.get('action') == 'close_tab' and m.get('id') == 20 for m in msgs), f"Native middle clicking unpinned tab failed to emit close_tab, got: {msgs}"

        # 3.6 Synthetic auxclick dispatch on all pinned tab elements (verify zero close_tab)
        def trigger_auxclick(selector):
            return page.evaluate(f'''() => {{
                const el = document.querySelector('{selector}');
                if (!el) return false;
                return el.dispatchEvent(new MouseEvent('auxclick', {{ bubbles: true, cancelable: true, button: 1 }}));
            }}''')

        for sel in ['.tab[data-id="40"]', '.tab[data-id="40"] .tab-select', '.tab[data-id="40"] .tab-fav', '.tab[data-id="50"] .tab-audio']:
            page.evaluate('window.messages = []')
            trigger_auxclick(sel)
            assert page.evaluate('window.messages') == [], f"Pinned tab auxclick on {sel} emitted close_tab!"

        # 3.7 Synthetic auxclick dispatch on unpinned tabs MUST emit close_tab
        for sel in ['.tab[data-id="20"]', '.tab[data-id="20"] .tab-title', '.tab[data-id="20"] .tab-select']:
            page.evaluate('window.messages = []')
            trigger_auxclick(sel)
            msgs = page.evaluate('window.messages')
            assert msgs == [{'action': 'close_tab', 'id': 20}], f"Unpinned auxclick on {sel} failed: {msgs}"

        # ====================================================================
        # 3.8 Overflowing Tab Bar Autoscroll Suppression & auxclick Dispatch
        # ====================================================================
        print("  [TEST 3.8] Testing middle-clicking under tab bar overflow conditions...")
        overflow_tabs = [
            {'id': 101, 'title': 'Pinned Tab 1', 'url': 'https://p1.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': False},
            {'id': 102, 'title': 'Pinned Tab 2 (Audio)', 'url': 'https://p2.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': True},
            {'id': 103, 'title': 'Pinned Tab 3 (Muted)', 'url': 'https://p3.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': True, 'is_playing_audio': False},
            {'id': 201, 'title': 'Unpinned Tab 1 with Long Title for Overflow', 'url': 'https://u1.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
            {'id': 202, 'title': 'Unpinned Tab 2 with Long Title for Overflow', 'url': 'https://u2.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': True},
            {'id': 203, 'title': 'Unpinned Tab 3 with Long Title for Overflow', 'url': 'https://u3.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': True, 'is_playing_audio': False},
            {'id': 204, 'title': 'Unpinned Tab 4 with Long Title for Overflow', 'url': 'https://u4.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
            {'id': 205, 'title': 'Unpinned Tab 5 with Long Title for Overflow', 'url': 'https://u5.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        ]
        overflow_state = copy.deepcopy(xss_state)
        overflow_state['tabs'] = overflow_tabs
        overflow_state['active'] = 201
        page.evaluate('window.applyState', overflow_state)

        # Confirm container is physically overflowing (scrollWidth > clientWidth)
        scroll_info = page.evaluate("""() => {
            const el = document.getElementById('tabs-container');
            return {
                scrollWidth: el.scrollWidth,
                clientWidth: el.clientWidth,
                isOverflowing: el.scrollWidth > el.clientWidth
            };
        }""")
        assert scroll_info['isOverflowing'], f"Tab container must be overflowing in Test 3.8: {scroll_info}"

        # 3.8.1 Native middle click on unpinned tab 201 in overflowing container -> MUST emit close_tab
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="201"]').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert any(m.get('action') == 'close_tab' and m.get('id') == 201 for m in msgs), (
            f"Native middle-click on unpinned tab failed to dispatch close_tab under overflow conditions! "
            f"Chromium autoscroll hijacked middle-click mousedown. Messages: {msgs}"
        )

        # Reset any active pointer state
        page.mouse.click(10, 10)

        # 3.8.2 Native middle click on pinned tab 101 in overflowing container -> MUST NOT emit close_tab
        page.evaluate('window.messages = []')
        page.locator('.tab[data-id="101"]').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert not any(m.get('action') == 'close_tab' for m in msgs), (
            f"Native middle-click on pinned tab dispatched close_tab under overflow conditions: {msgs}"
        )

        # 3.8.3 Native middle click on button[data-action="new_tab"] -> 0 messages
        page.evaluate('window.messages = []')
        page.locator('button[data-action="new_tab"]').click(button='middle')
        msgs = page.evaluate('window.messages')
        assert len(msgs) == 0, f"Middle click on new_tab button emitted messages: {msgs}"

        print("[TEST 3] PASS: Middle clicking pinned tabs is completely blocked across all elements (zero close_tab emitted).")

        # ====================================================================
        # TEST 4: Drag-and-Drop Cross-Boundary Enforcement
        # ====================================================================
        print("[TEST 4] Testing drag-and-drop boundary between pinned and unpinned tabs...")

        # Setup 2 pinned tabs (1, 2) and 2 unpinned tabs (10, 20)
        dnd_tabs = [
            {'id': 1, 'title': 'Pinned 1', 'url': 'https://p1.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': False},
            {'id': 2, 'title': 'Pinned 2', 'url': 'https://p2.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': False},
            {'id': 10, 'title': 'Unpinned 10', 'url': 'https://u10.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
            {'id': 20, 'title': 'Unpinned 20', 'url': 'https://u20.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
        ]
        dnd_state = copy.deepcopy(xss_state)
        dnd_state['tabs'] = dnd_tabs
        dnd_state['active'] = 1
        page.evaluate('window.applyState', dnd_state)

        # Helper to simulate HTML5 Drag and Drop events
        def simulate_drag_and_drop(from_id, to_id):
            return page.evaluate("""
                ({ fromId, toId }) => {
                    const fromEl = document.querySelector(`.tab[data-id="${fromId}"]`);
                    const toEl = document.querySelector(`.tab[data-id="${toId}"]`);
                    if (!fromEl || !toEl) return { error: 'elements not found' };

                    const dt = new DataTransfer();
                    
                    // 1. dragstart on fromEl
                    const dragstartEvent = new DragEvent('dragstart', {
                        bubbles: true,
                        cancelable: true,
                        dataTransfer: dt
                    });
                    fromEl.dispatchEvent(dragstartEvent);

                    // 2. dragover on toEl
                    toEl.dispatchEvent(new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt }));
                    const hasDragOver = toEl.classList.contains('drag-over');

                    // 3. drop on toEl
                    toEl.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt }));

                    // 4. dragend on fromEl
                    fromEl.dispatchEvent(new DragEvent('dragend', { bubbles: true, cancelable: true, dataTransfer: dt }));

                    return {
                        hasDragOver,
                        draggedTabPinnedAfter: window.draggedTabPinned
                    };
                }
            """, {'fromId': from_id, 'toId': to_id})

        # 4.1 ADVERSARIAL: Drag Pinned (1) -> Drop Unpinned (10)
        page.evaluate('window.messages = []')
        result_cross_1 = simulate_drag_and_drop(1, 10)
        msgs = page.evaluate('window.messages')
        # Cross-boundary dragover must NOT add .drag-over class to target
        assert not result_cross_1['hasDragOver'], "Cross-boundary dragover improperly added .drag-over class!"
        assert not any(m.get('action') == 'reorder_tab' for m in msgs), f"Cross-boundary drop emitted reorder_tab: {msgs}"
        assert result_cross_1['draggedTabPinnedAfter'] is None, "window.draggedTabPinned was not cleaned up on dragend"

        # 4.2 ADVERSARIAL: Drag Unpinned (10) -> Drop Pinned (1)
        page.evaluate('window.messages = []')
        result_cross_2 = simulate_drag_and_drop(10, 1)
        msgs = page.evaluate('window.messages')
        assert not result_cross_2['hasDragOver'], "Cross-boundary dragover from unpinned to pinned improperly added .drag-over class!"
        assert not any(m.get('action') == 'reorder_tab' for m in msgs), f"Cross-boundary drop emitted reorder_tab: {msgs}"

        # 4.3 VALID: Drag Pinned (1) -> Drop Pinned (2)
        page.evaluate('window.messages = []')
        result_valid_pinned = simulate_drag_and_drop(1, 2)
        msgs = page.evaluate('window.messages')
        assert result_valid_pinned['hasDragOver'], "Valid pinned-to-pinned dragover did not add .drag-over class!"
        assert any(m == {'action': 'reorder_tab', 'from': 1, 'to': 2} for m in msgs), f"Failed to emit reorder_tab for pinned tabs: {msgs}"

        # 4.4 VALID: Drag Unpinned (10) -> Drop Unpinned (20)
        page.evaluate('window.messages = []')
        result_valid_unpinned = simulate_drag_and_drop(10, 20)
        msgs = page.evaluate('window.messages')
        assert result_valid_unpinned['hasDragOver'], "Valid unpinned-to-unpinned dragover did not add .drag-over class!"
        assert any(m == {'action': 'reorder_tab', 'from': 10, 'to': 20} for m in msgs), f"Failed to emit reorder_tab for unpinned tabs: {msgs}"

        print("[TEST 4] PASS: Drag-and-drop boundaries strictly enforce pinned/unpinned isolation.")

        # ====================================================================
        # TEST 5: Shortcuts Verification (Ctrl+M, Ctrl+Shift+K)
        # ====================================================================
        print("[TEST 5] Testing keyboard shortcuts (Ctrl+M, Ctrl+Shift+K)...")
        
        # Test Ctrl+M
        page.evaluate('window.messages = []')
        page.keyboard.press('Control+m')
        msgs = page.evaluate('window.messages')
        assert any(m == {'action': 'toggle_mute_tab', 'id': 1} for m in msgs), f"Ctrl+M did not emit toggle_mute_tab: {msgs}"

        # Test Ctrl+Shift+K
        page.evaluate('window.messages = []')
        page.keyboard.press('Control+Shift+K')
        msgs = page.evaluate('window.messages')
        assert any(m == {'action': 'duplicate_tab', 'id': 1} for m in msgs), f"Ctrl+Shift+K did not emit duplicate_tab: {msgs}"

        print("[TEST 5] PASS: Keyboard shortcuts emit correct IPC actions.")

        assert not errors, f"Unexpected page errors: {errors}"
        browser.close()

    print("ALL M2 ADVERSARIAL DOM TESTS PASSED EMPIRICALLY!")

if __name__ == '__main__':
    run_tests()
