#![allow(dead_code, unused_imports)]

#[path = "../src/model.rs"]
mod model;

use model::{
    Preferences, extract_rule_domain, is_standard_rule, should_block, valid_host,
    valid_rule_or_host,
};
use std::sync::{Arc, Barrier};
use std::thread;

// =========================================================================
// 1. RULE CACHE MUTATION & INVALIDATION STRESS
// =========================================================================

#[test]
fn test_adversarial_rule_caching_invalidation_cycles() {
    let mut prefs_a = Preferences::default();
    prefs_a.blocked_hosts.push("||block-a.org^".into());
    prefs_a.allowed_hosts.push("@@||allow-a.org^".into());
    assert!(prefs_a.validate().is_ok());

    let mut prefs_b = Preferences::default();
    prefs_b.blocked_hosts.push("||block-b.org^".into());
    prefs_b.allowed_hosts.push("@@||allow-b.org^".into());
    assert!(prefs_b.validate().is_ok());

    // Rapidly toggle between prefs_a and prefs_b 500 times in sequence
    for i in 0..500 {
        if i % 2 == 0 {
            assert!(
                should_block(
                    "https://block-a.org/ad.js",
                    "https://example.com",
                    "script",
                    "GET",
                    &prefs_a
                ),
                "Cycle {i}: block-a.org MUST be blocked under prefs_a"
            );
            assert!(
                !should_block(
                    "https://block-b.org/ad.js",
                    "https://example.com",
                    "script",
                    "GET",
                    &prefs_a
                ),
                "Cycle {i}: block-b.org MUST NOT be blocked under prefs_a"
            );
        } else {
            assert!(
                should_block(
                    "https://block-b.org/ad.js",
                    "https://example.com",
                    "script",
                    "GET",
                    &prefs_b
                ),
                "Cycle {i}: block-b.org MUST be blocked under prefs_b"
            );
            assert!(
                !should_block(
                    "https://block-a.org/ad.js",
                    "https://example.com",
                    "script",
                    "GET",
                    &prefs_b
                ),
                "Cycle {i}: block-a.org MUST NOT be blocked under prefs_b"
            );
        }
    }
}

#[test]
fn test_adversarial_rule_addition_removal_and_migration() {
    let mut prefs = Preferences::default();

    // 1. Initially unblocked
    assert!(
        !should_block(
            "https://dynamic-target.io/script.js",
            "https://example.com",
            "script",
            "GET",
            &prefs
        ),
        "Initially dynamic-target.io must not be blocked"
    );

    // 2. Add block rule
    prefs.blocked_hosts.push("||dynamic-target.io^".into());
    assert!(prefs.validate().is_ok());
    assert!(
        should_block(
            "https://dynamic-target.io/script.js",
            "https://example.com",
            "script",
            "GET",
            &prefs
        ),
        "After adding block rule, dynamic-target.io must be blocked"
    );

    // 3. Remove block rule
    prefs.blocked_hosts.clear();
    assert!(prefs.validate().is_ok());
    assert!(
        !should_block(
            "https://dynamic-target.io/script.js",
            "https://example.com",
            "script",
            "GET",
            &prefs
        ),
        "After clearing blocked_hosts, dynamic-target.io must not be blocked"
    );

    // 4. Block again, then add specific exception
    prefs.blocked_hosts.push("||dynamic-target.io^".into());
    prefs
        .allowed_hosts
        .push("@@||dynamic-target.io/safe/*$script".into());
    assert!(prefs.validate().is_ok());

    assert!(
        !should_block(
            "https://dynamic-target.io/safe/app.js",
            "https://example.com",
            "script",
            "GET",
            &prefs
        ),
        "Safe path must be allowed by exception"
    );
    assert!(
        should_block(
            "https://dynamic-target.io/unsafe/leak.js",
            "https://example.com",
            "script",
            "GET",
            &prefs
        ),
        "Unsafe path must remain blocked"
    );

    // 5. Change allowed_hosts to plain domain whitelist (subresource invariant)
    prefs.allowed_hosts.clear();
    prefs.allowed_hosts.push("trusted-parent.org".into());
    assert!(prefs.validate().is_ok());

    assert!(
        !should_block(
            "https://dynamic-target.io/unsafe/leak.js",
            "https://trusted-parent.org",
            "script",
            "GET",
            &prefs
        ),
        "Subresources on trusted-parent.org must be whitelisted"
    );
    assert!(
        should_block(
            "https://dynamic-target.io/unsafe/leak.js",
            "https://untrusted.org",
            "script",
            "GET",
            &prefs
        ),
        "Subresources on untrusted.org must remain blocked"
    );
}

