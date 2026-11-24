//! `vless://` share links and the Xray-core config made from one.
//!
//! vless://<uuid>@<host>:<port>?type=tcp&security=reality&flow=xtls-rprx-vision&sni=..&fp=chrome
//!        &pbk=<public key>&sid=<short id>&spx=<spider x>#<name>
//! Also security=tls and type=ws (path, host): a server behind Cloudflare.

use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct Server {
    pub name: String,
    pub link: String,
    pub id: String,
    pub host: String,
    pub port: u16,
    params: Vec<(String, String)>,
}

impl Server {
    pub fn parse(link: &str) -> Result<Server, String> {
        let link = link.trim();
        let rest = link
            .strip_prefix("vless://")
            .or_else(|| link.strip_prefix("VLESS://"))
            .ok_or("Not a vless:// link")?;
        let (rest, name) = rest.split_once('#').map_or((rest, ""), |(r, n)| (r, n));
        let (address, query) = rest.split_once('?').map_or((rest, ""), |(a, q)| (a, q));
        let (id, host_port) = address.rsplit_once('@').ok_or("The link has no user id")?;
        let (host, port) = match host_port.strip_prefix('[') {
            // [IPv6]:port
            Some(v6) => v6.split_once("]:").ok_or("Bad IPv6 address")?,
            None => host_port.rsplit_once(':').ok_or("The link has no port")?,
        };
        let port: u16 = port.trim_end_matches('/').parse().map_err(|_| "Bad port")?;
        let params = query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .map(|(k, v)| (k.to_owned(), decode(v)))
            .collect();
        let id = decode(id);
        let host = host.to_owned();
        if id.is_empty() || host.is_empty() {
            return Err("The link has no server".into());
        }
        let name = match decode(name).trim() {
            "" => host.clone(),
            n => n.to_owned(),
        };
        Ok(Server { name, link: link.to_owned(), id, host, port, params })
    }

    /// The same account on the same server, whatever its name and settings in the link.
    pub fn is_same_server(&self, other: &Server) -> bool {
        self.id == other.id && self.port == other.port && self.host.eq_ignore_ascii_case(&other.host)
    }

    fn param(&self, key: &str) -> &str {
        self.params.iter().find(|(k, _)| k == key).map_or("", |(_, v)| v.as_str())
    }

    /// What the card shows under the name: "REALITY · Vision", "TLS · WebSocket".
    pub fn kind(&self, no_encryption: &str) -> String {
        let security = match self.param("security") {
            "reality" => "REALITY",
            "tls" => "TLS",
            _ => no_encryption,
        };
        let transport = match (self.param("type"), self.param("flow")) {
            ("ws", _) => "WebSocket",
            (_, f) if f.starts_with("xtls-rprx-vision") => "Vision",
            _ => "TCP",
        };
        format!("{security} · {transport}")
    }

    /// Xray-core's config: the VPN's TUN in, this server out.
    pub fn xray_config(&self) -> String {
        let network = match self.param("type") {
            "" | "raw" => "tcp",
            other => other,
        };
        let security = match self.param("security") {
            "" => "none",
            other => other,
        };
        let fingerprint = match self.param("fp") {
            "" => "chrome",
            other => other,
        };
        let sni = match self.param("sni") {
            "" => self.param("host"),
            other => other,
        };
        let mut stream = json!({ "network": network, "security": security });
        match security {
            "reality" => {
                stream["realitySettings"] = json!({
                    "serverName": sni,
                    "fingerprint": fingerprint,
                    "publicKey": self.param("pbk"),
                    "shortId": self.param("sid"),
                    "spiderX": self.param("spx"),
                })
            }
            "tls" => {
                let mut tls = json!({ "serverName": sni, "fingerprint": fingerprint });
                if !self.param("alpn").is_empty() {
                    tls["alpn"] = Value::from(self.param("alpn").split(',').collect::<Vec<_>>());
                }
                stream["tlsSettings"] = tls;
            }
            _ => {}
        }
        if network == "ws" {
            stream["wsSettings"] = json!({ "path": self.param("path"), "host": self.param("host") });
        }
        let user = json!({ "id": self.id, "encryption": "none", "flow": self.param("flow") });
        json!({
            "log": { "loglevel": "warning" },
            "inbounds": [{
                "tag": "tun",
                "port": 0,
                "protocol": "tun",
                "settings": tun_settings(),
                "sniffing": { "enabled": true, "destOverride": ["http", "tls", "quic"] }
            }],
            "outbounds": [
                {
                    "tag": "proxy",
                    "protocol": "vless",
                    "settings": { "vnext": [{ "address": self.host, "port": self.port, "users": [user] }] },
                    "streamSettings": stream
                },
                { "tag": "direct", "protocol": "freedom" },
                { "tag": "block", "protocol": "blackhole" }
            ]
        })
        .to_string()
    }
}

/// The TUN inbound's settings. Phones and Macs hand Xray the system VPN's file descriptor; on
/// Windows Xray makes the adapter itself (Wintun) and sets the routes, the DNS and the leak filters.
fn tun_settings() -> Value {
    #[cfg(windows)]
    return json!({
        "name": "Net Connect",
        "MTU": 1500,
        "gateway": ["10.10.14.1/30", "fd66:7665:7373::1/126"],
        "dns": ["1.1.1.1"],
        "autoSystemRoutingTable": ["0.0.0.0/0", "::/0"],
        "autoSystemWfpBlockLeak": ["dns", "misconfigtun"]
    });
    #[cfg(not(windows))]
    json!({ "name": "xray0", "MTU": 1500 })
}

/// Percent-decoding (%2F, + stays +).
fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reality_link() {
        let s = Server::parse("vless://abc-1@1.2.3.4:443?type=tcp&security=reality&flow=xtls-rprx-vision&sni=github.com&fp=chrome&pbk=KEY&sid=0a1b&spx=%2F#My%20server").unwrap();
        assert_eq!((s.name.as_str(), s.host.as_str(), s.port, s.id.as_str()), ("My server", "1.2.3.4", 443, "abc-1"));
        assert_eq!(s.kind("none"), "REALITY · Vision");
        let c: Value = serde_json::from_str(&s.xray_config()).unwrap();
        let r = &c["outbounds"][0]["streamSettings"]["realitySettings"];
        assert_eq!((r["publicKey"].as_str(), r["spiderX"].as_str()), (Some("KEY"), Some("/")));
        assert!(Server::parse("vmess://x").is_err());
    }
}
