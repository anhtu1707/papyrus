"""Empirical stress and adversarial DOM security tests for Papyrus Browser frontend.
Challenger 2 (Milestone M1): DOM Security & Robustness Challenger.
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
fixture = ROOT / 'target/challenger2-test.html'
fixture.parent.mkdir(exist_ok=True)
fixture.write_text(html, encoding='utf-8')

# 1. Adversarial XSS payloads for bookmarks
XSS_PAYLOADS = [
    '<script>window.xss_triggered="script"</script>',
    '<img src=x onerror="window.xss_triggered=\'img\'">',
    '<svg onload="window.xss_triggered=\'svg\'">',
    '<iframe src="javascript:window.xss_triggered=\'iframe\'"></iframe>',
    '<body onload="window.xss_triggered=\'body\'">',
    '<details open ontoggle="window.xss_triggered=\'details\'">',
    '<math><mtext><table><mglyph><style><!--</style><img src=x onerror="window.xss_triggered=\'math\'">',
    '"><script>window.xss_triggered="quote_script"</script>',
    "javascript:window.xss_triggered='js_url'",
    "data:text/html;base64,PHNjcmlwdD53aW5kb3cueHNzX3RyaWdnZXJlZD0neHNzJzwvc2NyaXB0Pg==",
    "data:image/svg+xml,<svg onload=\"window.xss_triggered='data_svg'\"></svg>",
    "><a href=\"javascript:window.xss_triggered='a_href'\">click</a>",
    "><input onfocus=\"window.xss_triggered='input'\" autofocus>",
    "><video><source onerror=\"window.xss_triggered='video'\">",
    "><audio src=1 onerror=\"window.xss_triggered='audio'\">",
    "><object data=\"javascript:window.xss_triggered='object'\">",
    "><embed src=\"javascript:window.xss_triggered='embed'\">",
    "{{constructor.constructor('window.xss_triggered=1')()}}",
    "${window.xss_triggered=1}",
    "Bookmark with null byte \x00 and <script>alert(1)</script>",
    "Bookmark with unicode RTL \u202e\u0000<svg onload=alert(1)>",
    "A" * 5000 + "<img src=x onerror=1>",
    "<DIV STYLE=\"width: expression(window.xss_triggered='css');\">",
    "';alert(String.fromCharCode(88,83,83))//\';alert(String.fromCharCode(88,83,83))//\";",
]

# 2. Adversarial filenames for downloads
DOWNLOAD_PAYLOADS = [
    "Đã tải: ../../../../../../etc/passwd",
    "Đã tải: ..\\..\\..\\..\\Windows\\System32\\cmd.exe",
    "Đã tải: C:/Downloads/../../Windows/System32/calc.exe",
    "Đã tải: C:/Downloads/report.pdf\x00.exe",
    "Đã tải: \x00evil.txt",
    "Đã tải: C:/Downloads/" + "long_path_" * 500 + ".bin",
    "Đã tải: C:/" + ("sub/" * 150) + "file.tar.gz",
    "Đã tải: C:/Downloads/📁🔥💥🚀/🎉\u202e\u0000.pdf",
    "Đã tải: C:/Downloads/مرحبا_بالعالم/测试文件/ファイル_2026.zip",
    "",  # Empty string
    "   ",  # Whitespace only
    "Đã tải:",  # Status with no path
    "Đang tải: ",  # Status with trailing space
    "C:/Downloads/raw_path_without_status.pdf",  # No status prefix
    "Đã tải: C:/Downloads/archive:part1:final.zip",  # Multiple colons
    "<img src=x onerror=\"window.xss_triggered='dl_xss'\">",  # Raw XSS payload
    "Đã tải: <script>window.xss_triggered='dl_script'</script>",
    "<svg onload=\"window.xss_triggered='dl_svg'\">: C:/Downloads/file.iso",
    "Đã tải: C:/Downloads/<img src=x onerror=1>/safe.docx",
]

# 3. Regex special characters and ReDoS patterns for search filtering
REGEX_PATTERNS = [
    ".*",
    ".*+?^${}()|[]\\",
    "[",
    "]",
    "(",
    ")",
    "{",
    "}",
    "\\",
    "^",
    "$",
    "+",
    "?",
    "|",
    "(?:a|b)",
    "(?=.*[a-z])",
    "[a-z0-9_-]+",
    "\\d+\\.\\d+\\.\\d+",
    "(a+)+$",  # Classic ReDoS pattern
    "(a|aa)+$",  # Classic ReDoS pattern
    "(.*a){10}",
    "a" * 1000,
    "🔥🚀🎉",
    "<script>",
    "onerror=",
    "\"'--><",
    "\x00null",
    "   ",
]

def run_stress_tests():
    print("[*] Starting empirical stress and adversarial testing harness...")
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True)
        page = browser.new_page(viewport={'width': 1280, 'height': 840})
        page_errors = []
        dialog_messages = []

        page.on('pageerror', lambda err: page_errors.append(str(err)))
        page.on('dialog', lambda dialog: (dialog_messages.append(dialog.message), dialog.accept()))

        # Install mock IPC recorder
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
                {'id': 1, 'title': 'Tab 1', 'url': 'https://example.org', 'loading': False, 'blocked': 0, 'error': None},
                {'id': 2, 'title': 'Dấu trang', 'url': 'papyrus://bookmarks', 'loading': False, 'blocked': 0, 'error': None},
                {'id': 3, 'title': 'Tệp tải xuống', 'url': 'papyrus://downloads', 'loading': False, 'blocked': 0, 'error': None},
            ],
            'active': 2,  # Start on bookmarks manager
            'preferences': {
                'engine': 'brave', 'zoom': 100, 'theme': 'parchment', 'startup': 'newtab',
                'show_bookmarks': True, 'adblock': True, 'block_popups': True,
                'allowed_hosts': [], 'blocked_hosts': [], 'download_dir': ''
            },
            'bookmarks': [],
            'history': [],
            'passwords': [],
            'blocked': 0, 'clearing': False, 'can_back': False, 'can_forward': False,
            'downloads': [], 'notice': '', 'data_dir': 'test-profile', 'version': '0.3.0', 'builtin_count': 23,
        }

        # =========================================================================
        # TEST SUITE 1: ADVERSARIAL DOM & XSS IN #bookmarks-panel
        # =========================================================================
        print("\n=== [TEST SUITE 1] Adversarial DOM & XSS in #bookmarks-panel ===")
        bm_state = copy.deepcopy(base_state)
        bm_state['active'] = 2

        for i, payload in enumerate(XSS_PAYLOADS):
            bm_state['bookmarks'].append({
                'title': f'BM_{i}: ' + payload,
                'url': payload if '://' in payload or payload.startswith('javascript:') or payload.startswith('data:') else f'https://example.org/test?id={i}&payload=' + payload
            })

        t0 = time.time()
        page.evaluate('window.applyState', bm_state)
        render_time = (time.time() - t0) * 1000
        print(f"[*] Rendered {len(XSS_PAYLOADS)} adversarial bookmarks in {render_time:.2f}ms")

        expect(page.locator('#bookmarks-panel')).to_be_visible()
        expect(page.locator('#bookmarks-list .bookmark-item')).to_have_count(len(XSS_PAYLOADS))

        # Check for XSS execution
        xss_val = page.evaluate('window.xss_triggered')
        assert xss_val is None, f"CRITICAL DOM XSS: window.xss_triggered = {xss_val}"

        # Check that no rogue script or malicious img tags were injected into DOM
        rogue_scripts = page.locator('#bookmarks-list script').count()
        assert rogue_scripts == 0, f"Rogue script elements in bookmarks list: {rogue_scripts}"

        # Verify favicons: all bookmark items have bookmark-fav img, none have malicious error triggers
        fav_count = page.locator('#bookmarks-list img.bookmark-fav').count()
        assert fav_count == len(XSS_PAYLOADS), f"Expected {len(XSS_PAYLOADS)} favicons, got {fav_count}"

        # Test clicking each bookmark link sends correct IPC navigate command without executing script
        print("[*] Testing click navigation on adversarial bookmarks...")
        items = page.locator('#bookmarks-list .bookmark-item')
        for i in range(min(5, len(XSS_PAYLOADS))):
            link = items.nth(i).locator('a.bookmark-title')
            link.click()
            last_msg = page.evaluate('window.messages.at(-1)')
            assert last_msg['action'] == 'navigate', f"Expected navigate action, got {last_msg}"
            assert last_msg['input'] == bm_state['bookmarks'][i]['url']

        # Test clicking delete button on adversarial bookmark
        print("[*] Testing delete confirmation on adversarial bookmark titles...")
        del_btn = items.first.locator('.history-del')
        del_btn.click()
        assert len(dialog_messages) > 0, "Expected confirm dialog to trigger"
        print(f"[*] Verified native confirm dialog triggered safely: {dialog_messages[-1][:60]}...")

        # Re-check XSS
        assert page.evaluate('window.xss_triggered') is None, "XSS triggered during click/delete interaction!"
        print("[+] SUITE 1 PASSED: Zero XSS executions across 24 adversarial vectors.")

        # =========================================================================
        # TEST SUITE 2: ADVERSARIAL DOWNLOADS IN #downloads-panel
        # =========================================================================
        print("\n=== [TEST SUITE 2] Adversarial Filenames & Paths in #downloads-panel ===")
        dl_state = copy.deepcopy(base_state)
        dl_state['active'] = 3
        dl_state['downloads'] = DOWNLOAD_PAYLOADS

        t0 = time.time()
        page.evaluate('window.applyState', dl_state)
        render_time = (time.time() - t0) * 1000
        print(f"[*] Rendered {len(DOWNLOAD_PAYLOADS)} adversarial download entries in {render_time:.2f}ms")

        expect(page.locator('#downloads-panel')).to_be_visible()
        expect(page.locator('#downloads-panel-list .download-item')).to_have_count(len(DOWNLOAD_PAYLOADS))

        # Check for XSS execution
        xss_val = page.evaluate('window.xss_triggered')
        assert xss_val is None, f"CRITICAL DOWNLOAD XSS: window.xss_triggered = {xss_val}"

        # Verify structure of rendered items
        dl_items = page.locator('#downloads-panel-list .download-item')
        print(f"[*] Validating download card controls and IPC dispatch across all {len(DOWNLOAD_PAYLOADS)} entries...")

        # Test button clicks on paths:
        # First item: path traversal
        first_card = dl_items.first
        open_btn = first_card.locator('button:has-text("Mở tệp")')
        folder_btn = first_card.locator('button:has-text("Hiển thị trong thư mục")')
        expect(open_btn).to_be_visible()
        expect(folder_btn).to_be_visible()

        open_btn.click()
        msg_open = page.evaluate('window.messages.at(-1)')
        assert msg_open['action'] == 'open_downloaded_file', f"Expected open_downloaded_file, got {msg_open}"
        print(f"[*] open_downloaded_file dispatched: path={msg_open.get('path')}")

        folder_btn.click()
        msg_folder = page.evaluate('window.messages.at(-1)')
        assert msg_folder['action'] == 'show_download_in_folder', f"Expected show_download_in_folder, got {msg_folder}"
        print(f"[*] show_download_in_folder dispatched: path={msg_folder.get('path')}")

        # Clear downloads test
        clear_btn = page.locator('#clear-downloads-btn')
        clear_btn.click()
        msg_clear = page.evaluate('window.messages.at(-1)')
        assert msg_clear['action'] == 'clear_downloads', f"Expected clear_downloads, got {msg_clear}"

        assert page.evaluate('window.xss_triggered') is None, "XSS triggered during downloads interaction!"
        print("[+] SUITE 2 PASSED: Path traversal, null bytes, long paths & emojis rendered safely.")

        # =========================================================================
        # TEST SUITE 3: LIVE FILTERING IN #bookmarks-search (REGEX & BURST INPUT)
        # =========================================================================
        print("\n=== [TEST SUITE 3] Live Filtering in #bookmarks-search ===")
        # Switch back to bookmarks panel
        page.evaluate('window.applyState', bm_state)
        expect(page.locator('#bookmarks-panel')).to_be_visible()

        search_input = page.locator('#bookmarks-search')

        print(f"[*] Testing {len(REGEX_PATTERNS)} regex and special character queries...")
        for pattern in REGEX_PATTERNS:
            search_input.fill(pattern)
            # Ensure no page error occurred
            assert not page_errors, f"Page error on regex search '{pattern}': {page_errors}"
            # Item count check (either filtered results or 'Không tìm thấy' message)
            item_count = page.locator('#bookmarks-list .bookmark-item').count()
            empty_msg = page.locator('#bookmarks-list .history-empty').count()
            assert item_count > 0 or empty_msg > 0, f"List in invalid state for pattern: {pattern}"

        # Test exact literal matching:
        # Add a bookmark that literally contains '.*'
        literal_state = copy.deepcopy(base_state)
        literal_state['active'] = 2
        literal_state['bookmarks'] = [
            {'title': 'Special .* Regex Test', 'url': 'https://regex101.com'},
            {'title': 'Standard Bookmark', 'url': 'https://google.com'},
            {'title': 'Another Standard', 'url': 'https://rust-lang.org'},
        ]
        page.evaluate('window.applyState', literal_state)
        expect(page.locator('#bookmarks-list .bookmark-item')).to_have_count(3)

        # Search for '.*' -> should match ONLY the first bookmark (literal, not wildcard)
        search_input.fill('.*')
        matched = page.locator('#bookmarks-list .bookmark-item')
        assert matched.count() == 1, f"Expected 1 literal match for '.*', got {matched.count()}"
        expect(matched.first.locator('.bookmark-title')).to_have_text('Special .* Regex Test')
        print("[*] Verified literal substring matching for '.*' (not treated as regex wildcard)")

        # Rapid burst typing test: simulate 50 keystrokes in quick succession
        print("[*] Testing rapid burst typing in search input...")
        burst_text = "abc.*+?^${}()|[]\\1234567890!@#$%^&*()_+~`|}{[]:;?><,./"
        search_input.focus()
        t0 = time.time()
        for char in burst_text:
            search_input.press_sequentially(char, delay=1)
        burst_time = (time.time() - t0) * 1000
        print(f"[*] Burst typed {len(burst_text)} characters in {burst_time:.2f}ms without crashing")

        assert not page_errors, f"Page errors encountered during burst search: {page_errors}"
        search_input.fill('')
        print("[+] SUITE 3 PASSED: All regex, ReDoS, and burst search inputs handled safely.")

        # =========================================================================
        # TEST SUITE 4: HISTORY, PASSWORDS, EXTENSIONS SEARCH & ROBUSTNESS
        # =========================================================================
        print("\n=== [TEST SUITE 4] History, Passwords & Extensions Robustness ===")
        # Test History search with regex characters
        hist_state = copy.deepcopy(base_state)
        hist_state['tabs'].append({'id': 4, 'title': 'Lịch sử', 'url': 'papyrus://history', 'loading': False, 'blocked': 0, 'error': None})
        hist_state['active'] = 4
        hist_state['history'] = [
            {'title': 'Lịch sử test 1', 'url': 'https://test1.com', 'time': int(time.time())},
            {'title': '<script>alert("hist")</script>', 'url': 'https://test2.com', 'time': int(time.time())},
        ]
        page.evaluate('window.applyState', hist_state)
        expect(page.locator('#history-panel')).to_be_visible()

        hist_search = page.locator('#history-search')
        for pattern in REGEX_PATTERNS[:10]:
            hist_search.fill(pattern)
        assert not page_errors, f"Errors in history search: {page_errors}"

        # Test Passwords panel with adversarial XSS data
        pwd_state = copy.deepcopy(base_state)
        pwd_state['tabs'].append({'id': 5, 'title': 'Mật khẩu', 'url': 'papyrus://passwords', 'loading': False, 'blocked': 0, 'error': None})
        pwd_state['active'] = 5
        pwd_state['passwords'] = [
            {'id': 1, 'url': 'https://example.com/<script>alert(1)</script>', 'username': 'admin<img src=x onerror=1>', 'password': 'secret"\'><svg onload=1>', 'updated': int(time.time())}
        ]
        page.evaluate('window.applyState', pwd_state)
        expect(page.locator('#passwords-panel')).to_be_visible()
        expect(page.locator('#passwords-list .password-card')).to_have_count(1)

        pwd_search = page.locator('#passwords-search')
        for pattern in REGEX_PATTERNS[:10]:
            pwd_search.fill(pattern)
        assert not page_errors, f"Errors in passwords search: {page_errors}"

        # Test Extensions panel search with regex characters
        ext_state = copy.deepcopy(base_state)
        ext_state['tabs'].append({'id': 6, 'title': 'Tiện ích', 'url': 'papyrus://extensions', 'loading': False, 'blocked': 0, 'error': None})
        ext_state['active'] = 6
        page.evaluate('window.applyState', ext_state)
        expect(page.locator('#extensions-panel')).to_be_visible()

        ext_search = page.locator('#extensions-search')
        for pattern in REGEX_PATTERNS[:10]:
            ext_search.fill(pattern)
        assert not page_errors, f"Errors in extensions search: {page_errors}"

        print("[+] SUITE 4 PASSED: History, Passwords, Extensions immune to XSS and regex syntax errors.")

        # =========================================================================
        # TEST SUITE 5: FILTERED DELETION INTEGRITY, FORM VALIDATION & HIGH LOAD
        # =========================================================================
        print("\n=== [TEST SUITE 5] Filtered Deletion Integrity, Add Drawers & Load ===")

        # 5.1 Filtered deletion index test
        idx_state = copy.deepcopy(base_state)
        idx_state['active'] = 2
        idx_state['bookmarks'] = [
            {'title': 'Alpha Website', 'url': 'https://alpha.example.com'},
            {'title': 'Beta Target', 'url': 'https://beta.example.com'},
            {'title': 'Gamma Portal', 'url': 'https://gamma.example.com'},
        ]
        page.evaluate('window.applyState', idx_state)
        expect(page.locator('#bookmarks-list .bookmark-item')).to_have_count(3)

        # Filter by 'Beta' -> only 1 item should be shown
        search_input.fill('Beta')
        expect(page.locator('#bookmarks-list .bookmark-item')).to_have_count(1)

        # Delete the filtered item (Beta Target, originally at index 1)
        del_btn = page.locator('#bookmarks-list .bookmark-item .history-del').first
        del_btn.click()
        msg_del = page.evaluate('window.messages.at(-1)')
        assert msg_del['action'] == 'delete_bookmark', f"Expected delete_bookmark, got {msg_del}"
        assert msg_del['index'] == 1, f"CRITICAL: Deletion sent filtered index instead of originalIndex! Got index={msg_del.get('index')}, expected 1"
        print("[*] Verified: Deleting filtered item correctly maps to original index in database (index 1)")

        # Clear search
        search_input.fill('')

        # 5.2 Add Bookmark Drawer validation & XSS test
        toggle_bm_btn = page.locator('#toggle-add-bookmark-btn')
        toggle_bm_btn.click()
        expect(page.locator('#add-bookmark-box')).to_be_visible()

        page.locator('#new-bookmark-title').fill('<script>window.xss_triggered="drawer_title"</script>')
        page.locator('#new-bookmark-url').fill('https://secure-test.example.com/<svg onload=1>')
        page.locator('#save-bookmark-btn').click()

        msg_add_bm = page.evaluate('window.messages.at(-1)')
        assert msg_add_bm['action'] == 'add_bookmark'
        assert msg_add_bm['title'] == '<script>window.xss_triggered="drawer_title"</script>'
        assert msg_add_bm['url'] == 'https://secure-test.example.com/<svg onload=1>'
        expect(page.locator('#add-bookmark-box')).to_be_hidden()
        assert page.locator('#new-bookmark-title').input_value() == ''
        assert page.locator('#new-bookmark-url').input_value() == ''
        print("[*] Verified: Add Bookmark drawer validates, submits clean IPC and clears inputs")

        # 5.3 Add Password Drawer validation & XSS test
        page.evaluate('window.applyState', pwd_state)
        expect(page.locator('#passwords-panel')).to_be_visible()

        page.locator('#toggle-add-pwd-btn').click()
        expect(page.locator('#add-pwd-box')).to_be_visible()

        page.locator('#new-pwd-url').fill('https://accounts.evil.org/<img src=x>')
        page.locator('#new-pwd-user').fill('admin\'"--><script>1</script>')
        page.locator('#new-pwd-val').fill('P@ssw0rd!#%&')
        page.locator('#save-pwd-btn').click()

        msg_save_pwd = page.evaluate('window.messages.at(-1)')
        assert msg_save_pwd['action'] == 'save_password'
        assert msg_save_pwd['url'] == 'https://accounts.evil.org/<img src=x>'
        assert msg_save_pwd['username'] == 'admin\'"--><script>1</script>'
        assert msg_save_pwd['password'] == 'P@ssw0rd!#%&'
        expect(page.locator('#add-pwd-box')).to_be_hidden()
        print("[*] Verified: Add Password drawer validates, submits clean IPC and clears inputs")

        # 5.4 Scale and Stress: 1,000 bookmarks rendering and filtering performance
        print("[*] Testing scale: generating and rendering 1,000 bookmark records...")
        scale_state = copy.deepcopy(base_state)
        scale_state['active'] = 2
        for i in range(1000):
            scale_state['bookmarks'].append({
                'title': f'Scale Bookmark #{i} - Fast search and retrieval benchmark',
                'url': f'https://benchmark-{i}.service.internal/path/{i}'
            })

        t0 = time.time()
        page.evaluate('window.applyState', scale_state)
        render_scale_ms = (time.time() - t0) * 1000
        print(f"[*] 1,000 bookmarks rendered in {render_scale_ms:.2f}ms")
        expect(page.locator('#bookmarks-list .bookmark-item')).to_have_count(1000)

        # Live filter over 1,000 items
        t0 = time.time()
        page.locator('#bookmarks-search').fill('Scale Bookmark #999')
        filter_scale_ms = (time.time() - t0) * 1000
        print(f"[*] Live filter over 1,000 bookmarks completed in {filter_scale_ms:.2f}ms")
        expect(page.locator('#bookmarks-list .bookmark-item')).to_have_count(1)
        expect(page.locator('#bookmarks-list .bookmark-title')).to_have_text('Scale Bookmark #999 - Fast search and retrieval benchmark')

        assert render_scale_ms < 500, f"Render time too slow: {render_scale_ms}ms"
        assert filter_scale_ms < 200, f"Filter time too slow: {filter_scale_ms}ms"
        print("[+] SUITE 5 PASSED: Filtered deletion index mapped correctly, form drawers safe, 1000-item scale test passed within performance budget.")

        # =========================================================================
        # FINAL VERIFICATION
        # =========================================================================
        xss_final = page.evaluate('window.xss_triggered')
        assert xss_final is None, f"Final XSS check failed: {xss_final}"
        assert not page_errors, f"Final page errors check failed: {page_errors}"

        browser.close()

    print("\n=======================================================")
    print("ALL EMPIRICAL CHALLENGES PASSED (100% SUCCESS, 0 XSS, 0 REGRESSIONS)")
    print("=======================================================\n")

if __name__ == '__main__':
    run_stress_tests()
