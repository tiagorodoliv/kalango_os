//! Configurações → Rede: IP fixo das conexões e VPN (OpenVPN e WireGuard),
//! como no Ubuntu, tudo pelo NetworkManager.
//!
//! - **IP**: `nmcli connection modify <nome> ipv4.method manual|auto ...` e,
//!   se a conexão estiver ativa, `connection up` para valer na hora.
//! - **OpenVPN**: arquivo `.ovpn` importado com `nmcli connection import`
//!   (não liga a VPN). O formulário manual gera um `.ovpn` em
//!   `~/.config/kalango/vpn/` e importa. Usuário e senha ficam na conexão.
//! - **WireGuard**: o `nmcli connection import` liga a VPN na hora (e com
//!   AllowedIPs 0.0.0.0/0 derrubaria a rede), e o `nmcli` não edita os pares.
//!   Por isso o `.conf` (ou o formulário) é lido aqui e a conexão é criada
//!   direto no NetworkManager pelo D-Bus (`gdbus`), desligada e sem conexão
//!   automática; o DNS vai depois pelo `nmcli`.
//! - Escolher arquivo: `zenity --file-selection`.

use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};

use crate::settings::device::split_terse;
use crate::common::i18n::tr;
use crate::{AppWindow, Device, InfoItem, Mode, NetConnection, VpnConnection};

/// Intervalo entre as leituras da lista de conexões.
const INTERVAL: Duration = Duration::from_secs(3);

// ---------------------------------------------------------------------------
// Validação de endereços (funções puras, testadas)
// ---------------------------------------------------------------------------

/// Prefixo da rede a partir de "24" ou de uma máscara "255.255.255.0".
pub fn parse_prefix(text: &str) -> Option<u8> {
    let text = text.trim().trim_start_matches('/');
    if let Ok(n) = text.parse::<u8>() {
        return (n <= 32).then_some(n);
    }
    let mask = u32::from(text.parse::<Ipv4Addr>().ok()?);
    // Máscara válida: uns seguidos de zeros.
    let ones = mask.leading_ones();
    (mask.checked_shl(ones).unwrap_or(0) == 0).then_some(ones as u8)
}

/// "192.168.1.50/24" → endereço e prefixo (sem prefixo: `default_prefix`).
pub fn parse_cidr(text: &str, default_prefix: Option<u8>) -> Option<(IpAddr, u8)> {
    let text = text.trim();
    let (addr, prefix) = match text.split_once('/') {
        Some((a, p)) => (a, Some(p.trim().parse::<u8>().ok()?)),
        None => (text, default_prefix),
    };
    let addr: IpAddr = addr.trim().parse().ok()?;
    let max = if addr.is_ipv4() { 32 } else { 128 };
    let prefix = prefix.unwrap_or(max);
    (prefix <= max).then_some((addr, prefix))
}

/// Lista de endereços separados por vírgula ou espaço (DNS).
pub fn parse_ip_list(text: &str) -> Option<Vec<IpAddr>> {
    text.split([',', ' ', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.parse().ok())
        .collect()
}

/// Configuração de IPv4 de uma conexão, lida do `nmcli`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IpSettings {
    pub manual: bool,
    pub address: String,
    pub prefix: String,
    pub gateway: String,
    pub dns: String,
    /// Endereço em uso agora (conexão ativa).
    pub current: String,
}

/// Lê `nmcli -t -f ipv4.method,ipv4.addresses,ipv4.gateway,ipv4.dns,IP4.ADDRESS connection show <nome>`.
pub fn parse_ip_settings(text: &str) -> IpSettings {
    let mut s = IpSettings::default();
    for line in text.lines() {
        let f = split_terse(line);
        if f.len() < 2 {
            continue;
        }
        let value = f[1..].join(":");
        match f[0].as_str() {
            "ipv4.method" => s.manual = value == "manual",
            "ipv4.addresses" => {
                let first = value.split(',').next().unwrap_or("").trim();
                if let Some((addr, prefix)) = first.split_once('/') {
                    s.address = addr.to_string();
                    s.prefix = prefix.to_string();
                }
            }
            "ipv4.gateway" => s.gateway = value.trim().to_string(),
            "ipv4.dns" => s.dns = value.split(',').map(str::trim).collect::<Vec<_>>().join(", "),
            k if k.starts_with("IP4.ADDRESS") && s.current.is_empty() => s.current = value.trim().to_string(),
            _ => {}
        }
    }
    s
}

/// Argumentos do `nmcli connection modify` para o IPv4 (`Err` = mensagem).
pub fn ip_modify_args(manual: bool, address: &str, mask: &str, gateway: &str, dns: &str) -> Result<Vec<String>, String> {
    let mut args = vec!["ipv4.method".to_string()];
    if !manual {
        args.extend(["auto", "ipv4.addresses", "", "ipv4.gateway", "", "ipv4.dns", ""].map(String::from));
        return Ok(args);
    }
    let addr: Ipv4Addr = address
        .trim()
        .parse()
        .map_err(|_| tr("Invalid IP address.", "Endereço IP inválido.").to_string())?;
    let prefix = parse_prefix(mask).ok_or_else(|| tr("Invalid subnet mask.", "Máscara de rede inválida.").to_string())?;
    args.extend(["manual".to_string(), "ipv4.addresses".to_string(), format!("{addr}/{prefix}")]);
    let gateway = gateway.trim();
    if gateway.is_empty() {
        args.extend(["ipv4.gateway".to_string(), String::new()]);
    } else {
        let gw: Ipv4Addr = gateway.parse().map_err(|_| tr("Invalid gateway.", "Gateway inválido.").to_string())?;
        args.extend(["ipv4.gateway".to_string(), gw.to_string()]);
    }
    let dns = parse_ip_list(dns).ok_or_else(|| tr("Invalid DNS server.", "Servidor DNS inválido.").to_string())?;
    args.extend([
        "ipv4.dns".to_string(),
        dns.iter().map(IpAddr::to_string).collect::<Vec<_>>().join(" "),
    ]);
    Ok(args)
}

// ---------------------------------------------------------------------------
// WireGuard: leitura do .conf e criação pelo D-Bus
// ---------------------------------------------------------------------------

/// Um par (servidor) do WireGuard.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WgPeer {
    pub public_key: String,
    pub preshared_key: String,
    pub endpoint: String,
    pub allowed_ips: Vec<String>,
    pub keepalive: Option<u32>,
}

