"""Adversarial Playwright stress tests for Milestone M3:
1. Multi-line ABP form input parsing, whitespace & comment stripping, and valid rule preservation.
2. Real-time omnibox shield counter and shield dropdown synchronization without navigation or tab switching.
"""
import copy
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[1]
html = (ROOT / 'assets/header.html').read_text(encoding='utf-8')
html = html.replace('/*HEADER_CSS*/', (ROOT / 'assets/header.css').read_text(encoding='utf-8'))
html = html.replace('/*HEADER_JS*/', (ROOT / 'assets/header.js').read_text(encoding='utf-8'))
fixture = ROOT / 'target/challenger-m3-2-frontend-test.html'
fixture.parent.mkdir(exist_ok=True)
fixture.write_text(html, encoding='utf-8')

def run_tests():
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        page = browser.new_page(viewport={'width': 1280, 'height': 840})
        errors = []
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.add_init_script("window.messages=[];window.ipc={postMessage:s=>window.messages.push(JSON.parse(s))}")
        page.goto(fixture.as_uri(), wait_until='networkidle')

        initial_state = {
            'tabs': [
                {'id': 1, 'title': 'News Portal', 'url': 'https://news.com/', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
                {'id': 2, 'title': 'Tech Blog', 'url': 'https://example.org/', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
                {'id': 3, 'title': 'Cài đặt', 'url': 'papyrus://settings', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': False},
            ],
            'active': 1,
            'preferences': {
                'engine': 'google',
                'zoom': 100,
                'theme': 'parchment',
                'startup': 'newtab',
                'show_bookmarks': True,
                'adblock': True,
                'block_popups': True,
                'allowed_hosts': [],
                'blocked_hosts': [],
                'download_dir': '',
            },
            'bookmarks': [],
            'history': [],
            'passwords': [],
            'blocked': 0,
            'clearing': False,
            'can_back': True,
            'can_forward': False,
            'downloads': [],
            'notice': '',
            'data_dir': 'test-profile',
            'version': '0.3.7',
            'builtin_count': 64,
        }

        page.evaluate('window.applyState', initial_state)

        # ====================================================================
        # TEST SUITE 1: Real-time Omnibox & Shield Dropdown Counter Sync
        # ====================================================================
        print("[TEST 1] Verifying real-time omnibox and shield counter synchronization...")
        expect(page.locator('#adblock-counter')).to_have_text('0')
        expect(page.locator('#blocked-total')).to_have_text('0')

        # Stream of 10 blocked ad requests occurring on active tab 1
        current_state = copy.deepcopy(initial_state)
        for count in range(1, 11):
            current_state['tabs'][0]['blocked'] = count
            current_state['blocked'] = count
            # State update arrives via applyState without tab switch or navigation
            page.evaluate('window.applyState', current_state)
            
            # Omnibox counter MUST reflect active tab's blocked count immediately
            expect(page.locator('#adblock-counter')).to_have_text(str(count))
            # Total blocked count MUST reflect total blocked count immediately
            expect(page.locator('#blocked-total')).to_have_text(str(count))

        print("  [+] Stream of 10 blocked requests updated DOM counters immediately without navigation.")

        # Toggle shield dropdown open
        page.locator('#shield-toggle').click()
        expect(page.locator('#shield-panel')).to_be_visible()
        expect(page.locator('#blocked-total')).to_have_text('10')
        expect(page.locator('#filter-status')).to_have_text('Bộ lọc đang bật')

        # Additional blocked request while shield dropdown is visibly open
        current_state['tabs'][0]['blocked'] = 11
        current_state['blocked'] = 11
        page.evaluate('window.applyState', current_state)
        expect(page.locator('#adblock-counter')).to_have_text('11')
        expect(page.locator('#blocked-total')).to_have_text('11')
        print("  [+] Shield dropdown reflects real-time counter while open.")

        # Close shield dropdown
        page.locator('#shield-close').click()
        expect(page.locator('#shield-panel')).to_be_hidden()

        # Tab isolation test: background tab 2 blocks 4 requests
        current_state['tabs'][1]['blocked'] = 4
        current_state['blocked'] = 15
        page.evaluate('window.applyState', current_state)
        # Active tab is still Tab 1, so omnibox counter must remain 11
        expect(page.locator('#adblock-counter')).to_have_text('11')
        expect(page.locator('#blocked-total')).to_have_text('15')

        # Switch active tab to Tab 2
        current_state['active'] = 2
        page.evaluate('window.applyState', current_state)
        expect(page.locator('#adblock-counter')).to_have_text('4')
        expect(page.locator('#blocked-total')).to_have_text('15')
        print("  [+] Tab isolation verified: omnibox counter tracks active tab blocked count accurately.")

        # ====================================================================
        # TEST SUITE 2: Settings Form Multi-line ABP Input & Normalization
        # ====================================================================
        print("[TEST 2] Verifying multi-line ABP list parsing, comment stripping, and rule preservation...")
        # Switch to settings tab
        current_state['active'] = 3
        page.evaluate('window.applyState', current_state)
        expect(page.locator('#settings-panel')).to_be_visible()

        # Adversarial multi-line input for blocked_hosts:
        # Includes leading/trailing whitespace, blank lines, carriage returns, ! comments, # comments, uppercase rules, duplicate rules
        blocked_input = (
            "! EasyList Vietnamese snippet\r\n"
            "   ||ads.doubleclick.net^   \r\n"
            "\r\n"
            "# Another comment style\r\n"
            "||TRACKER.ANALYTICS.COM^$script\r\n"
            "  \r\n"
            "*.telemetry.evil.com\r\n"
            "||ads.doubleclick.net^\r\n"
            "||popunder.net/ad/*\r\n"
            "! End of blocklist\r\n"
        )

        allowed_input = (
            "! Whitelist section\n"
            "   news.com   \n"
            "\n"
            "@@||LEGIT-SITE.COM^$document\n"
            "# Whitelist font CDN\n"
            "@@||fonts.googleapis.com^\n"
            "news.com\n"
        )

        page.locator('#blocked_hosts').fill(blocked_input)
        page.locator('#allowed_hosts').fill(allowed_input)

        # Clear captured IPC messages before submitting
        page.evaluate('window.messages = []')

        # Submit settings form
        page.locator('#settings-form button[type=submit]').click()

        # Check emitted save_preferences message
        save_msg = page.evaluate('window.messages.find(m => m.action === "save_preferences")')
        assert save_msg is not None, "save_preferences IPC message was not dispatched!"
        prefs = save_msg['preferences']

        # Verify blocked_hosts:
        expected_blocked = [
            '||ads.doubleclick.net^',
            '||tracker.analytics.com^$script',
            '*.telemetry.evil.com',
            '||popunder.net/ad/*',
        ]
        assert prefs['blocked_hosts'] == expected_blocked, (
            f"Blocked hosts mismatch! Expected {expected_blocked}, got {prefs['blocked_hosts']}"
        )

        # Verify allowed_hosts:
        expected_allowed = [
            'news.com',
            '@@||legit-site.com^$document',
            '@@||fonts.googleapis.com^',
        ]
        assert prefs['allowed_hosts'] == expected_allowed, (
            f"Allowed hosts mismatch! Expected {expected_allowed}, got {prefs['allowed_hosts']}"
        )

        print("  [+] Multi-line ABP lists parsed, trimmed, comments stripped, deduplicated, and preserved perfectly.")

        # ====================================================================
        # TEST SUITE 3: Unsaved Edits vs State Application Stability
        # ====================================================================
        print("[TEST 3] Verifying unsaved dirty form preservation...")
        page.locator('#blocked_hosts').fill('||custom-unsaved.net^')
        # Simulate state update arriving from backend (e.g. background tab title change or block count update)
        page.evaluate('window.applyState', current_state)
        # Form field must NOT be clobbered because preferencesDirty is true
        expect(page.locator('#blocked_hosts')).to_have_value('||custom-unsaved.net^')
        print("  [+] Live state does not overwrite unsaved textarea input.")

        # Ensure zero JavaScript errors occurred throughout
        assert len(errors) == 0, f"JavaScript runtime errors occurred: {errors}"
        print("\n=======================================================")
        print("ALL CHALLENGER M3-2 EMPIRICAL PLAYWRIGHT TESTS PASSED!")
        print("=======================================================")

        browser.close()

if __name__ == '__main__':
    run_tests()
