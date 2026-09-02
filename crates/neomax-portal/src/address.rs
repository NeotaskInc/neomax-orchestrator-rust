use std::fmt::{Display, Formatter};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::str::FromStr;

use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalBind(SocketAddr);

impl LocalBind {
    pub const DEFAULT_PORT: u16 = 8787;

    pub fn new(ip: IpAddr, port: u16) -> Result<Self> {
        if !ip.is_loopback() {
            bail!("portal must bind to a loopback address; refusing {ip}");
        }
        if port == 0 {
            bail!("portal port must be between 1 and 65535");
        }
        Ok(Self(SocketAddr::new(ip, port)))
    }

    pub fn loopback(port: u16) -> Self {
        Self(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
    }

    pub fn socket_addr(self) -> SocketAddr {
        self.0
    }

    pub fn port(self) -> u16 {
        self.0.port()
    }
}

impl Default for LocalBind {
    fn default() -> Self {
        Self::loopback(Self::DEFAULT_PORT)
    }
}

impl Display for LocalBind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

impl FromStr for LocalBind {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            bail!("portal bind address is empty");
        }

        // A bare port is intentionally accepted for parity with the Python launcher.
        if let Ok(port) = value.parse::<u16>() {
            return Self::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        }
        let socket: SocketAddr = value
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid portal bind address: {value}"))?;
        Self::new(socket.ip(), socket.port())
    }
}

pub fn local_url(bind: LocalBind) -> String {
    match bind.socket_addr().ip() {
        IpAddr::V6(Ipv6Addr::LOCALHOST) => format!("http://[::1]:{}", bind.port()),
        ip => format!("http://{ip}:{}", bind.port()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_localhost_and_reference_port() {
        let bind = LocalBind::default();
        assert_eq!(bind.socket_addr(), "127.0.0.1:8787".parse().unwrap());
        assert_eq!(local_url(bind), "http://127.0.0.1:8787");
    }

    #[test]
    fn accepts_bare_port_and_ipv6_loopback() {
        assert_eq!("9000".parse::<LocalBind>().unwrap().port(), 9000);
        let bind = "[::1]:9001".parse::<LocalBind>().unwrap();
        assert_eq!(local_url(bind), "http://[::1]:9001");
    }

    #[test]
    fn rejects_public_bind_addresses_and_zero_port() {
        assert!("0.0.0.0:8787".parse::<LocalBind>().is_err());
        assert!("127.0.0.1:0".parse::<LocalBind>().is_err());
    }
}