/// Uma configuração do WireGuard (formato do `wg-quick`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WgConfig {
    pub private_key: String,
    /// Endereços com prefixo ("10.0.0.2/24", "fd00::2/64").
    pub addresses: Vec<(IpAddr, u8)>,
    pub dns: Vec<IpAddr>,
    pub listen_port: Option<u32>,
    pub mtu: Option<u32>,
    pub peers: Vec<WgPeer>,
}

/// Chave do WireGuard: 32 bytes em base64 (44 caracteres terminando em "=").
pub fn valid_wg_key(key: &str) -> bool {
    key.len() == 44
        && key.ends_with('=')
        && key[..43].chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/')
}

/// Endpoint "host:porta" ou "[ipv6]:porta" (só caracteres seguros).
pub fn valid_endpoint(ep: &str) -> bool {
    let Some((host, port)) = ep.rsplit_once(':') else { return false };
    !host.is_empty()
        && port.parse::<u16>().is_ok()
        && host.chars().all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
}

/// Lê um arquivo de configuração do WireGuard (`[Interface]` e `[Peer]`).
pub fn parse_wg_conf(text: &str) -> Result<WgConfig, String> {
    let bad = |what: &str| format!("{} {what}", tr("Invalid WireGuard file:", "Arquivo do WireGuard inválido:"));
    let mut cfg = WgConfig::default();
    let mut section = "";
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case("[interface]") {
            section = "interface";
            continue;
        }
        if line.eq_ignore_ascii_case("[peer]") {
            section = "peer";
            cfg.peers.push(WgPeer::default());
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        match (section, key.as_str()) {
            ("interface", "privatekey") => cfg.private_key = value.to_string(),
            ("interface", "address") => {
                for a in value.split(',') {
                    cfg.addresses.push(parse_cidr(a, None).ok_or_else(|| bad("Address"))?);
                }
            }
            ("interface", "dns") => {
                // O DNS pode ter domínios de busca misturados: só os IPs contam.
                cfg.dns.extend(value.split(',').filter_map(|d| d.trim().parse::<IpAddr>().ok()));
            }
            ("interface", "listenport") => cfg.listen_port = Some(value.parse().map_err(|_| bad("ListenPort"))?),
            ("interface", "mtu") => cfg.mtu = Some(value.parse().map_err(|_| bad("MTU"))?),
            ("peer", k) => {
                let peer = cfg.peers.last_mut().expect("[Peer] já criado");
                match k {
                    "publickey" => peer.public_key = value.to_string(),
                    "presharedkey" => peer.preshared_key = value.to_string(),
                    "endpoint" => peer.endpoint = value.to_string(),
                    "allowedips" => {
                        for a in value.split(',').map(str::trim).filter(|a| !a.is_empty()) {
                            let (ip, prefix) = parse_cidr(a, None).ok_or_else(|| bad("AllowedIPs"))?;
                            peer.allowed_ips.push(format!("{ip}/{prefix}"));
                        }
                    }
                    "persistentkeepalive" => peer.keepalive = value.parse().ok(),
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if !valid_wg_key(&cfg.private_key) {
        return Err(bad("PrivateKey"));
    }
    if cfg.addresses.is_empty() {
        return Err(bad("Address"));
    }
    if cfg.peers.is_empty() {
        return Err(bad("[Peer]"));
    }
    for p in &cfg.peers {
        if !valid_wg_key(&p.public_key) || (!p.preshared_key.is_empty() && !valid_wg_key(&p.preshared_key)) {
            return Err(bad("PublicKey"));
        }
        if !p.endpoint.is_empty() && !valid_endpoint(&p.endpoint) {
            return Err(bad("Endpoint"));
        }
    }
    Ok(cfg)
}

/// Nome de interface válido no Linux (até 15 caracteres, sem espaços).
pub fn interface_name(name: &str) -> String {
    let mut s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    s.truncate(15);
    let s = s.trim_matches('-').to_string();
    if s.is_empty() { "wg0".to_string() } else { s }
}

/// Texto GVariant de uma string (entre aspas, com escape).
fn gv_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Argumento do `AddConnection` do NetworkManager para uma conexão WireGuard
/// desligada e sem conexão automática.
pub fn wg_connection_gvariant(name: &str, cfg: &WgConfig) -> String {
    let addr_data = |v4: bool| -> Vec<String> {
        cfg.addresses
            .iter()
            .filter(|(ip, _)| ip.is_ipv4() == v4)
            .map(|(ip, p)| format!("{{\"address\": <{}>, \"prefix\": <uint32 {p}>}}", gv_str(&ip.to_string())))
            .collect()
    };
    let peers: Vec<String> = cfg
        .peers
        .iter()
        .map(|p| {
            let mut fields = vec![format!("\"public-key\": <{}>", gv_str(&p.public_key))];
            if !p.preshared_key.is_empty() {
                fields.push(format!("\"preshared-key\": <{}>", gv_str(&p.preshared_key)));
                fields.push("\"preshared-key-flags\": <uint32 0>".into());
            }
            if !p.endpoint.is_empty() {
                fields.push(format!("\"endpoint\": <{}>", gv_str(&p.endpoint)));
            }
            let ips: Vec<String> = p.allowed_ips.iter().map(|a| gv_str(a)).collect();
            fields.push(format!("\"allowed-ips\": <[{}]>", if ips.is_empty() { "@as ".into() } else { ips.join(", ") }));
            if let Some(k) = p.keepalive {
                fields.push(format!("\"persistent-keepalive\": <uint32 {k}>"));
            }
            format!("{{{}}}", fields.join(", "))
        })
        .collect();
    let mut wg = vec![
        format!("\"private-key\": <{}>", gv_str(&cfg.private_key)),
        "\"private-key-flags\": <uint32 0>".into(),
        format!("\"peers\": <[{}]>", peers.join(", ")),
    ];
    if let Some(port) = cfg.listen_port {
        wg.push(format!("\"listen-port\": <uint32 {port}>"));
    }
    if let Some(mtu) = cfg.mtu {
        wg.push(format!("\"mtu\": <uint32 {mtu}>"));
    }
    let v4 = addr_data(true);
    let v6 = addr_data(false);
    let ipv4 = if v4.is_empty() {
        "{\"method\": <\"disabled\">}".to_string()
    } else {
        format!("{{\"method\": <\"manual\">, \"address-data\": <[{}]>}}", v4.join(", "))
    };
    let ipv6 = if v6.is_empty() {
        "{\"method\": <\"disabled\">}".to_string()
    } else {
        format!("{{\"method\": <\"manual\">, \"address-data\": <[{}]>}}", v6.join(", "))
    };
    format!(
        "{{\"connection\": {{\"id\": <{}>, \"type\": <\"wireguard\">, \"interface-name\": <{}>, \"autoconnect\": <false>}}, \
         \"wireguard\": {{{}}}, \"ipv4\": {}, \"ipv6\": {}}}",
        gv_str(name),
        gv_str(&interface_name(name)),
        wg.join(", "),
        ipv4,
        ipv6
    )
}

// ---------------------------------------------------------------------------
// OpenVPN: arquivo gerado pelo formulário manual
// ---------------------------------------------------------------------------

/// Tipo de autenticação do OpenVPN no formulário.
pub const AUTH_CERTS: i32 = 0;
pub const AUTH_PASSWORD: i32 = 1;
pub const AUTH_PASSWORD_CERTS: i32 = 2;

/// Texto do `.ovpn` do formulário manual (`Err` = mensagem).
pub fn build_ovpn(server: &str, port: &str, tcp: bool, auth: i32, ca: &str, cert: &str, key: &str) -> Result<String, String> {
    let server = server.trim();
    if server.is_empty() || !server.chars().all(|c| c.is_ascii_alphanumeric() || ".-:".contains(c)) {
        return Err(tr("Invalid server.", "Servidor inválido.").into());
    }
    let port: u16 = if port.trim().is_empty() {
        1194
    } else {
        port.trim().parse().map_err(|_| tr("Invalid port.", "Porta inválida.").to_string())?
    };
    let file_ok = |p: &str| !p.trim().is_empty() && !p.contains('\n') && std::path::Path::new(p.trim()).is_file();
    if !file_ok(ca) {
        return Err(tr("Choose the CA certificate.", "Escolha o certificado da CA.").into());
    }
    let mut text = format!(
        "client\ndev tun\nproto {}\nremote {server} {port}\nnobind\npersist-key\npersist-tun\nca {}\n",
        if tcp { "tcp" } else { "udp" },
        quote_path(ca.trim())
    );
    if auth != AUTH_PASSWORD {
        if !file_ok(cert) || !file_ok(key) {
            return Err(tr("Choose the user certificate and the private key.", "Escolha o certificado do usuário e a chave privada.").into());
        }
        text.push_str(&format!("cert {}\nkey {}\n", quote_path(cert.trim()), quote_path(key.trim())));
    }
    if auth != AUTH_CERTS {
        text.push_str("auth-user-pass\n");
    }
    Ok(text)
}

/// Caminho no `.ovpn` (entre aspas se tiver espaço).
fn quote_path(p: &str) -> String {
    if p.contains(' ') { format!("\"{p}\"") } else { p.to_string() }
}

// ---------------------------------------------------------------------------
// Conexões salvas
// ---------------------------------------------------------------------------

/// Detalhes de uma conexão para o painel "Detalhes" (como no Ubuntu).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ConnDetails {
    /// A conexão está em uso agora.
    pub active: bool,
    pub autoconnect: bool,
    /// `connection.metered` = yes.
    pub metered: bool,
    /// Wi-Fi com senha pessoal (WPA/WPA2/WPA3).
    pub secured: bool,
    /// Nome da rede Wi-Fi (pode ser diferente do nome da conexão).
    pub ssid: String,
    /// Interface em uso ("wlp2s0").
    pub device: String,
    pub ipv4: String,
    pub ipv6: String,
    pub gateway: String,
    pub dns: String,
    pub hwaddr: String,
    /// Velocidade do link ("1000 Mb/s", "270 Mbit/s").
    pub speed: String,
    /// Sinal do Wi-Fi, 0–100.
    pub signal: Option<u8>,
    /// Frequência do Wi-Fi em MHz.
    pub freq: Option<u32>,
    /// Segurança do Wi-Fi ("WPA2", "WPA3"...).
    pub security: String,
}

/// Lê `nmcli -t -f connection.autoconnect,connection.metered,GENERAL.STATE,
/// GENERAL.DEVICES,802-11-wireless.ssid,802-11-wireless-security.key-mgmt,
/// IP4.ADDRESS,IP4.GATEWAY,IP4.DNS,IP6.ADDRESS connection show id <nome>`.
pub fn parse_conn_details(text: &str) -> ConnDetails {
    let mut d = ConnDetails { autoconnect: true, ..Default::default() };
    let mut dns = Vec::new();
    // Melhor endereço IPv6 visto até agora e a "classe" dele (2 = global).
    let mut ipv6_link = String::new();
    let mut ipv6_rank = 0;
    for line in text.lines() {
        let f = split_terse(line);
        if f.len() < 2 {
            continue;
        }
        let value = f[1..].join(":").trim().to_string();
        // "IP4.DNS[2]" → "IP4.DNS"
        let key = f[0].split('[').next().unwrap_or("");
        match key {
            "connection.autoconnect" => d.autoconnect = value != "no",
            "connection.metered" => d.metered = value == "yes",
            "GENERAL.STATE" => d.active = value == "activated",
            "GENERAL.DEVICES" => d.device = value.split(',').next().unwrap_or("").to_string(),
            "802-11-wireless.ssid" => d.ssid = value,
            "802-11-wireless-security.key-mgmt" => d.secured = matches!(value.as_str(), "wpa-psk" | "sae" | "wpa-psk-sae"),
            "IP4.ADDRESS" if d.ipv4.is_empty() => d.ipv4 = value.split('/').next().unwrap_or("").to_string(),
            "IP4.GATEWAY" => d.gateway = value,
            "IP4.DNS" | "IP6.DNS" if !value.is_empty() => dns.push(value),
            // Prefere o endereço global ao local (fc00::/7) e ao de enlace (fe80::).
            "IP6.ADDRESS" => {
                let addr = value.split('/').next().unwrap_or("").to_string();
                let rank = match addr.get(..2).unwrap_or("") {
                    "fe" => 0,
                    "fc" | "fd" => 1,
                    _ => 2,
                };
                if ipv6_link.is_empty() || rank > ipv6_rank {
                    ipv6_link = addr;
                    ipv6_rank = rank;
                }
            }
            _ => {}
        }
    }
    d.ipv6 = ipv6_link;
    d.dns = dns.join(", ");
    if !d.active {
        // Sem conexão em uso, os endereços que sobraram são velhos.
        d.ipv4.clear();
        d.ipv6.clear();
        d.gateway.clear();
        d.dns.clear();
    }
    d
}

/// Completa com `nmcli -t -f GENERAL.HWADDR,CAPABILITIES.SPEED device show <dev>`.
pub fn apply_device_details(d: &mut ConnDetails, text: &str) {
    for line in text.lines() {
        let f = split_terse(line);
        if f.len() < 2 {
            continue;
        }
        let value = f[1..].join(":").trim().to_string();
        match f[0].as_str() {
            "GENERAL.HWADDR" => d.hwaddr = value,
            "CAPABILITIES.SPEED" if value.chars().next().is_some_and(|c| c.is_ascii_digit()) => d.speed = value,
            _ => {}
        }
    }
}

/// Completa com a rede em uso de `nmcli -t -f IN-USE,SSID,SIGNAL,FREQ,RATE,SECURITY
/// device wifi list --rescan no` (sinal, frequência, velocidade, segurança).
pub fn apply_wifi_details(d: &mut ConnDetails, text: &str) {
    for line in text.lines() {
        let f = split_terse(line);
        if f.len() < 6 || f[0] != "*" || (!d.ssid.is_empty() && f[1] != d.ssid) {
            continue;
        }
        d.signal = f[2].trim().parse().ok();
        d.freq = f[3].split_whitespace().next().and_then(|v| v.parse().ok());
        d.speed = f[4].trim().to_string();
        d.security = f[5].trim().to_string();
        return;
    }
}

/// Faixa do Wi-Fi pela frequência em MHz.
pub fn wifi_band(mhz: u32) -> &'static str {
    match mhz {
        0..=3000 => tr("2.4 GHz", "2,4 GHz"),
        3001..=5924 => "5 GHz",
        _ => "6 GHz",
    }
}

/// Linhas do painel (rótulo, valor), já no idioma, só as que têm valor.
pub fn detail_rows(d: &ConnDetails) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if !d.active {
        return rows;
    }
    let mut push = |label: &str, value: String| {
        if !value.is_empty() {
            rows.push((label.to_string(), value));
        }
    };
    if let Some(s) = d.signal {
        let word = match s {
            75.. => tr("Excellent", "Excelente"),
            50..=74 => tr("Good", "Bom"),
            30..=49 => tr("OK", "Razoável"),
            _ => tr("Weak", "Fraco"),
        };
        push(tr("Signal", "Sinal"), format!("{word} ({s}%)"));
    }
    push(tr("Link speed", "Velocidade"), d.speed.replace("Mbit/s", "Mb/s"));
    if !d.ssid.is_empty() {
        let security = if d.security.is_empty() || d.security == "--" {
            tr("None (open network)", "Nenhuma (rede aberta)").to_string()
        } else {
            d.security.replace(' ', " / ")
        };
        push(tr("Security", "Segurança"), security);
    }
    if let Some(f) = d.freq {
        push(tr("Band", "Faixa"), wifi_band(f).to_string());
    }
    push(tr("IPv4 address", "Endereço IPv4"), d.ipv4.clone());
    push(tr("IPv6 address", "Endereço IPv6"), d.ipv6.clone());
    push(tr("Router", "Roteador"), d.gateway.clone());
    push("DNS", d.dns.clone());
    push(tr("Hardware address", "Endereço físico (MAC)"), d.hwaddr.clone());
    rows
}

/// Conexão salva: nome, tipo e se está ativa.
pub type SavedConnection = (String, String, bool);

/// Conexões de `nmcli -t -f NAME,TYPE,ACTIVE connection show`: as de rede
/// (cabo e Wi-Fi) e as VPNs.
pub fn parse_connections(text: &str) -> (Vec<SavedConnection>, Vec<SavedConnection>) {
    let mut nets = Vec::new();
    let mut vpns = Vec::new();
    for line in text.lines() {
        let f = split_terse(line);
        if f.len() < 3 {
            continue;
        }
        let active = f[2] == "yes";
        match f[1].as_str() {
            "802-11-wireless" => nets.push((f[0].clone(), "wifi".to_string(), active)),
            "802-3-ethernet" => nets.push((f[0].clone(), "ethernet".to_string(), active)),
            "vpn" => vpns.push((f[0].clone(), "OpenVPN".to_string(), active)),
            "wireguard" => vpns.push((f[0].clone(), "WireGuard".to_string(), active)),
            _ => {}
        }
    }
    (nets, vpns)
}

/// Roda o `nmcli` (em inglês) e devolve a saída ou a primeira linha do erro.
fn nm(args: &[&str]) -> Result<String, String> {
    run("nmcli", args)
}

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .env("LC_ALL", "C")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().trim_start_matches("Error: ").to_string())
    }
}

