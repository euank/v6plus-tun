use anyhow::bail;
use clap::{Parser, Subcommand};

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

#[derive(Subcommand)]
enum Subcommands {
    Calculate(Calculate),
    SetupLinux {
        #[arg(required = true)]
        addr: std::net::Ipv6Addr,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.sub {
        Subcommands::Calculate(c) => {
            let data = c.calculate()?;
            println!("{data}");
        }
        Subcommands::SetupLinux { addr: _ } => {
            unimplemented!("TODO");
        }
    }

    Ok(())
}