// =========================================================================
// 2. MULTI-THREAD CONCURRENCY HEAVY LOAD
// =========================================================================

#[test]
fn test_adversarial_multithread_concurrency_heavy_load() {
    const NUM_THREADS: usize = 16;
    const ITERATIONS_PER_THREAD: usize = 1500;

    let barrier = Arc::new(Barrier::new(NUM_THREADS));
    let mut handles = Vec::with_capacity(NUM_THREADS);

    for thread_idx in 0..NUM_THREADS {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let mut prefs = Preferences::default();
            let thread_domain = format!("thread-{thread_idx}.com");
            prefs.blocked_hosts.push(format!("||{thread_domain}^"));
            prefs.allowed_hosts.push(format!("@@||{thread_domain}/exempt/*$script"));
            assert!(prefs.validate().is_ok());

            b.wait(); // Synchronize all threads to start simultaneously

            for iter in 0..ITERATIONS_PER_THREAD {
                // Own domain tests
                let exempt_url = format!("https://{thread_domain}/exempt/test_{iter}.js");
                let blocked_url = format!("https://{thread_domain}/blocked/test_{iter}.js");
                let image_url = format!("https://{thread_domain}/exempt/test_{iter}.png");

                assert!(
                    !should_block(&exempt_url, "https://main-app.org", "script", "GET", &prefs),
                    "Thread {thread_idx} iter {iter}: exempt script must pass"
                );
                assert!(
                    should_block(&blocked_url, "https://main-app.org", "script", "GET", &prefs),
                    "Thread {thread_idx} iter {iter}: blocked path must be blocked"
                );
                assert!(
                    should_block(&image_url, "https://main-app.org", "image", "GET", &prefs),
                    "Thread {thread_idx} iter {iter}: image on exempt path must be blocked because rule is $script"
                );

                // Other domain shouldn't be blocked by own rules (unless in Brave list)
                let other_url = format!("https://clean-site-{iter}.net/api");
                let _ = should_block(&other_url, "https://main-app.org", "xmlhttprequest", "GET", &prefs);
            }
        }));
    }

    for (idx, handle) in handles.into_iter().enumerate() {
        handle
            .join()
            .unwrap_or_else(|_| panic!("Thread {idx} panicked during concurrent stress test"));
    }
}

// =========================================================================
// 3. COMPLEX ABP OPTION COMBINATIONS & MODIFIERS
// =========================================================================

#[test]
fn test_adversarial_multiple_content_types_and_inversion() {
    let mut prefs = Preferences::default();

    // Rule: block script and image, but NOT xmlhttprequest
    prefs
        .blocked_hosts
        .push("||multi-opt.org^$script,image".into());
    assert!(prefs.validate().is_ok());

    assert!(
        should_block(
            "https://multi-opt.org/app.js",
            "https://news.com",
            "script",
            "GET",
            &prefs
        ),
        "multi-opt.org script must be blocked"
    );
    assert!(
        should_block(
            "https://multi-opt.org/banner.png",
            "https://news.com",
            "image",
            "GET",
            &prefs
        ),
        "multi-opt.org image must be blocked"
    );
    assert!(
        !should_block(
            "https://multi-opt.org/data.json",
            "https://news.com",
            "xmlhttprequest",
            "GET",
            &prefs
        ),
        "multi-opt.org xmlhttprequest must NOT be blocked"
    );
    assert!(
        !should_block(
            "https://multi-opt.org/style.css",
            "https://news.com",
            "stylesheet",
            "GET",
            &prefs
        ),
        "multi-opt.org stylesheet must NOT be blocked"
    );
}

#[test]
fn test_adversarial_domain_restriction_modifier_matrix() {
    let mut prefs = Preferences::default();

    // Block only when embedded on target-a.com or target-b.com, but NOT on target-c.com
    prefs
        .blocked_hosts
        .push("||scoped-tracker.com^$domain=target-a.com|target-b.com".into());
    assert!(prefs.validate().is_ok());

    assert!(
        should_block(
            "https://scoped-tracker.com/track.js",
            "https://target-a.com/page",
            "script",
            "GET",
            &prefs
        ),
        "Must be blocked when top_url is target-a.com"
    );
    assert!(
        should_block(
            "https://scoped-tracker.com/track.js",
            "https://target-b.com/page",
            "script",
            "GET",
            &prefs
        ),
        "Must be blocked when top_url is target-b.com"
    );
    assert!(
        !should_block(
            "https://scoped-tracker.com/track.js",
            "https://target-c.com/page",
            "script",
            "GET",
            &prefs
        ),
        "Must NOT be blocked when top_url is target-c.com"
    );
    assert!(
        !should_block(
            "https://scoped-tracker.com/track.js",
            "https://other.org/page",
            "script",
            "GET",
            &prefs
        ),
        "Must NOT be blocked on arbitrary other domains"
    );
}

