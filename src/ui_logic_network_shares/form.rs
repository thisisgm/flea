//! The five protocol choices and their literal form fields; secrets are never part of a form.
use super::networkaddress::{self, Address};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Protocol {
    #[default]
    Smb,
    Sftp,
    Ftps,
    WebDav,
    Nfs,
}

pub const PROTOCOLS: [Protocol; 5] = [
    Protocol::Smb,
    Protocol::Sftp,
    Protocol::Ftps,
    Protocol::WebDav,
    Protocol::Nfs,
];

impl Protocol {
    pub fn key(self) -> &'static str {
        match self {
            Self::Smb => "network.smb",
            Self::Sftp => "network.sftp",
            Self::Ftps => "network.ftps",
            Self::WebDav => "network.webdav",
            Self::Nfs => "network.nfs",
        }
    }
    pub fn scheme(self, tls: bool) -> &'static str {
        match self {
            Self::Smb => "smb",
            Self::Sftp => "sftp",
            Self::Ftps if !tls => "ftp",
            Self::Ftps => "ftps",
            Self::WebDav if tls => "davs",
            Self::WebDav => "dav",
            Self::Nfs => "nfs",
        }
    }
    pub fn port(self, tls: bool) -> u16 {
        if self == Self::Ftps {
            21
        } else {
            networkaddress::protocol(self.scheme(tls)).map_or(0, |(_, port)| port)
        }
    }
    pub fn credentials(self) -> bool {
        self != Self::Nfs
    }
    pub fn tls_control(self) -> bool {
        matches!(self, Self::Ftps | Self::WebDav)
    }
    pub fn path_key(self) -> &'static str {
        match self {
            Self::Smb => "network.share",
            Self::Nfs => "network.export",
            _ => "network.path",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub protocol: Protocol,
    pub tls: bool,
    pub label: String,
    pub host: String,
    pub port: String,
    pub path: String,
    pub domain: String,
    pub user: String,
    pub original: Option<Address>,
}

impl Default for Form {
    fn default() -> Self {
        Self {
            protocol: Protocol::Smb,
            tls: true,
            label: String::new(),
            host: String::new(),
            port: "445".into(),
            path: String::new(),
            domain: String::new(),
            user: String::new(),
            original: None,
        }
    }
}

impl Form {
    pub fn saved_address(uri: &str) -> Result<Address, &'static str> {
        if super::protocols::scheme_of(uri) != "ftp" {
            return networkaddress::parse(uri);
        }
        let tail = uri.split_once("://").map_or("", |(_, rest)| rest);
        let mut address = networkaddress::parse(&format!("ftps://{tail}"))?;
        address.scheme = "ftp".into();
        address.uri = address.uri.replacen("ftps://", "ftp://", 1);
        Ok(address)
    }
    pub fn pick(&mut self, protocol: Protocol) {
        self.protocol = protocol;
        self.port = protocol.port(self.tls).to_string();
    }
    pub fn toggle_tls(&mut self) {
        self.tls = !self.tls;
        self.port = self.protocol.port(self.tls).to_string();
    }
    pub fn uri(&self) -> Option<String> {
        if let Some(address) = &self.original {
            if self.protocol.scheme(self.tls) == address.scheme
                && self.host == address.host
                && self.port == address.port.to_string()
                && self.path == address.path.trim_start_matches('/')
                && self.user == address.user
                && self.domain == address.domain
            {
                return Some(address.uri.clone());
            }
        }
        if !networkaddress::valid_host(self.host.trim()) {
            return None;
        }
        let scheme = self.protocol.scheme(self.tls);
        let port = self.port.trim();
        let port = if port.is_empty() {
            self.protocol.port(self.tls)
        } else {
            networkaddress::decimal_port(port)?
        };
        let user = if self.protocol.credentials() {
            self.user.trim()
        } else {
            ""
        };
        let domain = if self.protocol == Protocol::Smb {
            self.domain.trim()
        } else {
            ""
        };
        let info = if user.is_empty() {
            String::new()
        } else {
            let prefix = if domain.is_empty() {
                String::new()
            } else {
                format!("{};", networkaddress::encode(domain, false))
            };
            format!("{}{}@", prefix, networkaddress::encode(user, false))
        };
        let suffix = if port == self.protocol.port(self.tls) {
            String::new()
        } else {
            format!(":{port}")
        };
        let path = self.path.trim();
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let uri = format!(
            "{scheme}://{info}{}{suffix}{}",
            self.host.trim(),
            networkaddress::encode(&path, true)
        );
        if scheme == "ftp" {
            let parsed = networkaddress::parse(&uri.replacen("ftp://", "ftps://", 1)).ok()?;
            Some(parsed.uri.replacen("ftps://", "ftp://", 1))
        } else {
            Some(networkaddress::parse(&uri).ok()?.uri)
        }
    }
    pub fn name(&self) -> String {
        if !self.label.trim().is_empty() {
            return self.label.trim().to_string();
        }
        let path = self.path.trim_end_matches('/');
        let leaf = path.rsplit('/').next().unwrap_or("");
        if leaf.is_empty() {
            self.host.trim().to_string()
        } else {
            leaf.to_string()
        }
    }
    pub fn load(address: &Address, label: &str) -> Self {
        let mut form = Self {
            label: label.into(),
            user: address.user.clone(),
            domain: address.domain.clone(),
            ..Self::default()
        };
        form.suggest(address);
        form.original = Some(address.clone());
        form
    }
    pub fn suggest(&mut self, address: &Address) {
        self.original = None;
        self.protocol = match address.scheme.as_str() {
            "sftp" => Protocol::Sftp,
            "ftp" | "ftps" => Protocol::Ftps,
            "dav" | "davs" => Protocol::WebDav,
            "nfs" => Protocol::Nfs,
            _ => Protocol::Smb,
        };
        self.tls = !matches!(address.scheme.as_str(), "ftp" | "dav");
        self.host.clone_from(&address.host);
        self.port = address.port.to_string();
        self.path = address.path.trim_start_matches('/').into();
    }
}
