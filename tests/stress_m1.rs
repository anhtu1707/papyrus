#![allow(dead_code, unused_imports)]

#[path = "../src/model.rs"]
mod model;

use model::{
    BOOKMARKS, Command, DOWNLOADS, EXTENSIONS, HISTORY, NEW_TAB, PASSWORDS, SETTINGS,
    allowed_navigation, parse_command, resolve_input,
};

// ============================================================================
// SUITE 1: parse_command Origin Validation Stress Tests
// ============================================================================
#[test]
fn test_parse_command_origin_strictness() {
    let valid_payload = r#"{"action":"new_tab"}"#;

    // Positive case: exact "about:blank"
    assert!(parse_command("about:blank", valid_payload).is_some());

    // Adversarial origins: variations, prefixes, suffixes, protocols
    let rejected_origins = [
        "",
        "about:blank ",
        " about:blank",
        "ABOUT:BLANK",
        "About:Blank",
        "about:blank\0",
        "about:blank#hash",
        "about:blank?query=1",
        "about:blank/path",
        "about:",
        "about:newtab",
        "chrome://newtab",
        "papyrus://settings",
        "http://about:blank",
        "https://about:blank",
        "null",
        "undefined",
        "file:///",
        "http://localhost",
        "https://evil.com",
        "javascript:void(0)",
        "blob:about:blank",
        "data:text/html,about:blank",
        "//about:blank",
    ];

    for origin in rejected_origins {
        assert!(
            parse_command(origin, valid_payload).is_none(),
            "Expected origin '{origin}' to be strictly rejected!"
        );
    }
}

