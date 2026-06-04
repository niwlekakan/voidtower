// Phase 3: Multi-node agent mode
// Agent nodes expose /agent/metrics and /agent/actions
// Main node polls agents, proxies actions, manages join tokens
// No external consensus (etcd/Consul/K8s) required

pub fn is_agent_mode() -> bool {
    std::env::args().any(|a| a == "--agent")
}
