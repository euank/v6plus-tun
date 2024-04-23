use anyhow::bail;
use clap::{Parser, Subcommand};
use cmd_lib::run_cmd;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    sub: Subcommands,
}

#[derive(Parser)]
struct Calculate {
    #[arg(required = true)]
    addr: std::net::Ipv6Addr,
}

#[derive(Debug)]
struct MapEData {
    addr: std::net::Ipv6Addr,
    ipv4_addr: std::net::Ipv4Addr,
    br_addr: std::net::Ipv6Addr,
    edge_addr: std::net::Ipv6Addr,
    psid: u8,
    port_ranges: Vec<(u16, u16)>,
}

impl std::fmt::Display for MapEData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "IPv4 Addr (CE IPv4 Address): {}", self.ipv4_addr)?;
        writeln!(f, "CE IPv6 Addr: {}", self.edge_addr)?;
        writeln!(
            f,
            "Port Ranges: {}",
            self.port_ranges
                .iter()
                .map(|el| format!("{}-{}", el.0, el.1))
                .collect::<Vec<_>>()
                .join(", ")
        )?;
        writeln!(f, "PSID: {}", self.psid)?;
        writeln!(f, "Border Relay Address (BR Address): {}", self.br_addr)
    }
}

impl Calculate {
    fn calculate(&self) -> anyhow::Result<MapEData> {
        let v6_segs = self.addr.segments();
        // Base mapping rules I think? Pulled from ~the internet~
        let ipv4_prefix = match (v6_segs[0], v6_segs[1]) {
            (0x2404, 0x7a80) => (133, 200),
            (0x2404, 0x7a84) => (133, 206),
            (0x240b, 0x10) => (106, 72),
            (0x240b, 0x11) => (106, 73),
            (0x240b, 0x12) => (14, 8),
            (0x240b, 0x250) => (14, 10),
            (0x240b, 0x251) => (14, 11),
            (0x240b, 0x252) => (14, 12),
            (0x240b, 0x253) => (14, 13),
            (a, b) => {
                bail!("unknown prefix: {:x}:{:x}", a, b);
            }
        };

        let v6_octets = self.addr.octets();
        let psid = v6_octets[6];
        // the last two octets of the map-e v4 address are just taken from the v6 address's 3rd
        // segment
        let ipv4_addr =
            std::net::Ipv4Addr::new(ipv4_prefix.0, ipv4_prefix.1, v6_octets[4], v6_octets[5]);
        let ipv4_octets = ipv4_addr.octets();

        let ce = std::net::Ipv6Addr::new(
            v6_segs[0],
            v6_segs[1],
            ((ipv4_octets[2] as u16) << 8) + ipv4_octets[3] as u16,
            (psid as u16) << 8,
            ipv4_octets[0] as u16,
            ((ipv4_octets[1] as u16) << 8) + ipv4_octets[2] as u16,
            (ipv4_octets[3] as u16) << 8,
            (psid as u16) << 8,
        );

        let prefix31: u32 = self
            .addr
            .segments()
            .into_iter()
            .take(2)
            .map(|el| el as u32)
            .reduce(|l, r| (l << 16) + (r & 0xfffe))
            .unwrap();
        let br_addr = if (0x24047a80..0x24047a84).contains(&prefix31) {
            std::net::Ipv6Addr::new(0x2001, 0x260, 0x700, 0x1, 0, 0, 0x1, 0x275)
        } else if (0x24047a84..0x24047a88).contains(&prefix31) {
            std::net::Ipv6Addr::new(0x2001, 0x260, 0x700, 0x1, 0, 0, 0x1, 0x276)
        } else if (0x240b0010..0x240b0014).contains(&prefix31)
            || (0x240b0250..0x240b0254).contains(&prefix31)
        {
            std::net::Ipv6Addr::new(0x2404, 0x9200, 0x225, 0x100, 0, 0, 0, 0x64)
        } else {
            bail!("unrecognized prefix");
        };

        let data = MapEData {
            addr: self.addr,
            ipv4_addr,
            // Also called "CE"
            edge_addr: ce,
            psid,
            br_addr,
            port_ranges: (1..=15)
                .map(|i| {
                    (
                        (i << 12) + ((psid as u16) << 4),
                        ((i << 12) + ((psid as u16) << 4) + 0xf),
                    )
                })
                .collect(),
        };
        Ok(data)
    }
}

