#![allow(dead_code, unused_imports)]

#[path = "../src/model.rs"]
mod model;

use model::{
    Preferences, extract_rule_domain, is_standard_rule, should_block, valid_host,
    valid_rule_or_host,
};

// ============================================================================
// SUITE 1: Whitelisting Invariants & Sub-resource Stability
// ============================================================================

#[test]
fn test_m3_2_whitelisted_plain_domain_subresource_invariant() {
    let mut prefs = Preferences::default();
    prefs.allowed_hosts.push("news.com".into());

    let top_url = "https://news.com/article/breaking-news-today";

    // 1. Remote webfonts must NEVER be blocked
    assert!(
        !should_block(
            "https://fonts.googleapis.com/css?family=Roboto:300,400,700&display=swap",
            top_url,
            "stylesheet",
            "GET",
            &prefs
        ),
        "Remote Google Fonts stylesheet must not be blocked on whitelisted news.com"
    );

    assert!(
        !should_block(
            "https://fonts.gstatic.com/s/roboto/v30/KFOmCnqEu92Fr1Mu4mxK.woff2",
            top_url,
            "font",
            "GET",
            &prefs
        ),
        "Remote Google Fonts binary font file must not be blocked on whitelisted news.com"
    );

    // 2. Remote CDNs and stylesheets must NEVER be blocked
    assert!(
        !should_block(
            "https://cdnjs.cloudflare.com/ajax/libs/normalize/8.0.1/normalize.min.css",
            top_url,
            "stylesheet",
            "GET",
            &prefs
        ),
        "CDNJS stylesheet must not be blocked on whitelisted news.com"
    );

    assert!(
        !should_block(
            "https://cdn.jsdelivr.net/npm/bootstrap@5.3.0/dist/css/bootstrap.min.css",
            top_url,
            "stylesheet",
            "GET",
            &prefs
        ),
        "jsDelivr stylesheet must not be blocked on whitelisted news.com"
    );

    assert!(
        !should_block(
            "https://cdnjs.cloudflare.com/ajax/libs/jquery/3.6.4/jquery.min.js",
            top_url,
            "script",
            "GET",
            &prefs
        ),
        "CDNJS script must not be blocked on whitelisted news.com"
    );

    // 3. First-party API requests and scripts must NEVER be blocked
    assert!(
        !should_block(
            "https://news.com/static/js/bundle.min.js",
            top_url,
            "script",
            "GET",
            &prefs
        ),
        "First-party script must not be blocked on news.com"
    );

    assert!(
        !should_block(
            "https://news.com/api/v1/breaking-articles",
            top_url,
            "xmlhttprequest",
            "GET",
            &prefs
        ),
        "First-party GET API must not be blocked on news.com"
    );

    assert!(
        !should_block(
            "https://news.com/api/v1/user-preferences",
            top_url,
            "xmlhttprequest",
            "POST",
            &prefs
        ),
        "First-party POST API must not be blocked on news.com"
    );

    assert!(
        !should_block(
            "https://api.news.com/graphql",
            top_url,
            "xmlhttprequest",
            "POST",
            &prefs
        ),
        "First-party GraphQL API on subdomain must not be blocked on news.com"
    );

    // 4. Subdomains of whitelisted domain must also be protected
    let sub_top_url = "https://finance.news.com/markets";
    assert!(
        !should_block(
            "https://fonts.googleapis.com/css?family=Open+Sans",
            sub_top_url,
            "stylesheet",
            "GET",
            &prefs
        ),
        "Webfonts on finance.news.com must not be blocked"
    );
    assert!(
        !should_block(
            "https://cdn.jsdelivr.net/npm/chart.js",
            sub_top_url,
            "script",
            "GET",
            &prefs
        ),
        "Chart.js on finance.news.com must not be blocked"
    );
}

