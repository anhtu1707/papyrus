"""Headless regression tests for the exact HTML/CSS/JS shipped in the executable."""
import copy
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

ROOT = Path(__file__).resolve().parents[1]
html = (ROOT / 'assets/header.html').read_text(encoding='utf-8')
html = html.replace('/*HEADER_CSS*/', (ROOT / 'assets/header.css').read_text(encoding='utf-8'))
html = html.replace('/*HEADER_JS*/', (ROOT / 'assets/header.js').read_text(encoding='utf-8'))
fixture = ROOT / 'target/frontend-test.html'
fixture.parent.mkdir(exist_ok=True)
fixture.write_text(html, encoding='utf-8')
snapshot = {
    'tabs': [{'id': 1, 'title': '<img src=x onerror="window.injected=true">', 'url': 'https://example.org/settings.html', 'loading': False, 'blocked': 2, 'error': None}],
    'active': 1, 'preferences': {'engine': 'brave', 'zoom': 125, 'theme': 'kraft', 'startup': 'restore', 'show_bookmarks': True, 'adblock': True, 'block_popups': True, 'allowed_hosts': [], 'blocked_hosts': [], 'download_dir': ''},
    'bookmarks': [{'title': '<svg onload="window.injected=true">', 'url': "https://example.org/?q='quoted'"}],
    'blocked': 2, 'clearing': False, 'can_back': True, 'can_forward': False,
    'downloads': ['Đã tải: C:/Downloads/report.pdf'], 'notice': '', 'data_dir': 'test-profile', 'version': '0.3.0', 'builtin_count': 23,
}
with sync_playwright() as p:
    browser = p.chromium.launch(headless=True)
    page = browser.new_page(viewport={'width': 1280, 'height': 840})
    errors = []
    page.on('pageerror', lambda error: errors.append(str(error)))
    page.add_init_script("window.messages=[];window.ipc={postMessage:s=>window.messages.push(JSON.parse(s))}")
    page.goto(fixture.as_uri(), wait_until='networkidle')
    assert page.evaluate('window.messages[0].action') == 'ready'
    page.evaluate('window.applyState', snapshot)
    expect(page.locator('.tab-select')).to_have_text(snapshot['tabs'][0]['title'])
    assert page.locator('#tabs-container img:not(.tab-fav), #pills-container :not(.pill-del) > svg').count() == 0
    assert page.evaluate('!!window.injected') is False
    expect(page.locator('#url')).to_have_value('https://example.org/settings.html')
    page.locator('#pills-container .pill button').first.click()
    assert page.evaluate('window.messages.at(-1).input') == snapshot['bookmarks'][0]['url']
    page.locator('[data-action=new_tab]').click()
    assert page.evaluate('window.messages.at(-1).action') == 'new_tab'
    page.locator('#url').fill('Rust 1.90 release')
    page.locator('#url').press('Enter')
    assert page.evaluate('window.messages.at(-1).input') == 'Rust 1.90 release'
    settings = copy.deepcopy(snapshot)
    settings['tabs'].append({'id': 2, 'title': 'Cài đặt', 'url': 'papyrus://settings', 'loading': False, 'blocked': 0, 'error': None})
    settings['active'] = 2
    page.evaluate('window.applyState', settings)
    expect(page.locator('#settings-panel')).to_be_visible()
    # Vietnamese headings must not mix Georgia with Times for missing accents.
    cdp = page.context.new_cdp_session(page)
    cdp.send('DOM.enable')
    cdp.send('CSS.enable')
    document = cdp.send('DOM.getDocument')['root']['nodeId']
    heading = cdp.send('DOM.querySelector', {'nodeId': document, 'selector': '.settings-nav h1'})['nodeId']
    rendered_fonts = cdp.send('CSS.getPlatformFontsForNode', {'nodeId': heading})['fonts']
    assert len({font['familyName'] for font in rendered_fonts}) == 1, rendered_fonts
    expect(page.locator('#engine')).to_have_value('brave')
    expect(page.locator('#zoom')).to_have_value('125')
    page.locator('#theme').select_option('dark')
    page.locator('#blocked_hosts').fill('ads.example.org\nads.example.org')
    page.locator('#allowed_hosts').fill('www.example.org')
    page.evaluate('window.applyState', settings)
    expect(page.locator('#theme')).to_have_value('dark')  # live state must not overwrite an unsaved edit
    page.locator('#settings-form button[type=submit]').click()
    prefs = page.evaluate('window.messages.at(-1)')
    assert prefs['action'] == 'save_preferences'
    assert prefs['preferences']['theme'] == 'dark'
    assert prefs['preferences']['blocked_hosts'] == ['ads.example.org']
    settings['preferences'] = prefs['preferences']
    page.evaluate('window.applyState', settings)
    assert page.locator('body').get_attribute('data-theme') == 'dark'
    page.locator('#clear-data').click()
    expect(page.locator('#clear-confirm')).to_be_visible()
    page.locator('#cancel-clear').click()
    expect(page.locator('#clear-confirm')).to_be_hidden()
    page.locator('#clear-data').click()
    page.locator('#confirm-clear').click()
    assert page.evaluate('window.messages.at(-1).action') == 'clear_data'

    # Verify Bookmarks Manager panel
    bm_state = copy.deepcopy(snapshot)
    bm_state['tabs'].append({'id': 3, 'title': 'Dấu trang', 'url': 'papyrus://bookmarks', 'loading': False, 'blocked': 0, 'error': None})
    bm_state['active'] = 3
    page.evaluate('window.applyState', bm_state)
    expect(page.locator('#bookmarks-panel')).to_be_visible()
    expect(page.locator('#bookmarks-list .bookmark-item')).to_have_count(1)

    # Verify Downloads Manager panel
    dl_state = copy.deepcopy(snapshot)
    dl_state['tabs'].append({'id': 4, 'title': 'Tệp tải xuống', 'url': 'papyrus://downloads', 'loading': False, 'blocked': 0, 'error': None})
    dl_state['active'] = 4
    page.evaluate('window.applyState', dl_state)
    expect(page.locator('#downloads-panel')).to_be_visible()
    expect(page.locator('#downloads-panel-list .download-item')).to_have_count(1)

    # Verify Pinned Tab & Audio Muting (Milestone M2)
    m2_state = copy.deepcopy(snapshot)
    m2_state['tabs'] = [
        {'id': 10, 'title': 'Ghim Tab Web', 'url': 'https://pinned.example.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': True, 'muted': False, 'is_playing_audio': False},
        {'id': 20, 'title': 'Audio Playing Tab', 'url': 'https://music.example.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': False, 'is_playing_audio': True},
        {'id': 30, 'title': 'Audio Muted Tab', 'url': 'https://podcast.example.org', 'loading': False, 'blocked': 0, 'error': None, 'pinned': False, 'muted': True, 'is_playing_audio': False},
    ]
    m2_state['active'] = 10
    page.evaluate('window.applyState', m2_state)

    # 1. Pinned tab: .pinned class, width = 36px, no close button, no title span, favicon present, tooltip on select
    pinned_tab = page.locator('.tab.pinned[data-id="10"]')
    expect(pinned_tab).to_be_visible()
    width = pinned_tab.evaluate('(el) => Math.round(el.getBoundingClientRect().width)')
    assert width == 36, f"Expected pinned tab width 36, got {width}"
    assert pinned_tab.locator('.tab-close').count() == 0
    assert pinned_tab.locator('.tab-title').count() == 0
    assert pinned_tab.locator('.tab-fav').count() == 1
    assert pinned_tab.locator('.tab-select').get_attribute('title') == 'Ghim Tab Web'

    # 2. Middle-clicking pinned tab does not close it
    page.evaluate('window.messages = []')
    pinned_tab.click(button='middle')
    assert not any(m.get('action') == 'close_tab' and m.get('id') == 10 for m in page.evaluate('window.messages'))

    # 3. Audio indicator on playing tab
    playing_tab = page.locator('.tab[data-id="20"]')
    expect(playing_tab.locator('.tab-audio.playing')).to_be_visible()
    page.evaluate('window.messages = []')
    playing_tab.locator('.tab-audio').click()
    assert page.evaluate('window.messages.at(-1)') == {'action': 'toggle_mute_tab', 'id': 20}

    # 4. Audio indicator on muted tab
    muted_tab = page.locator('.tab[data-id="30"]')
    expect(muted_tab.locator('.tab-audio.muted')).to_be_visible()
    page.evaluate('window.messages = []')
    muted_tab.locator('.tab-audio').click()
    assert page.evaluate('window.messages.at(-1)') == {'action': 'toggle_mute_tab', 'id': 30}

    # 5. Middle-clicking normal tab triggers close_tab
    page.evaluate('window.messages = []')
    playing_tab.click(button='middle')
    assert page.evaluate('window.messages.at(-1)') == {'action': 'close_tab', 'id': 20}

    # Restore settings panel for settings screenshot & responsiveness checks
    page.evaluate('window.applyState', settings)
    page.locator('#general').scroll_into_view_if_needed()
    page.screenshot(path=str(ROOT / 'target/settings-desktop.png'))
    for width in (680, 800, 1280, 1920):
        page.set_viewport_size({'width': width, 'height': 840})
        assert page.locator('.toolbar').evaluate('(e)=>e.scrollWidth<=e.clientWidth'), f'toolbar overflow at {width}'
    page.screenshot(path=str(ROOT / 'target/settings-compact.png'))
    assert not errors, errors
    page.goto((ROOT / 'assets/newtab.html').as_uri(), wait_until='networkidle')
    document = cdp.send('DOM.getDocument')['root']['nodeId']
    for selector in ['.brand-title', '.shortcuts a', '.footer']:
        node = cdp.send('DOM.querySelector', {'nodeId': document, 'selector': selector})['nodeId']
        fonts = cdp.send('CSS.getPlatformFontsForNode', {'nodeId': node})['fonts']
        assert len({font['familyName'] for font in fonts}) == 1, (selector, fonts)
    page.screenshot(path=str(ROOT / 'target/newtab-font-fixed.png'))
    browser.close()
print('PASS: Vietnamese heading/new-tab fonts, title/bookmark injection, quoted URLs, tab commands, settings state, theme, save, clear confirmation, responsive shell; no JavaScript errors.')