/// Roda um programa lendo a entrada de `input` (para o `wg pubkey`).
fn run_with_input(program: &str, args: &[&str], input: &str) -> Option<String> {
    use std::io::Write;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(input.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Cria a conexão WireGuard (desligada) e põe o DNS.
fn create_wireguard(name: &str, cfg: &WgConfig) -> Result<(), String> {
    run(
        "gdbus",
        &[
            "call",
            "--system",
            "--dest",
            "org.freedesktop.NetworkManager",
            "--object-path",
            "/org/freedesktop/NetworkManager/Settings",
            "--method",
            "org.freedesktop.NetworkManager.Settings.AddConnection",
            &wg_connection_gvariant(name, cfg),
        ],
    )?;
    if !cfg.dns.is_empty() {
        let v4: Vec<String> = cfg.dns.iter().filter(|d| d.is_ipv4()).map(IpAddr::to_string).collect();
        let v6: Vec<String> = cfg.dns.iter().filter(|d| d.is_ipv6()).map(IpAddr::to_string).collect();
        if !v4.is_empty() {
            nm(&["connection", "modify", "id", name, "ipv4.dns", &v4.join(" ")])?;
        }
        if !v6.is_empty() && cfg.addresses.iter().any(|(ip, _)| ip.is_ipv6()) {
            nm(&["connection", "modify", "id", name, "ipv6.dns", &v6.join(" ")])?;
        }
    }
    Ok(())
}

/// Importa um `.ovpn`; devolve o nome da conexão criada e se ela pede
/// usuário e senha.
fn import_openvpn(path: &str) -> Result<(String, bool), String> {
    let out = nm(&["connection", "import", "type", "openvpn", "file", path])?;
    // "Connection 'nome' (uuid) successfully added."
    let name = out
        .split('\'')
        .nth(1)
        .map(str::to_string)
        .ok_or_else(|| tr("The VPN was not imported.", "A VPN não foi importada.").to_string())?;
    let data = nm(&["-g", "vpn.data", "connection", "show", "id", &name]).unwrap_or_default();
    let needs_login = data.contains("connection-type = password");
    Ok((name, needs_login))
}

/// Grava usuário e senha numa VPN OpenVPN.
fn set_login(name: &str, user: &str, password: &str) -> Result<(), String> {
    nm(&[
        "connection",
        "modify",
        "id",
        name,
        "+vpn.data",
        &format!("username={}", user.trim()),
        "+vpn.data",
        "password-flags=0",
        "vpn.secrets",
        &format!("password={password}"),
    ])
    .map(|_| ())
}

/// Pasta dos `.ovpn` gerados pelo formulário.
fn vpn_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/kalango/vpn"))
}

// ---------------------------------------------------------------------------
// Ligação com a UI
// ---------------------------------------------------------------------------

/// Faz `work` numa thread e entrega a mensagem (vazia = deu certo) na UI.
fn background(weak: &Weak<AppWindow>, work: impl FnOnce() -> Result<String, String> + Send + 'static, done: impl FnOnce(&Device, Result<String, String>) + Send + 'static) {
    let weak = weak.clone();
    std::thread::spawn(move || {
        let result = work();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                let d = ui.global::<Device>();
                done(&d, result);
                refresh(&ui);
            }
        });
    });
}