// ============================================================================
// SUITE 2: parse_command Payload Size Boundary & Oversized Fuzzing
// ============================================================================
#[test]
fn test_parse_command_payload_size_boundaries() {
    // 1. Empty body
    assert!(parse_command("about:blank", "").is_none());

    // 2. Exactly at boundary (256_000 bytes)
    let base_prefix = r#"{"action":"navigate","input":""#;
    let base_suffix = r#""}"#;
    let base_overhead = base_prefix.len() + base_suffix.len();

    let target_pad_len = 256_000 - base_overhead;
    let mut valid_256k = String::with_capacity(256_000);
    valid_256k.push_str(base_prefix);
    for _ in 0..target_pad_len {
        valid_256k.push('a');
    }
    valid_256k.push_str(base_suffix);
    assert_eq!(valid_256k.len(), 256_000);

    // 256,000 bytes exactly should be parsed (not rejected by length check)
    let parsed_256k = parse_command("about:blank", &valid_256k);
    assert!(
        parsed_256k.is_some(),
        "Payload exactly at 256_000 bytes should be processed"
    );

    // 3. Exactly 1 byte over limit: 256_001 bytes
    let mut invalid_256001 = valid_256k.clone();
    invalid_256001.insert(base_prefix.len(), 'x');
    assert_eq!(invalid_256001.len(), 256_001);
    assert!(
        parse_command("about:blank", &invalid_256001).is_none(),
        "Payload at 256_001 bytes MUST be rejected"
    );

    // 4. Large payloads: 300KB, 500KB, 1MB, 2MB
    for size in [300_000, 500_000, 1_048_576, 2_097_152] {
        let oversized = " ".repeat(size);
        assert!(parse_command("about:blank", &oversized).is_none());

        let oversized_json = format!(r#"{{"action":"new_tab","padding":"{}"}}"#, "x".repeat(size));
        assert!(parse_command("about:blank", &oversized_json).is_none());
    }
}

// ============================================================================
// SUITE 3: parse_command Malformed JSON & Adversarial Payloads
// ============================================================================
#[test]
fn test_parse_command_malformed_json_and_fuzz() {
    let malformed_cases = [
        "",
        " ",
        "\t\r\n",
        "null",
        "true",
        "false",
        "12345",
        "-999.99",
        "\"a string\"",
        "[]",
        r#"["new_tab"]"#,
        r#"{"action"}"#,
        r#"{"action":}"#,
        r#"{"action":"#,
        r#"{"action": "new_tab""#,
        r#"{"action": 'new_tab'}"#,
        r#"{"action": "new_tab",}"#,
        r#"{action: "new_tab"}"#,
        r#"<action>new_tab</action>"#,
        r#"undefined"#,
        r#"NaN"#,
        r#"Infinity"#,
        r#"{"action": null}"#,
        r#"{"action": 123}"#,
        r#"{"action": true}"#,
        r#"{"action": []}"#,
        r#"{"action": {}}"#,
        r#"{"action": "new_tab\u0000"}"#,
        r#"{"action": "new_tab", "extra": "\uD800"}"#, // lone surrogate
        r#"{"action": "new_tab", "__proto__": {"polluted": true}}"#,
        r#"{"action": "new_tab", "constructor": {}}"#,
    ];

    for case in malformed_cases {
        assert!(
            parse_command("about:blank", case).is_none(),
            "Expected malformed case to return None: {case}"
        );
    }
}

// ============================================================================
// SUITE 4: parse_command Exhaustive 55 Actions Whitelist Matrix & Rejections
// ============================================================================
#[test]
fn test_parse_command_all_55_actions_and_extra_fields() {
    let actions_matrix: Vec<(&str, &str)> = vec![
        ("ready", r#"{"action":"ready"}"#),
        ("focus_address", r#"{"action":"focus_address"}"#),
        (
            "navigate",
            r#"{"action":"navigate","input":"https://example.com"}"#,
        ),
        ("new_tab", r#"{"action":"new_tab"}"#),
        ("new_window", r#"{"action":"new_window"}"#),
        ("switch_tab", r#"{"action":"switch_tab","id":42}"#),
        ("close_tab", r#"{"action":"close_tab","id":42}"#),
        ("back", r#"{"action":"back"}"#),
        ("forward", r#"{"action":"forward"}"#),
        ("reload", r#"{"action":"reload"}"#),
        ("home", r#"{"action":"home"}"#),
        ("close", r#"{"action":"close"}"#),
        ("minimize", r#"{"action":"minimize"}"#),
        ("maximize", r#"{"action":"maximize"}"#),
        ("snap", r#"{"action":"snap","layout":"left"}"#),
        ("drag", r#"{"action":"drag"}"#),
        ("settings", r#"{"action":"settings"}"#),
        ("history", r#"{"action":"history"}"#),
        ("bookmarks", r#"{"action":"bookmarks"}"#),
        ("downloads", r#"{"action":"downloads"}"#),
        ("passwords", r#"{"action":"passwords"}"#),
        ("extensions", r#"{"action":"extensions"}"#),
        (
            "save_password",
            r#"{"action":"save_password","url":"https://example.com","username":"alice","password":"secret123"}"#,
        ),
        ("delete_password", r#"{"action":"delete_password","id":10}"#),
        ("clear_passwords", r#"{"action":"clear_passwords"}"#),
        (
            "open_downloaded_file",
            r#"{"action":"open_downloaded_file","path":"C:\\Downloads\\test.pdf"}"#,
        ),
        (
            "show_download_in_folder",
            r#"{"action":"show_download_in_folder","path":"C:\\Downloads\\test.pdf"}"#,
        ),
        ("clear_downloads", r#"{"action":"clear_downloads"}"#),
        ("reopen_tab", r#"{"action":"reopen_tab"}"#),
        ("restore_tab", r#"{"action":"restore_tab"}"#),
        ("pin_tab", r#"{"action":"pin_tab","id":1}"#),
        ("toggle_mute_tab", r#"{"action":"toggle_mute_tab","id":1}"#),
        ("duplicate_tab", r#"{"action":"duplicate_tab","id":1}"#),
        ("zoom_in", r#"{"action":"zoom_in"}"#),
        ("zoom_out", r#"{"action":"zoom_out"}"#),
        ("zoom_reset", r#"{"action":"zoom_reset"}"#),
        ("print", r#"{"action":"print"}"#),
        ("next_tab", r#"{"action":"next_tab"}"#),
        ("prev_tab", r#"{"action":"prev_tab"}"#),
        (
            "select_tab_by_index",
            r#"{"action":"select_tab_by_index","index":3}"#,
        ),
        ("select_last_tab", r#"{"action":"select_last_tab"}"#),
        ("clear_history", r#"{"action":"clear_history"}"#),
        (
            "delete_history_entry",
            r#"{"action":"delete_history_entry","index":2}"#,
        ),
        (
            "save_preferences",
            r#"{"action":"save_preferences","preferences":{"engine":"brave","zoom":100,"theme":"parchment","startup":"newtab","show_bookmarks":true,"adblock":true,"block_popups":true,"allowed_hosts":[],"blocked_hosts":[],"download_dir":""}}"#,
        ),
        ("reset_preferences", r#"{"action":"reset_preferences"}"#),
        ("bookmark", r#"{"action":"bookmark"}"#),
        (
            "add_bookmark",
            r#"{"action":"add_bookmark","title":"Example","url":"https://example.com"}"#,
        ),
        (
            "delete_bookmark",
            r#"{"action":"delete_bookmark","index":0}"#,
        ),
        ("clear_bookmarks", r#"{"action":"clear_bookmarks"}"#),
        ("clear_data", r#"{"action":"clear_data"}"#),
        ("cancel_clear", r#"{"action":"cancel_clear"}"#),
        ("dev_tools", r#"{"action":"dev_tools"}"#),
        (
            "header_height",
            r#"{"action":"header_height","height":115}"#,
        ),
        ("show_menu", r#"{"action":"show_menu","x":100,"y":200}"#),
        (
            "show_snap_menu",
            r#"{"action":"show_snap_menu","x":50,"y":60}"#,
        ),
        (
            "show_tab_menu",
            r#"{"action":"show_tab_menu","id":1,"x":100,"y":30}"#,
        ),
        ("set_default_browser", r#"{"action":"set_default_browser"}"#),
        ("toggle_fullscreen", r#"{"action":"toggle_fullscreen"}"#),
        ("reorder_tab", r#"{"action":"reorder_tab","from":1,"to":2}"#),
    ];

    assert_eq!(
        actions_matrix.len(),
        59,
        "Matrix must test all 59 recognized actions in Command enum"
    );

    for (name, valid_json) in &actions_matrix {
        // 1. Positive verification: valid JSON must be accepted
        let cmd = parse_command("about:blank", valid_json);
        assert!(
            cmd.is_some(),
            "Action '{name}' with payload '{valid_json}' must be accepted!"
        );

        // 2. Extra field injection verification: inserting "injected_field": 123 must be rejected
        let mut v: serde_json::Value = serde_json::from_str(valid_json).unwrap();
        v.as_object_mut()
            .unwrap()
            .insert("injected_field".to_string(), serde_json::json!(123));
        let injected_json = serde_json::to_string(&v).unwrap();
        assert!(
            parse_command("about:blank", &injected_json).is_none(),
            "Action '{name}' with unexpected extra field MUST be rejected! Payload: {injected_json}"
        );

        // 3. Unauthorized origin test for this action
        assert!(
            parse_command("https://attacker.com", valid_json).is_none(),
            "Action '{name}' from external origin MUST be rejected"
        );
    }

    // 4. Missing required field tests for parameterized actions
    let missing_field_cases = [
        r#"{"action":"navigate"}"#,
        r#"{"action":"switch_tab"}"#,
        r#"{"action":"close_tab"}"#,
        r#"{"action":"snap"}"#,
        r#"{"action":"save_password","url":"u","username":"alice"}"#,
        r#"{"action":"save_password","url":"u","password":"p"}"#,
        r#"{"action":"delete_password"}"#,
        r#"{"action":"open_downloaded_file"}"#,
        r#"{"action":"show_download_in_folder"}"#,
        r#"{"action":"select_tab_by_index"}"#,
        r#"{"action":"delete_history_entry"}"#,
        r#"{"action":"save_preferences"}"#,
        r#"{"action":"add_bookmark","title":"t"}"#,
        r#"{"action":"delete_bookmark"}"#,
        r#"{"action":"header_height"}"#,
        r#"{"action":"show_menu","x":10}"#,
        r#"{"action":"show_snap_menu","y":20}"#,
        r#"{"action":"show_tab_menu","id":1}"#,
        r#"{"action":"reorder_tab","from":1}"#,
    ];
    for case in missing_field_cases {
        assert!(
            parse_command("about:blank", case).is_none(),
            "Missing field case MUST be rejected: {case}"
        );
    }

    // 5. Unauthorized action names fuzzing
    let unauthorized_actions = [
        "system",
        "exec",
        "eval",
        "cmd",
        "shell",
        "run",
        "execute",
        "delete_file",
        "format_disk",
        "open_url",
        "open_process",
        "drop_database",
        "__proto__",
        "constructor",
        "toString",
        "valueOf",
        "new_tab ",
        " new_tab",
        "NEW_TAB",
        "NewTab",
        "New_Tab",
        "new-tab",
        "save_password ",
        "SAVE_PASSWORD",
        "Ready",
        "READY",
    ];
    for action in unauthorized_actions {
        let payload = format!(r#"{{"action":"{action}"}}"#);
        assert!(
            parse_command("about:blank", &payload).is_none(),
            "Unauthorized action '{action}' MUST be rejected!"
        );
    }
}

// ============================================================================
// SUITE 5: resolve_input Scheme Variations & Normalization Matrix
// ============================================================================
#[test]
fn test_resolve_input_exhaustive_scheme_matrix() {
    let pages = [
        ("settings", SETTINGS),
        ("history", HISTORY),
        ("bookmarks", BOOKMARKS),
        ("downloads", DOWNLOADS),
        ("passwords", PASSWORDS),
        ("extensions", EXTENSIONS),
    ];

    let prefixes = [
        "",            // bare
        "papyrus://",  // papyrus standard
        "papyrus:",    // papyrus single colon
        "papyrus:/",   // papyrus single slash
        "papyrus:///", // papyrus triple slash
        "chrome://",   // chrome standard
        "chrome:",     // chrome single colon
        "chrome:/",    // chrome single slash
        "chrome:///",  // chrome triple slash
        "about://",    // about standard
        "about:",      // about single colon
        "about:/",     // about single slash
        "about:///",   // about triple slash
    ];

    let mut count = 0;
    for (page, canonical) in pages {
        for prefix in prefixes {
            for trailing in ["", "/", "//", "///"] {
                for ws in [("", ""), ("  ", "  "), ("\t", "\n"), ("\r\n  ", "  \r\n")] {
                    for quote in ["", "\"", "'"] {
                        let input = format!(
                            "{ws_0}{quote}{prefix}{page}{trailing}{quote}{ws_1}",
                            ws_0 = ws.0,
                            quote = quote,
                            prefix = prefix,
                            page = page,
                            trailing = trailing,
                            ws_1 = ws.1
                        );
                        let resolved = resolve_input(&input, "google").unwrap();
                        assert_eq!(
                            resolved, canonical,
                            "Failed to resolve variation '{input}' -> expected '{canonical}', got '{resolved}'"
                        );

                        // Also test UPPERCASE version of prefix + page
                        let upper_raw = format!("{prefix}{page}{trailing}").to_uppercase();
                        let upper_input = format!(
                            "{ws_0}{quote}{upper_raw}{quote}{ws_1}",
                            ws_0 = ws.0,
                            quote = quote,
                            upper_raw = upper_raw,
                            ws_1 = ws.1
                        );
                        let resolved_upper = resolve_input(&upper_input, "google").unwrap();
                        assert_eq!(
                            resolved_upper, canonical,
                            "Failed to resolve UPPERCASE variation '{upper_input}' -> expected '{canonical}', got '{resolved_upper}'"
                        );

                        count += 2;
                    }
                }
            }
        }
    }
    assert!(
        count > 1000,
        "Tested over {count} variations of internal page schemes"
    );
}

// ============================================================================
// SUITE 6: resolve_input NewTab and About:Blank Variations
// ============================================================================
#[test]
fn test_resolve_input_newtab_and_about_blank() {
    // Newtab aliases: newtab, new-tab
    for term in ["newtab", "new-tab"] {
        for prefix in [
            "",
            "papyrus://",
            "papyrus:",
            "chrome://",
            "chrome:",
            "about://",
            "about:",
        ] {
            for trailing in ["", "/"] {
                let input = format!("{prefix}{term}{trailing}");
                assert_eq!(
                    resolve_input(&input, "google").unwrap(),
                    NEW_TAB,
                    "Failed for newtab variation '{input}'"
                );
                let upper = input.to_uppercase();
                assert_eq!(
                    resolve_input(&upper, "google").unwrap(),
                    NEW_TAB,
                    "Failed for uppercase newtab variation '{upper}'"
                );
            }
        }
    }

    // Empty and whitespace input defaults to NEW_TAB
    for empty in ["", "   ", "\t\t", "\r\n", " \"\" ", " '' "] {
        assert_eq!(resolve_input(empty, "google").unwrap(), NEW_TAB);
    }

    // about:blank aliases
    for term in [
        "about:blank",
        "ABOUT:BLANK",
        "about:blank/",
        "about://blank",
        "about:/blank",
        "about:///blank",
    ] {
        assert_eq!(
            resolve_input(term, "google").unwrap(),
            "about:blank",
            "Failed for about:blank variation '{term}'"
        );
    }

    // chrome://blank or papyrus://blank must NOT resolve to about:blank
    let chrome_blank = resolve_input("chrome://blank", "google");
    assert!(chrome_blank.is_err() || !chrome_blank.unwrap().contains("about:blank"));
}

// ============================================================================
// SUITE 7: resolve_input Windows Paths, File URLs, and Boundaries
// ============================================================================
#[test]
fn test_resolve_input_windows_paths_and_boundaries() {
    // 1. Direct file URI
    assert_eq!(
        resolve_input("file:///C:/Users/test/index.html", "google").unwrap(),
        "file:///C:/Users/test/index.html"
    );
    assert_eq!(
        resolve_input("file:///D:/doc.pdf", "google").unwrap(),
        "file:///D:/doc.pdf"
    );

    // 2. Windows Drive letter paths
    let win_cases = [
        (
            r"C:\Windows\System32\drivers\etc\hosts",
            "file:///C:/Windows/System32/drivers/etc/hosts",
        ),
        (
            r"D:\projects\papyrus\index.html",
            "file:///D:/projects/papyrus/index.html",
        ),
        ("C:/Users/name/file.txt", "file:///C:/Users/name/file.txt"),
    ];
    for (win_path, expected_file_url) in win_cases {
        let res = resolve_input(win_path, "google").unwrap();
        assert_eq!(res, expected_file_url);
    }

    // 3. Short / Boundary drive letters (must not panic or out-of-bounds)
    let boundary_strings = [
        "C", "C:", "c:", "C::", "C::/", "1:/file", ":::", "///", "\\\\", "\\", "/", "CON", "PRN",
        "AUX", "NUL", "COM1", "LPT1",
    ];
    for b in boundary_strings {
        // Must never panic
        let _ = resolve_input(b, "google");
    }
}

// ============================================================================
// SUITE 8: resolve_input Security Rejection & Dangerous Schemes
// ============================================================================
#[test]
fn test_resolve_input_security_rejections() {
    let explicit_rejected_schemes = [
        "javascript:alert(1)",
        "javascript:void(0)",
        "javascript://test%0Aalert(1)",
        "data:text/html,<b>pwn</b>",
        "blob:https://example.com/uuid",
        "about:cache",
        "about:config",
        "about:foobar",
        "chrome://flags",
        "chrome://gpu",
        "papyrus://admin",
        "https://user:password@example.com",
        "http://admin:secret@127.0.0.1",
    ];

    for dangerous in explicit_rejected_schemes {
        let res = resolve_input(dangerous, "google");
        assert!(
            res.is_err(),
            "Dangerous navigation '{dangerous}' must be rejected with Err! Got: {res:?}"
        );
    }

    // Safe fallbacks to search engine for non-URL strings
    let search_fallbacks = [
        "vbscript:msgbox(1)",
        "cmd /c calc.exe",
        "select * from users;",
        "<script>alert(1)</script>",
    ];
    for s in search_fallbacks {
        let res = resolve_input(s, "google").unwrap();
        assert!(
            res.starts_with("https://www.google.com/search?q="),
            "Search query fallback failed for '{s}': got '{res}'"
        );
    }
}

// ============================================================================
// SUITE 9: resolve_input Unicode, Search Fallbacks & Search Engines
// ============================================================================
#[test]
fn test_resolve_input_searches_and_unicode_fallbacks() {
    // 1. Google search fallback
    let res_google = resolve_input("rust language 2026", "google").unwrap();
    assert!(res_google.starts_with("https://www.google.com/search?q="));
    assert!(
        res_google.contains("rust+language+2026") || res_google.contains("rust%20language%202026")
    );

    // 2. Brave search fallback
    let res_brave = resolve_input("privacy browser", "brave").unwrap();
    assert!(res_brave.starts_with("https://search.brave.com/search?q="));

    // 3. DuckDuckGo search fallback
    let res_ddg = resolve_input("duckduckgo search", "duckduckgo").unwrap();
    assert!(res_ddg.starts_with("https://duckduckgo.com/?q="));

    // 4. Default search fallback for unknown engine
    let res_def = resolve_input("test query", "unknown_engine").unwrap();
    assert!(res_def.starts_with("https://www.google.com/"));

    // 5. Vietnamese search terms with diacritics
    let vn_queries = [
        "trình duyệt Papyrus",
        "báo Tuổi Trẻ hôm nay",
        "thời tiết Hà Nội",
        "công nghệ thông tin Việt Nam",
        "đại học Bách Khoa",
    ];
    for vn in vn_queries {
        let res = resolve_input(vn, "google").unwrap();
        assert!(res.starts_with("https://www.google.com/search?q="));
    }

    // 6. International Unicode scripts (CJK, Arabic, Cyrillic, Greek, Hebrew, Emojis)
    let unicode_queries = [
        "你好世界",
        "こんにちは世界",
        "مرحبا بالعالم",
        "Привет мир",
        "Γειά σου Κόσμε",
        "שלום עולם",
        "🦀 🚀 💻 🔍",
        "Zażółć gęślą jaźń",
    ];
    for u in unicode_queries {
        let res = resolve_input(u, "brave").unwrap();
        assert!(res.starts_with("https://search.brave.com/search?q="));
    }
}

// ============================================================================
// SUITE 10: resolve_input & parse_command High-Throughput Fuzzing (10,000 runs)
// ============================================================================
#[test]
fn test_resolve_input_and_parse_command_fuzz_harness() {
    let mut state: u64 = 0xdeadbeefcafebabe;
    let mut lcg = || -> u64 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        state
    };

    let sample_chars = [
        'a', 'Z', '0', ':', '/', '.', '?', '&', '=', '%', '-', '_', '#', ' ', '\t', '\n', '"',
        '\'', '\\', '\0', 'é', 'à', 'ữ', '日', '한', '🚀', '<', '>', '{', '}', '[', ']',
    ];

    // Fuzz 5,000 arbitrary strings through resolve_input
    for _ in 0..5_000 {
        let len = (lcg() % 64) as usize;
        let mut s = String::with_capacity(len);
        for _ in 0..len {
            let idx = (lcg() as usize) % sample_chars.len();
            s.push(sample_chars[idx]);
        }

        // Neither panics nor infinite loops are allowed
        let _ = resolve_input(&s, "google");
    }

    // Fuzz 5,000 arbitrary payloads through parse_command
    for _ in 0..5_000 {
        let len = (lcg() % 128) as usize;
        let mut s = String::with_capacity(len);
        for _ in 0..len {
            let idx = (lcg() as usize) % sample_chars.len();
            s.push(sample_chars[idx]);
        }

        // Neither panics nor unauthorized command parsing are allowed
        let _ = parse_command("about:blank", &s);
    }
}
