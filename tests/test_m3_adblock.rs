#![allow(dead_code, unused_imports)]

#[path = "../src/model.rs"]
mod model;

use model::{
    Preferences, extract_rule_domain, is_standard_rule, should_block, valid_host,
    valid_rule_or_host,
};

#[test]
fn test_m3_valid_rule_or_host_comprehensive() {
    // Valid standard hostnames
    assert!(valid_rule_or_host("example.com"));
    assert!(valid_rule_or_host("sub.domain.co.uk"));
    assert!(valid_rule_or_host("tracker123.net"));

    // Valid ABP network rules
    assert!(valid_rule_or_host("||ads.example.com^"));
    assert!(valid_rule_or_host("||googleadservices.com^"));
    assert!(valid_rule_or_host("@@||legit-site.com^$document"));
    assert!(valid_rule_or_host("@@||cdn.assets.com^$script,stylesheet"));
    assert!(valid_rule_or_host("*.doubleclick.net"));
    assert!(valid_rule_or_host("||youtube.com/pagead/*"));
    assert!(valid_rule_or_host("||bad.com^$third-party,image"));
    assert!(valid_rule_or_host("||evil.com^$media"));
    assert!(valid_rule_or_host("@@||connect.facebook.net/*/sdk.js"));

    // Invalid rules
    assert!(!valid_rule_or_host(""));
    assert!(!valid_rule_or_host("   "));
    assert!(!valid_rule_or_host("invalid host with spaces"));
    assert!(!valid_rule_or_host("! comment line"));
    assert!(!valid_rule_or_host("# comment line"));
    assert!(!valid_rule_or_host("bad\0null"));
    assert!(!valid_rule_or_host("rule\nwith\nnewlines"));
}

#[test]
fn test_m3_extract_rule_domain() {
    assert_eq!(extract_rule_domain("example.com"), Some("example.com"));
    assert_eq!(
        extract_rule_domain("||adservice.google.com^"),
        Some("adservice.google.com")
    );
    assert_eq!(
        extract_rule_domain("@@||legit-site.com^$document"),
        Some("legit-site.com")
    );
    assert_eq!(
        extract_rule_domain("*.doubleclick.net"),
        Some("doubleclick.net")
    );
    assert_eq!(
        extract_rule_domain("||youtube.com/pagead/*"),
        Some("youtube.com")
    );
    assert_eq!(
        extract_rule_domain("@@||connect.facebook.net/*/sdk.js"),
        Some("connect.facebook.net")
    );
    assert_eq!(
        extract_rule_domain("@@||other-site.org^$document"),
        Some("other-site.org")
    );
    assert_eq!(extract_rule_domain(""), None);
    assert_eq!(extract_rule_domain("||"), None);
    assert_eq!(extract_rule_domain("^"), None);
}

#[test]
fn test_m3_preferences_validation_with_filter_rules() {
    let mut prefs = Preferences::default();
    assert!(prefs.validate().is_ok());

    prefs.blocked_hosts.push("||adservice.google.com^".into());
    prefs.blocked_hosts.push("*.doubleclick.net".into());
    prefs
        .allowed_hosts
        .push("@@||legit-site.com^$document".into());
    prefs.allowed_hosts.push("plain-host.vn".into());
    assert!(prefs.validate().is_ok());

    // Invalid rule in blocked_hosts
    prefs.blocked_hosts.push("invalid entry with spaces".into());
    assert!(prefs.validate().is_err());
}

#[test]
fn test_m3_resource_context_and_methods() {
    let prefs = Preferences::default();

    // SCRIPT
    assert!(should_block(
        "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
        "https://example.org",
        "script",
        "GET",
        &prefs
    ));

    // IMAGE
    assert!(should_block(
        "https://googleads.g.doubleclick.net/pagead/imgad?id=123",
        "https://example.org",
        "image",
        "GET",
        &prefs
    ));

    // XMLHTTPREQUEST / FETCH with POST
    assert!(should_block(
        "https://analytics.tiktok.com/api/v2/track",
        "https://example.org",
        "xmlhttprequest",
        "POST",
        &prefs
    ));

    // MAIN_FRAME should not be blocked for legitimate websites
    assert!(!should_block(
        "https://example.org/",
        "https://example.org/",
        "main_frame",
        "GET",
        &prefs
    ));
}