/// Relê as conexões salvas (rápido: um `nmcli`).
fn refresh(ui: &AppWindow) {
    let d = ui.global::<Device>();
    let Ok(text) = nm(&["-t", "-f", "NAME,TYPE,ACTIVE", "connection", "show"]) else {
        return;
    };
    let (nets, vpns) = parse_connections(&text);
    let nets: Vec<NetConnection> = nets
        .into_iter()
        .map(|(name, kind, active)| NetConnection { name: name.into(), kind: kind.into(), active })
        .collect();
    let vpns: Vec<VpnConnection> = vpns
        .into_iter()
        .map(|(name, kind, active)| VpnConnection { name: name.into(), kind: kind.into(), active })
        .collect();
    d.set_connections(ModelRc::from(Rc::new(VecModel::from(nets))));
    d.set_vpns(ModelRc::from(Rc::new(VecModel::from(vpns))));
}

/// Lê os detalhes de uma conexão (três `nmcli` rápidos; sem procurar redes).
fn read_details(name: &str) -> ConnDetails {
    let text = nm(&[
        "-t",
        "-f",
        "connection.autoconnect,connection.metered,GENERAL.STATE,GENERAL.DEVICES,802-11-wireless.ssid,\
         802-11-wireless-security.key-mgmt,IP4.ADDRESS,IP4.GATEWAY,IP4.DNS,IP6.ADDRESS,IP6.DNS",
        "connection",
        "show",
        "id",
        name,
    ])
    .unwrap_or_default();
    let mut d = parse_conn_details(&text);
    if d.active && !d.device.is_empty() {
        let dev = nm(&["-t", "-f", "GENERAL.HWADDR,CAPABILITIES.SPEED", "device", "show", &d.device]).unwrap_or_default();
        apply_device_details(&mut d, &dev);
        if !d.ssid.is_empty() {
            let list = nm(&["-t", "-f", "IN-USE,SSID,SIGNAL,FREQ,RATE,SECURITY", "device", "wifi", "list", "--rescan", "no"])
                .unwrap_or_default();
            apply_wifi_details(&mut d, &list);
        }
    }
    d
}