#[test]
fn test_m3_2_whitelisted_abp_document_rule_subresource_invariant() {
    let mut prefs = Preferences::default();
    prefs.allowed_hosts.push("@@||news.com^$document".into());

    let top_url = "https://news.com/opinion/tech";

    // 1. Remote webfonts must NEVER be blocked
    assert!(
        !should_block(
            "https://fonts.googleapis.com/css?family=Inter:400,600",
            top_url,
            "stylesheet",
            "GET",
            &prefs
        ),
        "Google Fonts stylesheet must not be blocked with @@||news.com^$document"
    );

    assert!(
        !should_block(
            "https://fonts.gstatic.com/s/inter/v12/UcCO3FwrK3iLTeHuS_fvQtMwCp50KnMw2boKoduKmMEVuLyfAZ9hjp-Ek-_EeA.woff2",
            top_url,
            "font",
            "GET",
            &prefs
        ),
        "Google Fonts woff2 must not be blocked with @@||news.com^$document"
    );

    // 2. Remote stylesheets and CDNs must NEVER be blocked
    assert!(
        !should_block(
            "https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.4.0/css/all.min.css",
            top_url,
            "stylesheet",
            "GET",
            &prefs
        ),
        "FontAwesome CDN CSS must not be blocked with @@||news.com^$document"
    );

    assert!(
        !should_block(
            "https://cdn.jsdelivr.net/npm/tailwind-cdn@latest/tailwind.min.css",
            top_url,
            "stylesheet",
            "GET",
            &prefs
        ),
        "Tailwind CDN CSS must not be blocked with @@||news.com^$document"
    );

    // 3. First-party API requests and scripts must NEVER be blocked
    assert!(
        !should_block(
            "https://news.com/assets/app.js",
            top_url,
            "script",
            "GET",
            &prefs
        ),
        "First party script must not be blocked with @@||news.com^$document"
    );

    assert!(
        !should_block(
            "https://news.com/api/comments?story_id=9872",
            top_url,
            "xmlhttprequest",
            "GET",
            &prefs
        ),
        "First party comments API must not be blocked with @@||news.com^$document"
    );

    assert!(
        !should_block(
            "https://news.com/auth/session/refresh",
            top_url,
            "xmlhttprequest",
            "POST",
            &prefs
        ),
        "First party session refresh POST must not be blocked with @@||news.com^$document"
    );
}

// ============================================================================
// SUITE 2: Unwhitelisted Sites & Ad Network / Coin Miner Blocking
// ============================================================================

#[test]
fn test_m3_2_unwhitelisted_ad_networks_and_trackers_are_blocked() {
    let prefs = Preferences::default();
    let top_url = "https://unwhitelisted-blog.org/articles/how-to-code";

    let ad_network_requests = [
        ("https://doubleclick.net/pagead/ads.js", "script", "GET"),
        (
            "https://googleads.g.doubleclick.net/pagead/ads?client=ca-pub-123",
            "subdocument",
            "GET",
        ),
        (
            "https://securepubads.g.doubleclick.net/tag/js/gpt.js",
            "script",
            "GET",
        ),
        (
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "script",
            "GET",
        ),
        (
            "https://partner.googleadservices.com/gampad/cookie.js",
            "script",
            "GET",
        ),
        (
            "https://googleadservices.com/pagead/conversion.js",
            "script",
            "GET",
        ),
        (
            "https://adservice.google.com/adsid/google/ui",
            "script",
            "GET",
        ),
        ("https://pubmatic.com/adserver.js", "script", "GET"),
        ("https://openx.net/w/1.0/afr", "subdocument", "GET"),
        ("https://rubiconproject.com/pixel", "image", "GET"),
        ("https://adnxs.com/seg?add=1", "image", "GET"),
        ("https://criteo.com/delivery/ajs.php", "script", "GET"),
        (
            "https://taboola.com/libtrc/unwhitelisted/loader.js",
            "script",
            "GET",
        ),
        ("https://outbrain.com/outbrain.js", "script", "GET"),
        ("https://mc.yandex.ru/metrika/tag.js", "script", "GET"),
        ("https://popcash.net/show.js", "script", "GET"),
        ("https://popads.net/pop.js", "script", "GET"),
        ("https://google-analytics.com/analytics.js", "script", "GET"),
        (
            "https://googletagmanager.com/gtm.js?id=GTM-XXXX",
            "script",
            "GET",
        ),
        ("https://hotjar.com/c/hotjar-12345.js?sv=7", "script", "GET"),
    ];

    for (url, res_type, method) in ad_network_requests {
        assert!(
            should_block(url, top_url, res_type, method, &prefs),
            "Expected ad network request to be blocked: {url} ({res_type})"
        );
    }
}