#[test]
fn test_m3_bundled_rules_ad_networks_and_coin_miners() {
    let prefs = Preferences::default();

    // Ad networks
    assert!(should_block(
        "https://pubmatic.com/ad.js",
        "https://news.com",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://openx.net/track",
        "https://news.com",
        "xmlhttprequest",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://rubiconproject.com/pixel",
        "https://news.com",
        "image",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://popcash.net/pop.js",
        "https://news.com",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://mc.yandex.ru/metrika/tag.js",
        "https://news.com",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://googleadservices.com/pagead/conversion.js",
        "https://news.com",
        "script",
        "GET",
        &prefs
    ));

    // Cryptocurrency miners
    assert!(should_block(
        "https://coinhive.com/lib/miner.js",
        "https://news.com",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://crypto-loot.com/miner.js",
        "https://news.com",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://jsecoin.com/server.js",
        "https://news.com",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://authedmine.com/lib/authedmine.min.js",
        "https://news.com",
        "script",
        "GET",
        &prefs
    ));
    assert!(should_block(
        "https://monerominer.rocks/miner.js",
        "https://news.com",
        "script",
        "GET",
        &prefs
    ));

    // First-party exceptions must never be blocked
    assert!(!should_block(
        "https://accounts.google.com/o/oauth2/v2/auth",
        "https://example.com",
        "main_frame",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://fonts.googleapis.com/css?family=Open+Sans",
        "https://example.com",
        "stylesheet",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://ajax.googleapis.com/ajax/libs/jquery/3.6.0/jquery.min.js",
        "https://example.com",
        "script",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://connect.facebook.net/en_US/sdk.js",
        "https://example.com",
        "script",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://connect.facebook.net/vi_VN/sdk.js",
        "https://example.com",
        "script",
        "GET",
        &prefs
    ));
}

#[test]
fn test_m3_layout_stability_subresource_whitelisting() {
    let mut prefs = Preferences::default();

    // On unwhitelisted site, ads are blocked
    assert!(should_block(
        "https://doubleclick.net/ad.js",
        "https://news.com/",
        "script",
        "GET",
        &prefs
    ));

    // Whitelisting site allows all subresources (fonts, styles, scripts)
    prefs.allowed_hosts.push("legit-site.com".into());
    assert!(!should_block(
        "https://legit-site.com/app.js",
        "https://legit-site.com/",
        "script",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://fonts.googleapis.com/css",
        "https://legit-site.com/",
        "stylesheet",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://fonts.gstatic.com/font.woff2",
        "https://legit-site.com/",
        "font",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://doubleclick.net/ad.js",
        "https://legit-site.com/",
        "script",
        "GET",
        &prefs
    ));

    // Whitelisting via ABP exception rule @@||other-site.org^$document
    prefs.allowed_hosts.clear();
    prefs
        .allowed_hosts
        .push("@@||other-site.org^$document".into());
    assert!(!should_block(
        "https://cdn.other-site.org/style.css",
        "https://other-site.org/",
        "stylesheet",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://doubleclick.net/ad.js",
        "https://other-site.org/",
        "script",
        "GET",
        &prefs
    ));
}

#[test]
fn test_m3_adblock_toggle_and_dynamic_user_rules() {
    let mut prefs = Preferences::default();

    // Normal blocking
    assert!(should_block(
        "https://doubleclick.net/ad.js",
        "https://example.org",
        "script",
        "GET",
        &prefs
    ));

    // Adblock disabled
    prefs.adblock = false;
    assert!(!should_block(
        "https://doubleclick.net/ad.js",
        "https://example.org",
        "script",
        "GET",
        &prefs
    ));
    assert!(!should_block(
        "https://pubmatic.com/ad.js",
        "https://example.org",
        "script",
        "GET",
        &prefs
    ));

    // Re-enable and add custom user rule
    prefs.adblock = true;
    prefs
        .blocked_hosts
        .push("||my-custom-tracker.com^$script".into());
    assert!(should_block(
        "https://my-custom-tracker.com/track.js",
        "https://example.org",
        "script",
        "GET",
        &prefs
    ));
    // Images from custom tracker are not blocked because rule specifies $script
    assert!(!should_block(
        "https://my-custom-tracker.com/logo.png",
        "https://example.org",
        "image",
        "GET",
        &prefs
    ));
}
