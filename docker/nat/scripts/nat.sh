#!/bin/sh
set -eu

lan_subnet="$1"
peer_wan="$2"
lan_ip="$3"
wan_ip="$4"

if [ "$(cat /proc/sys/net/ipv4/ip_forward)" != "1" ]; then
    echo "net.ipv4.ip_forward is not enabled" >&2
    exit 1
fi

lan_if=$(ip -o -4 addr show | awk -v ip="$lan_ip" 'index($4, ip "/") == 1 { print $2; exit }')
wan_if=$(ip -o -4 addr show | awk -v ip="$wan_ip" 'index($4, ip "/") == 1 { print $2; exit }')
if [ -z "$lan_if" ] || [ -z "$wan_if" ]; then
    echo "failed to resolve interfaces: lan=$lan_ip wan=$wan_ip" >&2
    ip -o -4 addr show >&2
    exit 1
fi

iptables -t nat -F
iptables -F
iptables -P FORWARD DROP
iptables -P INPUT ACCEPT
iptables -P OUTPUT ACCEPT

# Drop unsolicited WAN input so the kernel does not answer hole-punch
# probes with RST before conntrack has a mapping.
iptables -A INPUT -i "$wan_if" -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
iptables -A INPUT -i "$wan_if" -j DROP

iptables -t nat -A POSTROUTING -s "$lan_subnet" -o "$wan_if" -j MASQUERADE

# Relay-only mode: drop direct UDP between the two public NAT addresses.
# The relay itself (172.30.0.10) is still reachable.
if [ "${BLOCK_DIRECT:-1}" = "1" ]; then
    iptables -A FORWARD -p udp -d "$peer_wan" -j DROP
fi

iptables -A FORWARD -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
iptables -A FORWARD -s "$lan_subnet" -i "$lan_if" -o "$wan_if" -j ACCEPT
iptables -A FORWARD -d "$lan_subnet" -i "$wan_if" -o "$lan_if" -j ACCEPT

netem_args=""
if [ -n "${NETEM_DELAY:-}" ]; then
    netem_args="delay ${NETEM_DELAY}"
fi
if [ -n "${NETEM_LOSS:-}" ]; then
    netem_args="${netem_args} loss ${NETEM_LOSS}"
fi
if [ -n "$netem_args" ]; then
    tc qdisc replace dev "$wan_if" root netem $netem_args
    tc qdisc replace dev "$lan_if" root netem $netem_args
fi

exec sleep infinity
