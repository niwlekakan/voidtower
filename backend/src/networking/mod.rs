// Phase 3: Network interface and firewall management
// Reads /sys/class/net, /proc/net/route, invokes ip/ss commands
// Firewall backends: UFW, firewalld, nftables, iptables
// This module will expose:
//   - list_interfaces() -> Vec<NetworkInterface>
//   - list_routes() -> Vec<RouteInfo>
//   - get_firewall_status() -> FirewallStatus
//   - list_listening_ports() -> Vec<PortInfo>