#[test]
fn test_adversarial_domain_inversion_modifier() {
    let mut prefs = Preferences::default();

    // Block everywhere EXCEPT on safe-hub.org
    prefs
        .blocked_hosts
        .push("||wide-block.com^$domain=~safe-hub.org".into());
    assert!(prefs.validate().is_ok());

    assert!(
        !should_block(
            "https://wide-block.com/lib.js",
            "https://safe-hub.org/page",
            "script",
            "GET",
            &prefs
        ),
        "Must NOT be blocked when top_url is inverted domain safe-hub.org"
    );
    assert!(
        should_block(
            "https://wide-block.com/lib.js",
            "https://any-other.org/page",
            "script",
            "GET",
            &prefs
        ),
        "MUST be blocked on any other site"
    );
}

#[test]
fn test_adversarial_third_party_flags_evaluation() {
    let mut prefs = Preferences::default();

    // Block only when third-party
    prefs
        .blocked_hosts
        .push("||tp-tracker.com^$third-party".into());
    assert!(prefs.validate().is_ok());

    // When third-party (top url is example.org) -> blocked
    assert!(
        should_block(
            "https://tp-tracker.com/beacon.js",
            "https://example.org",
            "script",
            "GET",
            &prefs
        ),
        "Third-party request to tp-tracker.com must be blocked"
    );

    // When first-party (top url is tp-tracker.com) -> NOT blocked
    assert!(
        !should_block(
            "https://tp-tracker.com/beacon.js",
            "https://tp-tracker.com/dashboard",
            "script",
            "GET",
            &prefs
        ),
        "First-party request to tp-tracker.com must NOT be blocked"
    );
}

// =========================================================================
// 4. MULTI-TIER EXCEPTION PRIORITY HIERARCHY
// =========================================================================