#[test]
fn test_m3_2_unwhitelisted_coin_miners_are_blocked() {
    let prefs = Preferences::default();
    let top_url = "https://some-free-game-site.net/play";

    let miner_requests = [
        ("https://coinhive.com/lib/coinhive.min.js", "script", "GET"),
        ("https://coin-hive.com/lib/worker.js", "script", "GET"),
        ("https://crypto-loot.com/miner.js", "script", "GET"),
        ("https://jsecoin.com/server.js", "script", "GET"),
        (
            "https://authedmine.com/lib/authedmine.min.js",
            "script",
            "GET",
        ),
        (
            "https://monerominer.rocks/scripts/miner.js",
            "script",
            "GET",
        ),
        ("https://minr.pw/miner.js", "script", "GET"),
        ("https://webminepool.com/lib/base.js", "script", "GET"),
    ];

    for (url, res_type, method) in miner_requests {
        assert!(
            should_block(url, top_url, res_type, method, &prefs),
            "Expected cryptocurrency miner to be blocked: {url} ({res_type})"
        );
    }
}

#[test]
fn test_m3_2_unwhitelisted_site_legitimate_subresources_not_blocked() {
    let prefs = Preferences::default();
    let top_url = "https://unwhitelisted-independent-site.org/index.html";

    // Remote webfonts have bundled exception rules
    assert!(
        !should_block(
            "https://fonts.googleapis.com/css2?family=Open+Sans&display=swap",
            top_url,
            "stylesheet",
            "GET",
            &prefs
        ),
        "Google Fonts CSS must not be blocked on unwhitelisted sites"
    );

    assert!(
        !should_block(
            "https://fonts.gstatic.com/s/opensans/v40/memvYaGs126MiZpBA-UvWbX2vVnXBbObj2OVTS-mu0SC55I.woff2",
            top_url,
            "font",
            "GET",
            &prefs
        ),
        "Google Fonts binary font must not be blocked on unwhitelisted sites"
    );

    // Standard public CDNs are not ad networks or trackers
    assert!(
        !should_block(
            "https://cdnjs.cloudflare.com/ajax/libs/react/18.2.0/umd/react.production.min.js",
            top_url,
            "script",
            "GET",
            &prefs
        ),
        "CDNJS React library must not be blocked"
    );

    assert!(
        !should_block(
            "https://cdn.jsdelivr.net/npm/vue@3.3.4/dist/vue.global.prod.js",
            top_url,
            "script",
            "GET",
            &prefs
        ),
        "jsDelivr Vue library must not be blocked"
    );

    // First-party resources on the unwhitelisted site itself
    assert!(
        !should_block(
            "https://unwhitelisted-independent-site.org/style.css",
            top_url,
            "stylesheet",
            "GET",
            &prefs
        ),
        "First-party style.css must not be blocked"
    );

    assert!(
        !should_block(
            "https://unwhitelisted-independent-site.org/js/app.js",
            top_url,
            "script",
            "GET",
            &prefs
        ),
        "First-party app.js must not be blocked"
    );

    assert!(
        !should_block(
            "https://unwhitelisted-independent-site.org/api/user/profile",
            top_url,
            "xmlhttprequest",
            "GET",
            &prefs
        ),
        "First-party profile API must not be blocked"
    );
}

// ============================================================================
// SUITE 3: ABP Syntax & Settings Parsing Stress Tests
// ============================================================================

#[test]
fn test_m3_2_valid_rule_or_host_stress_matrix() {
    let valid_cases = [
        "example.com",
        "sub.domain.org.vn",
        "a-b-c.co",
        "127.0.0.1",
        "||doubleclick.net^",
        "||googleadservices.com^$script",
        "||adserver.com^$image,stylesheet",
        "||tracker.io^$third-party",
        "||badsite.net^$~image",
        "@@||news.com^$document",
        "@@||legit-site.org^",
        "@@||fonts.googleapis.com^",
        "*.adnetwork.com",
        "||youtube.com/pagead/*",
        "/ads/banner_*.gif",
        "||bad.com^$method=POST",
        "|https://exact.url.com/ad.js|",
        "||sub.example.com^$domain=example.com|news.com",
    ];

    for case in valid_cases {
        assert!(
            valid_rule_or_host(case),
            "Expected rule to be valid: {case}"
        );
    }

    let invalid_cases = [
        "",
        "   ",
        "\t",
        "\n",
        "\r\n",
        "has space in rule",
        "! adblock comment line",
        "# another comment syntax",
        "null\0byte",
        "newlines\nin\nrule",
        "carriage\rreturn",
        &"a".repeat(1025), // length > 1024
    ];

    for case in invalid_cases {
        assert!(
            !valid_rule_or_host(case),
            "Expected rule to be invalid: {case:?}"
        );
    }
}