/// Abre a janela de escolher arquivo (zenity) e devolve o caminho.
fn pick_file(title: &str, filter: Option<&str>) -> Option<String> {
    let mut args = vec!["--file-selection".to_string(), format!("--title={title}")];
    if let Some(f) = filter {
        args.push(format!("--file-filter={f}"));
        args.push(format!("--file-filter={}| *", tr("All files", "Todos os arquivos")));
    }
    let out = Command::new("zenity").args(&args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !path.is_empty()).then_some(path)
}

/// Liga IP fixo e VPN à página de Rede (sessão real, Configurações).
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if !(crate::shell::session::is_native() && mode == Mode::Settings) {
        return None;
    }
    let d = ui.global::<Device>();
    d.set_vpn_available(true);
    d.set_openvpn_available(
        fs::read_dir("/usr/lib/x86_64-linux-gnu/NetworkManager")
            .map(|r| r.flatten().any(|e| e.file_name().to_string_lossy().contains("openvpn")))
            .unwrap_or(false)
            || std::path::Path::new("/usr/lib/NetworkManager/VPN/nm-openvpn-service.name").exists(),
    );

    // --- Detalhes da conexão e IP ---
    let weak = ui.as_weak();
    d.on_ip_edit(move |name| {
        let Some(ui) = weak.upgrade() else { return };
        let d = ui.global::<Device>();
        let text = nm(&[
            "-t",
            "-f",
            "ipv4.method,ipv4.addresses,ipv4.gateway,ipv4.dns,IP4.ADDRESS",
            "connection",
            "show",
            "id",
            &name,
        ])
        .unwrap_or_default();
        let s = parse_ip_settings(&text);
        d.set_ip_manual(s.manual);
        d.set_ip_address(s.address.into());
        d.set_ip_mask(s.prefix.into());
        d.set_ip_gateway(s.gateway.into());
        d.set_ip_dns(s.dns.into());
        d.set_ip_current(s.current.into());
        d.set_ip_message(SharedString::new());

        let details = read_details(&name);
        let rows: Vec<InfoItem> = detail_rows(&details)
            .into_iter()
            .map(|(label, value)| InfoItem { label: label.into(), value: value.into() })
            .collect();
        d.set_conn_details(ModelRc::from(Rc::new(VecModel::from(rows))));
        d.set_conn_kind(if details.ssid.is_empty() { "ethernet" } else { "wifi" }.into());
        d.set_conn_active(details.active);
        d.set_conn_autoconnect(details.autoconnect);
        d.set_conn_metered(details.metered);
        d.set_conn_secured(details.secured);
        d.set_conn_password(SharedString::new());
        d.set_ip_for(name);
    });

    let weak = ui.as_weak();
    d.on_ip_apply(move |name, manual, address, mask, gateway, dns, autoconnect, metered| {
        let args = match ip_modify_args(manual, &address, &mask, &gateway, &dns) {
            Ok(a) => a,
            Err(msg) => {
                if let Some(ui) = weak.upgrade() {
                    ui.global::<Device>().set_ip_message(msg.into());
                }
                return;
            }
        };
        let name = name.to_string();
        background(
            &weak,
            move || {
                // O IP mudou? Só então a conexão é ativada de novo (derruba a
                // rede por um instante); as outras opções valem sem isso.
                let before = parse_ip_settings(
                    &nm(&["-t", "-f", "ipv4.method,ipv4.addresses,ipv4.gateway,ipv4.dns", "connection", "show", "id", &name])
                        .unwrap_or_default(),
                );
                let ip_changed =
                    ip_modify_args(before.manual, &before.address, &before.prefix, &before.gateway, &before.dns).ok()
                        != Some(args.clone());
                let mut cmd: Vec<&str> = vec!["connection", "modify", "id", &name];
                cmd.extend(args.iter().map(String::as_str));
                cmd.extend([
                    "connection.autoconnect",
                    if autoconnect { "yes" } else { "no" },
                    // Desligado = "unknown": o NetworkManager ainda percebe sozinho
                    // o roteador do celular (como no Ubuntu).
                    "connection.metered",
                    if metered { "yes" } else { "unknown" },
                ]);
                nm(&cmd)?;
                let state = nm(&["-g", "GENERAL.STATE,GENERAL.DEVICES", "connection", "show", "id", &name]).unwrap_or_default();
                if state.contains("activated") {
                    if ip_changed {
                        nm(&["connection", "up", "id", &name])?;
                    } else if let Some(dev) = state.lines().nth(1).filter(|d| !d.is_empty()) {
                        // Conexão limitada vale na hora com o reapply.
                        let _ = nm(&["device", "reapply", dev]);
                    }
                }
                Ok(String::new())
            },
            |d, r| match r {
                Ok(_) => {
                    d.set_ip_message(tr("Settings saved.", "Configurações salvas.").into());
                    d.set_ip_for(SharedString::new());
                }
                Err(e) => d.set_ip_message(format!("{} {e}", tr("Not saved:", "Não salvo:")).into()),
            },
        );
    });

    let weak = ui.as_weak();
    d.on_conn_show_password(move |name| {
        let Some(ui) = weak.upgrade() else { return };
        let d = ui.global::<Device>();
        match nm(&["-s", "-g", "802-11-wireless-security.psk", "connection", "show", "id", &name]) {
            Ok(p) if !p.trim().is_empty() => d.set_conn_password(p.trim().into()),
            _ => d.set_ip_message(
                tr(
                    "The password could not be read (it may be stored only for another user).",
                    "Não foi possível ler a senha (ela pode estar guardada só para outro usuário).",
                )
                .into(),
            ),
        }
    });

    let weak = ui.as_weak();
    d.on_conn_forget(move |name| {
        let name = name.to_string();
        let shown = name.clone();
        background(
            &weak,
            move || nm(&["connection", "delete", "id", &name]).map(|_| String::new()),
            move |d, r| match r {
                Ok(_) => {
                    d.set_ip_for(SharedString::new());
                    d.set_ip_message(format!("{} {shown}", tr("Removed:", "Removida:")).into());
                }
                Err(e) => d.set_ip_message(format!("{} {e}", tr("Not removed:", "Não removida:")).into()),
            },
        );
    });

    // --- VPN: ligar, desligar, remover ---
    let weak = ui.as_weak();
    d.on_vpn_set(move |name, on| {
        if let Some(ui) = weak.upgrade() {
            let d = ui.global::<Device>();
            d.set_vpn_busy(name.clone());
            d.set_vpn_message(SharedString::new());
        }
        let name = name.to_string();
        let login_name = name.clone();
        background(
            &weak,
            move || nm(&["connection", if on { "up" } else { "down" }, "id", &name]),
            move |d, r| {
                d.set_vpn_busy(SharedString::new());
                if let Err(e) = r {
                    let e = if e.contains("secrets") || e.contains("Secrets") {
                        // Abre o pedido de usuário e senha desta VPN.
                        d.set_vpn_login_for(login_name.into());
                        tr("The VPN needs a username and password.", "A VPN precisa de usuário e senha.").to_string()
                    } else {
                        e
                    };
                    d.set_vpn_message(format!("{} {e}", tr("Could not connect:", "Não foi possível conectar:")).into());
                }
            },
        );
    });

    let weak = ui.as_weak();
    d.on_vpn_remove(move |name| {
        let name = name.to_string();
        background(
            &weak,
            move || nm(&["connection", "delete", "id", &name]),
            |d, r| {
                if let Err(e) = r {
                    d.set_vpn_message(e.into());
                }
            },
        );
    });

    // --- VPN: importar arquivo ---
    let weak = ui.as_weak();
    d.on_vpn_import(move || {
        background(
            &weak,
            || {
                let Some(path) = pick_file(tr("Import VPN", "Importar VPN"), Some("VPN | *.ovpn *.conf")) else {
                    return Ok(String::new());
                };
                let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                if text.to_ascii_lowercase().contains("[interface]") {
                    let cfg = parse_wg_conf(&text)?;
                    let stem = std::path::Path::new(&path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "WireGuard".into());
                    create_wireguard(&stem, &cfg)?;
                    Ok(String::new())
                } else {
                    let (name, needs_login) = import_openvpn(&path)?;
                    // "login:" = a UI pede usuário e senha para esta VPN.
                    Ok(if needs_login { format!("login:{name}") } else { String::new() })
                }
            },
            |d, r| match r {
                Ok(s) if s.starts_with("login:") => {
                    d.set_vpn_login_for(s.trim_start_matches("login:").into());
                    d.set_vpn_message(tr("VPN imported. Enter the username and password.", "VPN importada. Digite o usuário e a senha.").into());
                }
                Ok(_) => d.set_vpn_message(SharedString::new()),
                Err(e) => d.set_vpn_message(format!("{} {e}", tr("Not imported:", "Não importada:")).into()),
            },
        );
    });

    let weak = ui.as_weak();
    d.on_vpn_set_login(move |name, user, password| {
        let name = name.to_string();
        background(
            &weak,
            move || set_login(&name, &user, &password).map(|_| String::new()),
            |d, r| match r {
                Ok(_) => {
                    d.set_vpn_login_for(SharedString::new());
                    d.set_vpn_message(tr("Username and password saved.", "Usuário e senha salvos.").into());
                }
                Err(e) => d.set_vpn_message(e.into()),
            },
        );
    });

    // --- VPN: formulários ---
    let weak = ui.as_weak();
    d.on_vpn_add_openvpn(move |name, server, port, tcp, auth, ca, cert, key, user, password| {
        let name = name.trim().to_string();
        let (server, port, ca, cert, key, user, password) =
            (server.to_string(), port.to_string(), ca.to_string(), cert.to_string(), key.to_string(), user.to_string(), password.to_string());
        background(
            &weak,
            move || {
                if name.is_empty() {
                    return Err(tr("Give the VPN a name.", "Dê um nome à VPN.").into());
                }
                let text = build_ovpn(&server, &port, tcp, auth, &ca, &cert, &key)?;
                let dir = vpn_dir().ok_or("HOME")?;
                fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let file = dir.join(format!("{}.ovpn", interface_name(&name)));
                fs::write(&file, text).map_err(|e| e.to_string())?;
                let (imported, _) = import_openvpn(&file.to_string_lossy())?;
                nm(&["connection", "modify", "id", &imported, "connection.id", &name])?;
                if auth != AUTH_CERTS {
                    set_login(&name, &user, &password)?;
                }
                Ok(String::new())
            },
            |d, r| match r {
                Ok(_) => {
                    d.set_vpn_form(0);
                    d.set_vpn_message(tr("VPN added.", "VPN adicionada.").into());
                }
                Err(e) => d.set_vpn_message(e.into()),
            },
        );
    });

    let weak = ui.as_weak();
    d.on_vpn_add_wireguard(move |name, address, private_key, dns, peer_key, endpoint, allowed| {
        let name = name.trim().to_string();
        // Monta um .conf e usa o mesmo caminho da importação.
        let conf = format!(
            "[Interface]\nPrivateKey = {}\nAddress = {}\nDNS = {}\n\n[Peer]\nPublicKey = {}\nEndpoint = {}\nAllowedIPs = {}\nPersistentKeepalive = 25\n",
            private_key.trim(),
            address.trim(),
            dns.trim(),
            peer_key.trim(),
            endpoint.trim(),
            if allowed.trim().is_empty() { "0.0.0.0/0" } else { allowed.trim() }
        );
        background(
            &weak,
            move || {
                if name.is_empty() {
                    return Err(tr("Give the VPN a name.", "Dê um nome à VPN.").into());
                }
                let cfg = parse_wg_conf(&conf)?;
                create_wireguard(&name, &cfg).map(|_| String::new())
            },
            |d, r| match r {
                Ok(_) => {
                    d.set_vpn_form(0);
                    d.set_vpn_message(tr("VPN added.", "VPN adicionada.").into());
                }
                Err(e) => d.set_vpn_message(e.into()),
            },
        );
    });

    let weak = ui.as_weak();
    d.on_wg_generate_key(move || {
        let Some(ui) = weak.upgrade() else { return };
        let d = ui.global::<Device>();
        let private = run("wg", &["genkey"]).map(|k| k.trim().to_string()).unwrap_or_default();
        let public = run_with_input("wg", &["pubkey"], &private).unwrap_or_default();
        if private.is_empty() || public.is_empty() {
            d.set_vpn_message(tr("Could not generate the key (wireguard-tools).", "Não foi possível gerar a chave (wireguard-tools).").into());
            return;
        }
        d.set_wg_private_key(private.into());
        d.set_wg_public_key(public.into());
    });

    let weak = ui.as_weak();
    d.on_wg_public_of(move |private| {
        let Some(ui) = weak.upgrade() else { return };
        let public = if valid_wg_key(private.trim()) {
            run_with_input("wg", &["pubkey"], private.trim()).unwrap_or_default()
        } else {
            String::new()
        };
        ui.global::<Device>().set_wg_public_key(public.into());
    });

    let weak = ui.as_weak();
    d.on_pick_file(move |purpose| {
        let weak = weak.clone();
        std::thread::spawn(move || {
            let filter = if purpose == "key" { "Key | *.key *.pem" } else { "Certificate | *.crt *.pem *.cer" };
            let Some(path) = pick_file(tr("Choose a file", "Escolha um arquivo"), Some(filter)) else { return };
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak.upgrade() {
                    let d = ui.global::<Device>();
                    d.set_picked_purpose(purpose);
                    d.set_picked_path(path.into());
                    d.set_picked_serial(d.get_picked_serial() + 1);
                }
            });
        });
    });

    refresh(ui);
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, INTERVAL, move || {
        if let Some(ui) = weak.upgrade() {
            refresh(&ui);
        }
    });
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mascara_e_prefixo() {
        assert_eq!(parse_prefix("24"), Some(24));
        assert_eq!(parse_prefix("/16"), Some(16));
        assert_eq!(parse_prefix("255.255.255.0"), Some(24));
        assert_eq!(parse_prefix("255.255.254.0"), Some(23));
        assert_eq!(parse_prefix("255.0.255.0"), None);
        assert_eq!(parse_prefix("33"), None);
        assert_eq!(parse_prefix("abc"), None);
    }

    #[test]
    fn argumentos_do_ip_fixo() {
        let a = ip_modify_args(true, "192.168.1.50", "255.255.255.0", "192.168.1.1", "8.8.8.8, 1.1.1.1").unwrap();
        assert_eq!(
            a,
            ["ipv4.method", "manual", "ipv4.addresses", "192.168.1.50/24", "ipv4.gateway", "192.168.1.1", "ipv4.dns", "8.8.8.8 1.1.1.1"]
        );
        assert!(ip_modify_args(true, "192.168.1.500", "24", "", "").is_err());
        assert!(ip_modify_args(true, "192.168.1.5", "24", "x", "").is_err());
        assert!(ip_modify_args(true, "192.168.1.5", "24", "", "8.8.8").is_err());
        let auto = ip_modify_args(false, "lixo", "", "", "").unwrap();
        assert_eq!(auto[1], "auto");
    }

    #[test]
    fn le_ip_atual_da_conexao() {
        let s = parse_ip_settings(
            "ipv4.method:manual\nipv4.addresses:192.168.1.50/24\nipv4.gateway:192.168.1.1\nipv4.dns:8.8.8.8,1.1.1.1\nIP4.ADDRESS[1]:192.168.1.50/24\n",
        );
        assert!(s.manual);
        assert_eq!((s.address.as_str(), s.prefix.as_str()), ("192.168.1.50", "24"));
        assert_eq!(s.dns, "8.8.8.8, 1.1.1.1");
        assert_eq!(s.current, "192.168.1.50/24");
        let auto = parse_ip_settings("ipv4.method:auto\nipv4.addresses:\nipv4.gateway:\nipv4.dns:\nIP4.ADDRESS[1]:192.168.1.50/24\n");
        assert!(!auto.manual);
        assert_eq!(auto.current, "192.168.1.50/24");
    }

    const WG: &str = "[Interface]\n# casa\nPrivateKey = yAnz5TF+lXXJte14tji3zlMNq+hd2rYUIgJBgB3fBmk=\nAddress = 10.0.0.2/24, fd00::2/64\nDNS = 1.1.1.1, casa.lan\n\n[Peer]\nPublicKey = xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=\nEndpoint = vpn.exemplo.com:51820\nAllowedIPs = 0.0.0.0/0, ::/0\nPersistentKeepalive = 25\n";

    #[test]
    fn le_arquivo_do_wireguard() {
        let cfg = parse_wg_conf(WG).unwrap();
        assert_eq!(cfg.addresses.len(), 2);
        assert_eq!(cfg.dns, vec!["1.1.1.1".parse::<IpAddr>().unwrap()]);
        assert_eq!(cfg.peers.len(), 1);
        assert_eq!(cfg.peers[0].endpoint, "vpn.exemplo.com:51820");
        assert_eq!(cfg.peers[0].allowed_ips, ["0.0.0.0/0", "::/0"]);
        assert_eq!(cfg.peers[0].keepalive, Some(25));
        assert!(parse_wg_conf("[Interface]\nPrivateKey = curta\nAddress = 10.0.0.2/24\n").is_err());
        assert!(parse_wg_conf(&WG.replace("[Peer]", "[Outro]")).is_err());
    }

    #[test]
    fn conexao_wireguard_para_o_dbus() {
        let cfg = parse_wg_conf(WG).unwrap();
        let gv = wg_connection_gvariant("Casa \"VPN\"", &cfg);
        assert!(gv.contains("\"id\": <\"Casa \\\"VPN\\\"\">"));
        assert!(gv.contains("\"interface-name\": <\"Casa--VPN\">"));
        assert!(gv.contains("\"autoconnect\": <false>"));
        assert!(gv.contains("\"allowed-ips\": <[\"0.0.0.0/0\", \"::/0\"]>"));
        assert!(gv.contains("\"address\": <\"10.0.0.2\">, \"prefix\": <uint32 24>"));
        assert!(gv.contains("\"ipv6\": {\"method\": <\"manual\">"));
    }

    #[test]
    fn nome_de_interface() {
        assert_eq!(interface_name("Minha VPN do trabalho"), "Minha-VPN-do-tr");
        assert_eq!(interface_name("***"), "wg0");
    }

    #[test]
    fn arquivo_openvpn_do_formulario() {
        let dir = std::env::temp_dir();
        let ca = dir.join("kalango-teste-ca.crt");
        fs::write(&ca, "x").unwrap();
        let ca = ca.to_string_lossy().to_string();
        let t = build_ovpn("vpn.exemplo.com", "", false, AUTH_PASSWORD, &ca, "", "").unwrap();
        assert!(t.contains("proto udp\nremote vpn.exemplo.com 1194\n"));
        assert!(t.contains("auth-user-pass"));
        assert!(!t.contains("\ncert "));
        assert!(build_ovpn("vpn.exemplo.com", "443", true, AUTH_CERTS, &ca, "", "").is_err());
        assert!(build_ovpn("vpn exemplo", "", false, AUTH_PASSWORD, &ca, "", "").is_err());
        assert!(build_ovpn("vpn.exemplo.com", "99999", false, AUTH_PASSWORD, &ca, "", "").is_err());
    }

    #[test]
    fn detalhes_do_wifi_em_uso() {
        let mut d = parse_conn_details(
            "connection.autoconnect:yes\nconnection.metered:yes\n802-11-wireless.ssid:Casa 5G\n\
             802-11-wireless-security.key-mgmt:wpa-psk\nGENERAL.STATE:activated\nGENERAL.DEVICES:wlp2s0\n\
             IP4.ADDRESS[1]:192.168.1.50/24\nIP4.GATEWAY:192.168.1.1\nIP4.DNS[1]:192.168.1.1\nIP4.DNS[2]:8.8.8.8\n\
             IP6.ADDRESS[1]:fd28::19a/128\nIP6.ADDRESS[2]:2804:14c::5/64\nIP6.ADDRESS[3]:fe80\\:\\:1/64\n",
        );
        assert!(d.active && d.autoconnect && d.metered && d.secured);
        assert_eq!((d.ssid.as_str(), d.device.as_str(), d.ipv4.as_str()), ("Casa 5G", "wlp2s0", "192.168.1.50"));
        assert_eq!(d.ipv6, "2804:14c::5");
        assert_eq!(d.dns, "192.168.1.1, 8.8.8.8");
        apply_device_details(&mut d, "GENERAL.HWADDR:AA\\:BB\\:CC\\:DD\\:EE\\:FF\nCAPABILITIES.SPEED:unknown\n");
        assert_eq!(d.hwaddr, "AA:BB:CC:DD:EE:FF");
        apply_wifi_details(&mut d, ":Vizinho:90:2412 MHz:54 Mbit/s:WPA2\n*:Casa 5G:68:5180 MHz:270 Mbit/s:WPA2 WPA3\n");
        assert_eq!((d.signal, d.freq), (Some(68), Some(5180)));
        let rows = detail_rows(&d);
        let get = |l: &str| rows.iter().find(|(k, _)| k == l).map(|(_, v)| v.as_str());
        assert_eq!(get("Signal"), Some("Good (68%)"));
        assert_eq!(get("Link speed"), Some("270 Mb/s"));
        assert_eq!(get("Security"), Some("WPA2 / WPA3"));
        assert_eq!(get("Band"), Some("5 GHz"));
        assert_eq!(get("Hardware address"), Some("AA:BB:CC:DD:EE:FF"));
    }

    #[test]
    fn detalhes_do_cabo_desligado() {
        let d = parse_conn_details("connection.autoconnect:no\nconnection.metered:unknown\nGENERAL.STATE:\nIP4.ADDRESS[1]:10.0.0.9/8\n");
        assert!(!d.active && !d.autoconnect && !d.metered && !d.secured);
        assert!(d.ipv4.is_empty(), "endereço velho não aparece");
        assert!(detail_rows(&d).is_empty());
        assert_eq!(wifi_band(2437), "2.4 GHz");
        assert_eq!(wifi_band(5955), "6 GHz");
    }

    #[test]
    fn separa_conexoes_e_vpns() {
        let (nets, vpns) = parse_connections(
            "Cats:802-11-wireless:yes\nlo:loopback:yes\nWired connection 1:802-3-ethernet:no\nTrabalho:vpn:no\ncasa:wireguard:yes\n",
        );
        assert_eq!(nets.len(), 2);
        assert_eq!(nets[1], ("Wired connection 1".to_string(), "ethernet".to_string(), false));
        assert_eq!(vpns, vec![("Trabalho".to_string(), "OpenVPN".to_string(), false), ("casa".to_string(), "WireGuard".to_string(), true)]);
    }
}
