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
// 1. RULE SYNTAX ADVERSARIAL TESTING
// =========================================================================

#[test]
fn test_adversarial_domain_anchors() {
    let mut prefs = Preferences::default();

    // 1.1 ||ads.example.com^
    prefs.blocked_hosts.push("||ads.example.com^".into());
    assert!(prefs.validate().is_ok());

    assert!(
        should_block(
            "http://ads.example.com/banner.png",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Domain anchor ||ads.example.com^ must block http subresource"
    );
    assert!(
        should_block(
            "https://sub.ads.example.com/track",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Domain anchor ||ads.example.com^ must block subdomains"
    );
    assert!(
        !should_block(
            "https://example.com/ads",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Domain anchor ||ads.example.com^ must not block root example.com/ads"
    );
    assert!(
        !should_block(
            "https://notads.example.com/banner",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Domain anchor ||ads.example.com^ must not block sibling notads.example.com"
    );

    // 1.2 Exact URL anchor: |http://example.com/banner|
    prefs.blocked_hosts.clear();
    prefs
        .blocked_hosts
        .push("|http://example.com/banner|".into());
    assert!(prefs.validate().is_ok());

    assert!(
        should_block(
            "http://example.com/banner",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Exact anchor |http://example.com/banner| must block exact match"
    );
    assert!(
        !should_block(
            "http://example.com/banner/",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Exact anchor |http://example.com/banner| must not block trailing slash"
    );
    assert!(
        !should_block(
            "http://example.com/banner123",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Exact anchor |http://example.com/banner| must not block extended path"
    );
    assert!(
        !should_block(
            "https://example.com/banner",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Exact anchor |http://example.com/banner| must not block https scheme"
    );

    // 1.3 Wildcard domain: *.badsite.net
    prefs.blocked_hosts.clear();
    prefs.blocked_hosts.push("*.badsite.net".into());
    assert!(prefs.validate().is_ok());

    assert!(
        should_block(
            "https://sub.badsite.net/ad",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Wildcard *.badsite.net must block sub.badsite.net"
    );
    assert!(
        should_block(
            "https://deep.sub.badsite.net/ad",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Wildcard *.badsite.net must block deep.sub.badsite.net"
    );
    assert!(
        !should_block(
            "https://notbadsite.net/ad",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Wildcard *.badsite.net must not block notbadsite.net"
    );
}

#[test]
fn test_adversarial_content_type_specifiers() {
    let mut prefs = Preferences::default();

    // Test matrix of all supported content-type specifiers
    let types = [
        ("image", "https://content-test.org/ad.png"),
        ("script", "https://content-test.org/bundle.js"),
        ("stylesheet", "https://content-test.org/styles.css"),
        ("subdocument", "https://content-test.org/frame.html"),
        ("xmlhttprequest", "https://content-test.org/telemetry"),
        ("media", "https://content-test.org/clip.mp4"),
        ("font", "https://content-test.org/font.woff2"),
    ];

    for (target_type, target_url) in &types {
        prefs.blocked_hosts.clear();
        let rule = format!("||content-test.org^${target_type}");
        prefs.blocked_hosts.push(rule.clone());
        assert!(
            prefs.validate().is_ok(),
            "Rule {rule} must pass Preferences::validate"
        );

        // The matching type should be blocked
        assert!(
            should_block(target_url, "https://news.org", target_type, "GET", &prefs),
            "Rule {rule} MUST block {target_type} at {target_url}"
        );

        // Every other type should NOT be blocked by this specific rule
        for (other_type, other_url) in &types {
            if other_type != target_type {
                assert!(
                    !should_block(other_url, "https://news.org", other_type, "GET", &prefs),
                    "Rule {rule} must NOT block other type {other_type} at {other_url}"
                );
            }
        }
    }
}

#[test]
fn test_adversarial_negated_content_types() {
    let mut prefs = Preferences::default();

    // Rule: ||negated-test.org^$~image
    // Should block non-images (script, stylesheet, etc.) but ALLOW image
    prefs
        .blocked_hosts
        .push("||negated-test.org^$~image".into());
    assert!(prefs.validate().is_ok());

    assert!(
        !should_block(
            "https://negated-test.org/pic.png",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Negated rule $~image MUST NOT block image"
    );
    assert!(
        should_block(
            "https://negated-test.org/script.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Negated rule $~image MUST block script"
    );
    assert!(
        should_block(
            "https://negated-test.org/style.css",
            "https://news.org",
            "stylesheet",
            "GET",
            &prefs
        ),
        "Negated rule $~image MUST block stylesheet"
    );

    // Rule: ||negated-script.org^$~script
    // Should block non-scripts (image, media) but ALLOW script
    prefs.blocked_hosts.clear();
    prefs
        .blocked_hosts
        .push("||negated-script.org^$~script".into());
    assert!(prefs.validate().is_ok());

    assert!(
        !should_block(
            "https://negated-script.org/bundle.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Negated rule $~script MUST NOT block script"
    );
    assert!(
        should_block(
            "https://negated-script.org/pic.png",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Negated rule $~script MUST block image"
    );
}

#[test]
fn test_adversarial_method_modifiers() {
    let mut prefs = Preferences::default();

    // Rule: ||method-test.org^$method=POST
    prefs
        .blocked_hosts
        .push("||method-test.org^$method=POST".into());
    assert!(prefs.validate().is_ok());

    assert!(
        should_block(
            "https://method-test.org/api/collect",
            "https://news.org",
            "xmlhttprequest",
            "POST",
            &prefs
        ),
        "Rule $method=POST MUST block POST requests"
    );
    assert!(
        !should_block(
            "https://method-test.org/api/collect",
            "https://news.org",
            "xmlhttprequest",
            "GET",
            &prefs
        ),
        "Rule $method=POST MUST NOT block GET requests"
    );

    // Rule: ||method-test.org^$method=GET
    prefs.blocked_hosts.clear();
    prefs
        .blocked_hosts
        .push("||method-test.org^$method=GET".into());
    assert!(prefs.validate().is_ok());

    assert!(
        should_block(
            "https://method-test.org/api/query",
            "https://news.org",
            "xmlhttprequest",
            "GET",
            &prefs
        ),
        "Rule $method=GET MUST block GET requests"
    );
    assert!(
        !should_block(
            "https://method-test.org/api/query",
            "https://news.org",
            "xmlhttprequest",
            "POST",
            &prefs
        ),
        "Rule $method=GET MUST NOT block POST requests"
    );
}

#[test]
fn test_adversarial_third_party_flags() {
    let mut prefs = Preferences::default();

    // Rule: ||third-party-test.org^$third-party
    prefs
        .blocked_hosts
        .push("||third-party-test.org^$third-party".into());
    assert!(prefs.validate().is_ok());

    // When requested from third-party news.org -> BLOCK
    assert!(
        should_block(
            "https://third-party-test.org/script.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Rule $third-party MUST block request originating from different domain news.org"
    );
    // When requested from first-party third-party-test.org -> ALLOW
    assert!(
        !should_block(
            "https://third-party-test.org/script.js",
            "https://third-party-test.org",
            "script",
            "GET",
            &prefs
        ),
        "Rule $third-party MUST NOT block request originating from same domain"
    );

    // Rule: ||first-party-only.org^$~third-party
    prefs.blocked_hosts.clear();
    prefs
        .blocked_hosts
        .push("||first-party-only.org^$~third-party".into());
    assert!(prefs.validate().is_ok());

    // First party -> BLOCK
    assert!(
        should_block(
            "https://first-party-only.org/script.js",
            "https://first-party-only.org",
            "script",
            "GET",
            &prefs
        ),
        "Rule $~third-party MUST block request originating from first-party domain"
    );
    // Third party -> ALLOW
    assert!(
        !should_block(
            "https://first-party-only.org/script.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Rule $~third-party MUST NOT block request originating from third-party domain"
    );
}

#[test]
fn test_adversarial_exception_rules_priority_and_modifiers() {
    let mut prefs = Preferences::default();

    // Base block: ||cdn.example.org^
    prefs.blocked_hosts.push("||cdn.example.org^".into());
    assert!(prefs.validate().is_ok());

    // Before exception: both script and image are blocked
    assert!(should_block(
        "https://cdn.example.org/tracker.js",
        "https://news.org",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://cdn.example.org/tracker.png",
        "https://news.org",
        "image",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://cdn.example.org/data.json",
        "https://news.org",
        "xmlhttprequest",
        "GET",
        &prefs
    ));

    // Add exception rule specifically for script: @@||cdn.example.org^$script
    prefs
        .allowed_hosts
        .push("@@||cdn.example.org^$script".into());
    assert!(prefs.validate().is_ok());

    // Script MUST be allowed by exception rule
    assert!(
        !should_block(
            "https://cdn.example.org/tracker.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Exception rule @@||cdn.example.org^$script MUST exempt script request"
    );

    // Image and XMLHttpRequest MUST REMAIN BLOCKED because exception was ONLY for $script!
    assert!(
        should_block(
            "https://cdn.example.org/tracker.png",
            "https://news.org",
            "image",
            "GET",
            &prefs
        ),
        "Exception rule @@||cdn.example.org^$script MUST NOT exempt image request"
    );
    assert!(
        should_block(
            "https://cdn.example.org/data.json",
            "https://news.org",
            "xmlhttprequest",
            "GET",
            &prefs
        ),
        "Exception rule @@||cdn.example.org^$script MUST NOT exempt xmlhttprequest"
    );
}

// =========================================================================
// 2. ENGINE CACHE STRESS & CONCURRENCY
// =========================================================================

#[test]
fn test_adversarial_engine_cache_rapid_mutation_1000_checks() {
    let mut prefs = Preferences::default();

    // Sequentially alternate rules and verify cache invalidation across 1,000 checks
    for i in 0..1000 {
        let domain = format!("test-ad-host-{}.com", i % 20);
        let rule = format!("||{domain}^");

        if i % 2 == 0 {
            prefs.blocked_hosts.push(rule.clone());
            let url = format!("https://{domain}/banner.png");
            assert!(
                should_block(&url, "https://news.org", "image", "GET", &prefs),
                "Iteration {i}: Adding {rule} must immediately block {url}"
            );
        } else {
            // Remove the rule
            prefs.blocked_hosts.retain(|r| r != &rule);
            let url = format!("https://{domain}/banner.png");
            // If it's not in bundled rules or blocked_hosts, it should not be blocked
            assert!(
                !should_block(&url, "https://news.org", "image", "GET", &prefs),
                "Iteration {i}: Removing {rule} must immediately allow {url}"
            );
        }
    }
}

#[test]
fn test_adversarial_engine_cache_multithreaded_concurrency() {
    let num_threads = 8;
    let iterations_per_thread = 200;
    let barrier = Arc::new(Barrier::new(num_threads));

    let mut handles = Vec::new();
    for t in 0..num_threads {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            let mut local_prefs = Preferences::default();
            for i in 0..iterations_per_thread {
                let domain = format!("worker-{t}-step-{i}.net");
                let rule = format!("||{domain}^$script");
                local_prefs.blocked_hosts.push(rule);

                let url_script = format!("https://{domain}/test.js");
                let url_image = format!("https://{domain}/test.png");

                // script should be blocked
                assert!(
                    should_block(
                        &url_script,
                        "https://example.com",
                        "script",
                        "GET",
                        &local_prefs
                    ),
                    "Thread {t} iteration {i}: Script must be blocked"
                );
                // image should not be blocked
                assert!(
                    !should_block(
                        &url_image,
                        "https://example.com",
                        "image",
                        "GET",
                        &local_prefs
                    ),
                    "Thread {t} iteration {i}: Image must not be blocked"
                );

                // Now add exception for thread's domain
                let exception = format!("@@||{domain}^$script");
                local_prefs.allowed_hosts.push(exception);

                // Now script should be allowed
                assert!(
                    !should_block(
                        &url_script,
                        "https://example.com",
                        "script",
                        "GET",
                        &local_prefs
                    ),
                    "Thread {t} iteration {i}: Script must now be allowed after exception"
                );

                // Clear for next iteration
                local_prefs.blocked_hosts.clear();
                local_prefs.allowed_hosts.clear();
            }
        }));
    }

    for h in handles {
        h.join()
            .expect("Worker thread panicked during cache concurrency stress");
    }
}

#[test]
fn test_adversarial_multiple_content_types_combination() {
    let mut prefs = Preferences::default();

    // Rule: ||combo-ad.org^$image,script
    prefs
        .blocked_hosts
        .push("||combo-ad.org^$image,script".into());
    assert!(prefs.validate().is_ok());

    // Both image and script should be blocked
    assert!(should_block(
        "https://combo-ad.org/banner.png",
        "https://news.org",
        "image",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://combo-ad.org/track.js",
        "https://news.org",
        "script",
        "GET",
        &prefs
    ));

    // Stylesheet, media, font, xmlhttprequest should NOT be blocked
    assert!(!should_block(
        "https://combo-ad.org/theme.css",
        "https://news.org",
        "stylesheet",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://combo-ad.org/video.mp4",
        "https://news.org",
        "media",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://combo-ad.org/font.woff2",
        "https://news.org",
        "font",
        "GET",
        &prefs
    ));
}

#[test]
fn test_adversarial_domain_restriction_modifier() {
    let mut prefs = Preferences::default();

    // Rule: ||targeted-ad.org^$domain=targetsite.com
    prefs
        .blocked_hosts
        .push("||targeted-ad.org^$domain=targetsite.com".into());
    assert!(prefs.validate().is_ok());

    // When requested on targetsite.com -> BLOCKED
    assert!(
        should_block(
            "https://targeted-ad.org/ad.js",
            "https://targetsite.com",
            "script",
            "GET",
            &prefs
        ),
        "Rule with $domain=targetsite.com MUST block on targetsite.com"
    );

    // When requested on othersite.com -> ALLOWED
    assert!(
        !should_block(
            "https://targeted-ad.org/ad.js",
            "https://othersite.com",
            "script",
            "GET",
            &prefs
        ),
        "Rule with $domain=targetsite.com MUST NOT block on othersite.com"
    );
}

#[test]
fn test_adversarial_exception_path_restrictions() {
    let mut prefs = Preferences::default();

    // Block the entire domain
    prefs.blocked_hosts.push("||multi-path.com^".into());
    assert!(prefs.validate().is_ok());

    // Add exception ONLY for safe path: @@||multi-path.com/safe/*
    prefs.allowed_hosts.push("@@||multi-path.com/safe/*".into());
    assert!(prefs.validate().is_ok());

    // Safe path should be exempt
    assert!(
        !should_block(
            "https://multi-path.com/safe/bundle.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Path exception @@||multi-path.com/safe/* MUST allow safe path"
    );

    // Unsafe path MUST BE BLOCKED
    assert!(
        should_block(
            "https://multi-path.com/unsafe/ad.js",
            "https://news.org",
            "script",
            "GET",
            &prefs
        ),
        "Path exception @@||multi-path.com/safe/* MUST NOT allow unsafe path"
    );
}

#[test]
fn test_adversarial_exception_method_restrictions() {
    let mut prefs = Preferences::default();

    // Block the host
    prefs.blocked_hosts.push("||method-ex.com^".into());
    assert!(prefs.validate().is_ok());

    // Add exception ONLY for POST: @@||method-ex.com^$method=POST
    prefs
        .allowed_hosts
        .push("@@||method-ex.com^$method=POST".into());
    assert!(prefs.validate().is_ok());

    // POST request should be exempt
    assert!(
        !should_block(
            "https://method-ex.com/api",
            "https://news.org",
            "xmlhttprequest",
            "POST",
            &prefs
        ),
        "Method exception @@||method-ex.com^$method=POST MUST allow POST"
    );

    // GET request MUST BE BLOCKED
    assert!(
        should_block(
            "https://method-ex.com/api",
            "https://news.org",
            "xmlhttprequest",
            "GET",
            &prefs
        ),
        "Method exception @@||method-ex.com^$method=POST MUST NOT allow GET"
    );
}