#[test]
fn test_m3_2_preferences_validation_stress_limits() {
    let mut prefs = Preferences::default();
    assert!(prefs.validate().is_ok());

    // Populate with 2,000 valid rules (within 10,000 limit)
    for i in 0..1000 {
        prefs
            .blocked_hosts
            .push(format!("||tracker{i}.adnetwork.com^"));
        prefs
            .allowed_hosts
            .push(format!("@@||legit{i}.portal.vn^$document"));
    }
    assert!(
        prefs.validate().is_ok(),
        "2000 valid rules should pass validation"
    );

    // Adding 1 invalid rule causes validate() to fail
    prefs.blocked_hosts.push("this rule has spaces.com".into());
    assert!(
        prefs.validate().is_err(),
        "Invalid rule with space must fail validation"
    );

    // Remove invalid rule
    prefs.blocked_hosts.pop();
    assert!(prefs.validate().is_ok());

    // Exceeding 10,000 rules boundary
    for i in 1000..5001 {
        prefs
            .blocked_hosts
            .push(format!("||tracker{i}.adnetwork.com^"));
        prefs
            .allowed_hosts
            .push(format!("@@||legit{i}.portal.vn^$document"));
    }
    // Total is now 10,002 > 10,000
    assert!(
        prefs.validate().is_err(),
        "Exceeding 10,000 rules must fail validation"
    );
}

#[test]
fn test_m3_2_engine_cache_invalidation_and_concurrency() {
    let mut prefs1 = Preferences::default();
    prefs1
        .blocked_hosts
        .push("||dynamic-target-one.com^".into());

    let mut prefs2 = Preferences::default();
    prefs2
        .blocked_hosts
        .push("||dynamic-target-two.com^".into());

    let top = "https://example.org";

    // Request with prefs1
    assert!(should_block(
        "https://dynamic-target-one.com/ad.js",
        top,
        "script",
        "GET",
        &prefs1
    ));
    assert!(!should_block(
        "https://dynamic-target-two.com/ad.js",
        top,
        "script",
        "GET",
        &prefs1
    ));

    // Request with prefs2 (must invalidate cache and recompile)
    assert!(!should_block(
        "https://dynamic-target-one.com/ad.js",
        top,
        "script",
        "GET",
        &prefs2
    ));
    assert!(should_block(
        "https://dynamic-target-two.com/ad.js",
        top,
        "script",
        "GET",
        &prefs2
    ));

    // Multithreaded concurrency test
    let handles: Vec<_> = (0..8)
        .map(|thread_id| {
            let p1 = prefs1.clone();
            let p2 = prefs2.clone();
            std::thread::spawn(move || {
                for iter in 0..100 {
                    let (p, target_blocked, target_allowed) = if (thread_id + iter) % 2 == 0 {
                        (
                            &p1,
                            "https://dynamic-target-one.com/ad.js",
                            "https://dynamic-target-two.com/ad.js",
                        )
                    } else {
                        (
                            &p2,
                            "https://dynamic-target-two.com/ad.js",
                            "https://dynamic-target-one.com/ad.js",
                        )
                    };
                    assert!(should_block(
                        target_blocked,
                        "https://example.org",
                        "script",
                        "GET",
                        p
                    ));
                    assert!(!should_block(
                        target_allowed,
                        "https://example.org",
                        "script",
                        "GET",
                        p
                    ));
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Thread should join cleanly without panics");
    }
}

#[test]
fn test_m3_2_exception_rule_modifiers_must_not_broadly_whitelist_domain() {
    let mut prefs = Preferences::default();
    prefs.blocked_hosts.push("||adserver.org^".into());
    // Rule specifies $script ONLY
    prefs.allowed_hosts.push("@@||adserver.org^$script".into());
    assert!(prefs.validate().is_ok());

    // Script request to adserver.org must be exempt
    assert!(
        !should_block(
            "https://adserver.org/track.js",
            "https://unwhitelisted.com",
            "script",
            "GET",
            &prefs
        ),
        "Script request must be exempt by @@||adserver.org^$script"
    );

    // Image request to adserver.org MUST NOT be exempt because exception rule was specifically for $script!
    assert!(
        should_block(
            "https://adserver.org/track.png",
            "https://unwhitelisted.com",
            "image",
            "GET",
            &prefs
        ),
        "Image request to adserver.org MUST remain blocked despite @@||adserver.org^$script"
    );
}