#[test]
fn test_adversarial_multi_tier_exception_priority_matrix() {
    let mut prefs = Preferences::default();

    // Domain block
    prefs.blocked_hosts.push("||matrix-cdn.com^".into());
    // Specific path + type + method exceptions
    prefs
        .allowed_hosts
        .push("@@||matrix-cdn.com/api/v1/*$xmlhttprequest,method=POST".into());
    prefs
        .allowed_hosts
        .push("@@||matrix-cdn.com/assets/img/*$image".into());
    assert!(prefs.validate().is_ok());

    // Case 1: POST to /api/v1/ -> ALLOWED
    assert!(
        !should_block(
            "https://matrix-cdn.com/api/v1/update",
            "https://news.org",
            "xmlhttprequest",
            "POST",
            &prefs
        ),
        "POST /api/v1/ must be exempt"
    );

    // Case 2: GET to /api/v1/ -> BLOCKED (method mismatch)
    assert!(
        should_block(
            "https://matrix-cdn.com/api/v1/update",
            "https://news.org",
            "xmlhttprequest",
            "GET",
            &prefs
        ),
        "GET /api/v1/ must remain blocked due to method mismatch"
    );

    // Case 3: POST to /api/v2/ -> BLOCKED (path mismatch)
    assert!(
        should_block(
            "https://matrix-cdn.com/api/v2/update",
            "https://news.org",
            "xmlhttprequest",
            "POST",
            &prefs
        ),
        "POST /api/v2/ must remain blocked due to path mismatch"
    );

    // Case 4: image from /assets/img/ -> ALLOWED
    assert!(
        !should_block(
            "https://matrix-cdn.com/assets/img/logo.png",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "image from /assets/img/ must be exempt"
    );

    // Case 5: script from /assets/img/ -> BLOCKED (type mismatch)
    assert!(
        should_block(
            "https://matrix-cdn.com/assets/img/exploit.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "script from /assets/img/ must remain blocked due to type mismatch"
    );

    // Case 6: image from /assets/banners/ -> BLOCKED (path mismatch)
    assert!(
        should_block(
            "https://matrix-cdn.com/assets/banners/ad.png",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "image from /assets/banners/ must remain blocked due to path mismatch"
    );
}

// =========================================================================
// 5. MALFORMED URLS, SPECIAL SCHEMES, AND BOUNDARY CONDITIONS
// =========================================================================

#[test]
fn test_adversarial_malformed_urls_and_special_schemes() {
    let prefs = Preferences::default();

    let malformed_inputs = [
        "",
        "not_a_url",
        "://missing-scheme",
        "http://",
        "https://",
        "about:blank",
        "data:text/html,Hello%20World",
        "blob:https://example.com/d94982bb-236b-4e08-9df5-63fef570bc84",
        "javascript:alert(1)",
        "file:///C:/Users/test.txt",
        "chrome://settings",
        "papyrus://bookmarks",
    ];

    for raw in malformed_inputs {
        // Must never panic on any malformed or non-http input
        let result = should_block(raw, "https://example.com", "other", "GET", &prefs);
        assert!(
            !result,
            "Non-http or invalid URL '{raw}' should safely return false"
        );
    }
}

#[test]
fn test_adversarial_case_insensitivity_and_normalization() {
    let mut prefs = Preferences::default();
    prefs.blocked_hosts.push("||case-test.com^$script".into());
    assert!(prefs.validate().is_ok());

    // Standard lowercase domain matching
    assert!(
        should_block(
            "https://case-test.com/tracker.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Domain must be blocked"
    );

    // Method case variations: Chromium / HTTP methods
    assert!(
        should_block(
            "https://case-test.com/tracker.js",
            "https://news.org",
            "script",
            "get",
            &prefs
        ),
        "Lowercase method 'get' must still evaluate"
    );
    assert!(
        should_block(
            "https://case-test.com/tracker.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Uppercase method 'GET' must evaluate"
    );
}

#[test]
fn test_adversarial_youtube_and_googlevideo_heuristics_integrity() {
    let prefs = Preferences::default();

    // YouTube ad endpoints must be blocked
    assert!(should_block(
        "https://www.youtube.com/pagead/lvz?ai=xyz",
        "https://www.youtube.com",
        "xmlhttprequest",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://www.youtube.com/api/stats/ads?v=xyz",
        "https://www.youtube.com",
        "xmlhttprequest",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://www.youtube.com/ptracking?v=xyz",
        "https://www.youtube.com",
        "xmlhttprequest",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://www.youtube.com/youtubei/v1/att/get",
        "https://www.youtube.com",
        "xmlhttprequest",
        "POST",
        &prefs
    ));
    assert!(should_block(
        "https://www.youtube.com/api/stats/qoe?adformat=1_2",
        "https://www.youtube.com",
        "xmlhttprequest",
        "GET",
        &prefs
    ));

    // YouTube legitimate endpoints must NOT be blocked
    assert!(!should_block(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://www.youtube.com",
        "main_frame",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://www.youtube.com/s/desktop/xyz/base.js",
        "https://www.youtube.com",
        "script",
        "GET",
        &prefs
    ));

    // GoogleVideo ad query params must be blocked
    assert!(should_block(
        "https://rr1---sn-abc.googlevideo.com/videoplayback?adformat=1&id=123",
        "https://www.youtube.com",
        "media",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://rr1---sn-abc.googlevideo.com/videoplayback?ctier=L&id=123",
        "https://www.youtube.com",
        "media",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://rr1---sn-abc.googlevideo.com/videoplayback?el=adunit&id=123",
        "https://www.youtube.com",
        "media",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://rr1---sn-abc.googlevideo.com/videoplayback?ad_type=2&id=123",
        "https://www.youtube.com",
        "media",
        "GET",
        &prefs
    ));

    // GoogleVideo legitimate video stream must NOT be blocked
    assert!(!should_block(
        "https://rr1---sn-abc.googlevideo.com/videoplayback?expire=123&sparams=expire&id=123",
        "https://www.youtube.com",
        "media",
        "GET",
        &prefs
    ));
}

// =========================================================================
// 6. DYNAMIC ADBLOCK TOGGLE INVARIANT
// =========================================================================

#[test]
fn test_adversarial_adblock_disabled_toggle_bypasses_all_filters() {
    let mut prefs = Preferences {
        adblock: false, // Ad blocking globally disabled
        ..Preferences::default()
    };
    prefs.blocked_hosts.push("||evil-tracker.com^".into());
    prefs.blocked_hosts.push("coinhive.com".into());

    let test_requests = [
        ("https://evil-tracker.com/ad.js", "script", "GET"),
        ("https://doubleclick.net/pagead/ads.js", "script", "GET"),
        ("https://coinhive.com/miner.js", "script", "GET"),
        (
            "https://www.youtube.com/pagead/test",
            "xmlhttprequest",
            "GET",
        ),
        (
            "https://rr1---sn-abc.googlevideo.com/videoplayback?adformat=1",
            "media",
            "GET",
        ),
    ];

    for (url, res_type, method) in test_requests {
        assert!(
            !should_block(url, "https://example.com", res_type, method, &prefs),
            "When adblock=false, {url} MUST NOT be blocked"
        );
    }
}

// =========================================================================
// 7. PORTS, QUERY PARAMETERS, AND FRAGMENTS
// =========================================================================

#[test]
fn test_adversarial_ports_queries_and_fragments() {
    let mut prefs = Preferences::default();
    prefs.blocked_hosts.push("||ad-server.net^$image".into());
    assert!(prefs.validate().is_ok());

    // Port numbers
    assert!(
        should_block(
            "http://ad-server.net:8080/ad.gif",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Port 8080 request must be blocked"
    );
    assert!(
        should_block(
            "https://ad-server.net:8443/ad.png?query=1&track=yes#banner",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Port 8443 with query and fragment must be blocked"
    );
    assert!(
        !should_block(
            "http://ad-server.net:8080/app.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Script request to ad-server.net must NOT be blocked (rule was $image)"
    );
}

// =========================================================================
// 8. SUBDOMAIN MATCHING AND BOUNDARIES
// =========================================================================

#[test]
fn test_adversarial_domain_boundaries_and_suffixes() {
    let mut prefs = Preferences::default();
    // Plain domain in allowed_hosts
    prefs.allowed_hosts.push("legit-corp.com".into());
    // Plain domain in blocked_hosts
    prefs.blocked_hosts.push("malicious-net.com".into());
    assert!(prefs.validate().is_ok());

    // Exact matches
    assert!(!should_block(
        "https://legit-corp.com/script.js",
        "https://news.org",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://malicious-net.com/track.js",
        "https://news.org",
        "script",
        "GET",
        &prefs
    ));

    // Subdomains
    assert!(!should_block(
        "https://sub.legit-corp.com/script.js",
        "https://news.org",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://ads.sub.malicious-net.com/track.js",
        "https://news.org",
        "script",
        "GET",
        &prefs
    ));

    // Sibling domains (must NOT match)
    assert!(
        !should_block(
            "https://notlegit-corp.com/script.js",
            "https://notlegit-corp.com",
            "script",
            "GET",
            &prefs
        ),
        "Sibling notlegit-corp.com must not be confused with legit-corp.com"
    );
    assert!(
        !should_block(
            "https://notmalicious-net.com/track.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Sibling notmalicious-net.com must not be blocked"
    );
}

// =========================================================================
// 9. HIGH VOLUME FUZZING & DETERMINISM HARNESS
// =========================================================================

#[test]
fn test_adversarial_high_volume_fuzzing_10000_evaluations() {
    let mut prefs = Preferences::default();
    prefs.blocked_hosts.push("||fuzz-ad.org^$script".into());
    prefs
        .blocked_hosts
        .push("||fuzz-tracker.io^$image,xmlhttprequest".into());
    prefs
        .allowed_hosts
        .push("@@||fuzz-ad.org/safe/*$script".into());
    assert!(prefs.validate().is_ok());

    let methods = ["GET", "POST", "PUT", "DELETE", "HEAD", "OPTIONS"];
    let types = [
        "script",
        "image",
        "xmlhttprequest",
        "stylesheet",
        "font",
        "media",
        "other",
    ];

    for i in 0..10_000 {
        let method = methods[i % methods.len()];
        let res_type = types[i % types.len()];

        let is_safe = (i % 3) == 0;
        let path = if is_safe {
            format!("/safe/item_{i}.js")
        } else {
            format!("/leak/item_{i}.js")
        };
        let url = format!("https://fuzz-ad.org{path}");

        let blocked = should_block(&url, "https://top-portal.org", res_type, method, &prefs);

        if res_type == "script" {
            if is_safe {
                assert!(!blocked, "Fuzz iter {i}: safe script path MUST be exempt");
            } else {
                assert!(blocked, "Fuzz iter {i}: unsafe script path MUST be blocked");
            }
        } else {
            // Rule was only $script, so non-script on fuzz-ad.org is not blocked by fuzz rule
            // (unless covered by bundled Brave list, which fuzz-ad.org is not)
            assert!(
                !blocked,
                "Fuzz iter {i}: non-script on fuzz-ad.org must not be blocked"
            );
        }
    }
}
