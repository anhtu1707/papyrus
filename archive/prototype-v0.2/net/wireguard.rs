#[derive(Clone, Debug, PartialEq)]
pub struct VpnNode {
    pub name: &'static str,
    pub country_flag: &'static str,
    pub ping_ms: u32,
    pub bandwidth: &'static str,
    pub jurisdiction: &'static str,
}

pub struct WireGuardManager {
    pub enabled: bool,
    pub kill_switch: bool,
    pub active_node_idx: usize,
    pub nodes: Vec<VpnNode>,
}

impl WireGuardManager {
    pub fn new() -> Self {
        Self {
            enabled: true,
            kill_switch: true,
            active_node_idx: 0,
            nodes: vec![
                VpnNode {
                    name: "Zurich Entry",
                    country_flag: "🇨🇭",
                    ping_ms: 14,
                    bandwidth: "10 Gbps",
                    jurisdiction: "Swiss Federal Data Protection Act",
                },
                VpnNode {
                    name: "Reykjavik Relay",
                    country_flag: "🇮🇸",
                    ping_ms: 28,
                    bandwidth: "10 Gbps",
                    jurisdiction: "Icelandic Zero-Log Freedom",
                },
                VpnNode {
                    name: "Tokyo Exit",
                    country_flag: "🇯🇵",
                    ping_ms: 42,
                    bandwidth: "5 Gbps",
                    jurisdiction: "Asian Fast Gateway",
                },
                VpnNode {
                    name: "Singapore Relay",
                    country_flag: "🇸🇬",
                    ping_ms: 22,
                    bandwidth: "10 Gbps",
                    jurisdiction: "SEA Low Latency Hub",
                },
            ],
        }
    }

    pub fn active_node(&self) -> &VpnNode {
        &self.nodes[self.active_node_idx]
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn select_node(&mut self, idx: usize) {
        if idx < self.nodes.len() {
            self.active_node_idx = idx;
        }
    }
}