#[derive(Parser)]
struct SetupLinux {
    #[arg(required = true)]
    addr: std::net::Ipv6Addr,
    #[arg(
        long = "wan",
        required = true,
        help = "WAN interface device, such as 'enp0s1' or 'eth0'"
    )]
    wan_dev: String,
    #[arg(
        long = "tun",
        default_value = "ip4tun0",
        help = "Tunnel interface to create, such as 'iptun0'"
    )]
    tun_dev: String,
    #[arg(
        long = "add-ipv4-addr",
        default_value = "false",
        help = "Add your public ipv4 address to the wan interface"
    )]
    add_ipv4_wan: bool,
    #[arg(
        long = "no-snat-ipv4-ports",
        default_value = "",
        help = "IPv4 ports to not SNAT to, for example ports used for port-forwarding or such"
    )]
    no_snat_ports: Vec<u16>,
}

impl SetupLinux {
    fn setup(&self) -> anyhow::Result<()> {
        let data = Calculate { addr: self.addr }.calculate()?;
        let (tun_dev, br_addr, edge_addr, wan_dev, ipv4_addr) = (
            &self.tun_dev,
            data.br_addr,
            data.edge_addr,
            &self.wan_dev,
            data.ipv4_addr,
        );

        let mut port_ranges = data.port_ranges.clone();

        // take into account extra no-snat ports
        for &port in self.no_snat_ports.iter() {
            // Find the port_range
            let next = (port_ranges)
                .iter()
                .flat_map(|&(start, end)| {
                    if port == start {
                        vec![(start + 1, end)]
                    } else if port == end {
                        vec![(start, end - 1)]
                    } else if port > start && port < end {
                        vec![(start, port - 1), (port + 1, end)]
                    } else {
                        vec![(start, end)]
                    }
                })
                .filter(|el| -> bool {
                    // If we excluded an entire port range, or two consecutive ports, we can hit this,
                    // trim out any zero-sized ranges
                    el.0 <= el.1
                })
                .collect::<Vec<_>>();
            if next == port_ranges {
                bail!("specified 'no_snat_port' {port} was not in any port range")
            }
            port_ranges = next;
        }


        // This is a copy of a well-known bash script that floats around the internet for people
        // doing this sorta thing.
        // Copyright unclear, I'll rewrite this in proper rust eventually, but for now I just want
        // something that works.

        // Add our side of the tunnel to the WAN interface, that's the CE addr
        run_cmd!(ip -6 addr add $edge_addr dev $wan_dev)?;
        // Add the tunnel
        run_cmd!(ip -6 tunnel add $tun_dev mode ip4ip6 remote $br_addr local $edge_addr dev $wan_dev encaplimit none)?;
        // TODO: calc mtu from WAN, not from hard coding it
        run_cmd!(ip link set dev $tun_dev mtu 1460)?;
        run_cmd!(ip link set dev $tun_dev up)?;

        // all ipv4 goes over the tunnel
        run_cmd!(ip route del default)?;
        run_cmd!(ip route add default dev $tun_dev)?;

        // and now nat rules
        // Major TODO, we should not be flushing nat, we should be creating a chain and jumping to
        // it and playing nice with other iptables users.
        run_cmd!(iptables -t nat -F)?;
        let num_ranges = port_ranges.len();

        // randomly snat to one of the port ranges externally based on our internally chosen sport.
        // This gives us consistent routing, and also a reasonably even distribution.
        let mark_base = 0x10;
        run_cmd!(iptables -t mangle -I PREROUTING -j HMARK --hmark-tuple sport --hmark-mod $num_ranges --hmark-offset $mark_base --hmark-rnd 4)?;
        for (i, (start, end)) in port_ranges.iter().enumerate() {
            let mark = mark_base + i; // arbitrary
            for proto in ["icmp", "tcp", "udp"] {
                run_cmd!(iptables -t nat -A POSTROUTING -p $proto -o $tun_dev -m mark --mark $mark -j SNAT --to $ipv4_addr:$start-$end)?;
            }
        }
        run_cmd!(iptables -t mangle -o $tun_dev --insert FORWARD 1 -p tcp --tcp-flags SYN,RST SYN -m tcpmss --mss 1400:65495 -j TCPMSS --clamp-mss-to-pmtu)?;

        if self.add_ipv4_wan {
            run_cmd!(ip addr add $ipv4_addr dev $wan_dev)?;
        }

        Ok(())
    }
}

#[derive(Subcommand)]
enum Subcommands {
    Calculate(Calculate),
    SetupLinux(SetupLinux),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.sub {
        Subcommands::Calculate(c) => {
            let data = c.calculate()?;
            println!("{data}");
            Ok(())
        }
        Subcommands::SetupLinux(s) => s.setup(),
    }
}
