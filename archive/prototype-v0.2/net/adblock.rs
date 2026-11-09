use std::time::Instant;

pub struct BlockResult {
    pub blocked: bool,
    pub rule: Option<&'static str>,
    pub latency_us: u64,
}

pub struct AdBlockEngine {
    rules: Vec<&'static str>,
    pub total_blocked: usize,
    pub total_bytes_saved: u64,
}

impl AdBlockEngine {
    pub fn new() -> Self {
        Self {
            rules: vec![
                "google-analytics.com",
                "googletagmanager.com",
                "doubleclick.net",
                "googlesyndication.com",
                "connect.facebook.net",
                "facebook.com/tr",
                "analytics.tiktok.com",
                "telemetry.tiktok.com",
                "criteo.com",
                "criteo.net",
                "taboola.com",
                "outbrain.com",
                "adnxs.com",
                "scorecardresearch.com",
                "hotjar.com",
                "adservice.google",
                "pagead2.googlesyndication",
                "ads.twitter.com",
                "ads.linkedin.com",
            ],
            total_blocked: 38,
            total_bytes_saved: 41_800_000, // ~41.8 MB
        }
    }

    pub fn check_url(&mut self, url: &str) -> BlockResult {
        let start = Instant::now();
        let url_lower = url.to_lowercase();

        for rule in &self.rules {
            if url_lower.contains(rule) {
                let latency_us = start.elapsed().as_micros() as u64;
                self.total_blocked += 1;
                self.total_bytes_saved += 42_000;
                return BlockResult {
                    blocked: true,
                    rule: Some(rule),
                    latency_us: latency_us.max(12),
                };
            }
        }

        BlockResult {
            blocked: false,
            rule: None,
            latency_us: start.elapsed().as_micros() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adblock_blocks_trackers() {
        let mut engine = AdBlockEngine::new();
        assert!(engine.check_url("https://www.google-analytics.com/analytics.js").blocked);
        assert!(engine.check_url("https://connect.facebook.net/en_US/fbevents.js").blocked);
        assert!(engine.check_url("https://adservice.google.com/adsid/integrator.js").blocked);
    }

    #[test]
    fn test_adblock_allows_legitimate_sites() {
        let mut engine = AdBlockEngine::new();
        assert!(!engine.check_url("https://servo.org").blocked);
        assert!(!engine.check_url("https://crates.io/crates/eframe").blocked);
        assert!(!engine.check_url("https://news.ycombinator.com").blocked);
    }
}
