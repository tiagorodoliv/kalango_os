//! Aviso do firewall: um programa passou a esperar conexões de outros
//! computadores numa porta que o firewall bloqueia.
//!
//! A área de trabalho olha a cada 5 s as portas TCP em escuta
//! (`/proc/net/tcp` e `tcp6`) fora do próprio computador (não `127.0.0.1` nem
//! `::1`). Porta nova, bloqueada e vista duas vezes seguidas (não é um
//! programa que abriu e fechou) → pergunta pelo zenity se libera, com o nome
//! do programa. "Permitir" = `pkexec kalango-firewall permitir PORTA/tcp NOME`
//! (senha do administrador); "Manter bloqueado" fica anotado em
//! `~/.config/kalango/firewall-bloqueados` e não pergunta mais.
//!
//! Portas efêmeras (≥ 32768, sorteadas a cada execução) ficam de fora: cada
//! execução perguntaria de novo.

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::common::i18n::tr;
use crate::settings::firewall;

/// Primeira porta efêmera do Linux (`ip_local_port_range`).
const EPHEMERAL: u16 = 32768;

/// Uma porta TCP em escuta: número e inode do socket (para achar o programa).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    pub port: u16,
    pub inode: u64,
}

/// Lê `/proc/net/tcp` ou `tcp6`: portas em escuta (estado `0A`) fora do
/// loopback.
pub fn parse_listeners(text: &str) -> Vec<Listener> {
    let mut out = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        // sl local remoto estado tx:rx tr:when retrnsmt uid timeout inode
        if f.len() < 10 || f[3] != "0A" {
            continue;
        }
        let Some((addr, port)) = f[1].split_once(':') else { continue };
        if is_loopback(addr) {
            continue;
        }
        let (Ok(port), Ok(inode)) = (u16::from_str_radix(port, 16), f[9].parse::<u64>()) else { continue };
        if !out.iter().any(|l: &Listener| l.port == port) {
            out.push(Listener { port, inode });
        }
    }
    out
}

/// Endereço em hexadecimal do `/proc/net/tcp*` é do próprio computador.
fn is_loopback(addr: &str) -> bool {
    match addr.len() {
        // IPv4 com os bytes invertidos: 127.x.x.x termina em "7F".
        8 => addr.ends_with("7F"),
        // ::1 e ::ffff:127.x.x.x.
        32 => addr == "00000000000000000000000001000000" || (addr.starts_with("0000000000000000FFFF0000") && addr.ends_with("7F")),
        _ => false,
    }
}

/// Nome do programa dono do socket (só dá para ver os da própria pessoa;
/// serviços do sistema ficam sem nome).
fn owner(inode: u64) -> Option<String> {
    let target = format!("socket:[{inode}]");
    for proc in fs::read_dir("/proc").ok()?.flatten() {
        let Ok(fds) = fs::read_dir(proc.path().join("fd")) else { continue };
        for fd in fds.flatten() {
            if fs::read_link(fd.path()).is_ok_and(|l| l.to_string_lossy() == target) {
                return fs::read_to_string(proc.path().join("comm")).ok().map(|c| c.trim().to_string());
            }
        }
    }
    None
}

/// Nome conhecido de algumas portas (para serviços do sistema).
fn known(port: u16) -> Option<&'static str> {
    Some(match port {
        22 => "SSH",
        80 | 8080 => "Web (HTTP)",
        443 => "Web (HTTPS)",
        445 | 139 => "Samba",
        3389 => "RDP",
        5900 => "VNC",
        _ => return None,
    })
}

fn blocked_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("kalango/firewall-bloqueados"))
}

fn blocked() -> HashSet<u16> {
    blocked_file()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|t| t.lines().filter_map(|l| l.trim().strip_suffix("/tcp")?.parse().ok()).collect())
        .unwrap_or_default()
}

fn remember_blocked(port: u16) {
    let Some(path) = blocked_file() else { return };
    let mut text = fs::read_to_string(&path).unwrap_or_default();
    text.push_str(&format!("{port}/tcp\n"));
    let _ = path.parent().map(fs::create_dir_all);
    let _ = fs::write(path, text);
}

/// Pergunta se libera a porta. `true` = Permitir.
fn ask(port: u16, name: &str) -> bool {
    let en = format!(
        "<b>{name}</b> wants to receive connections from other computers (port {port}).\n\n\
         The firewall is blocking it. Allow only if you trust this program."
    );
    let pt = format!(
        "<b>{name}</b> quer receber conexões de outros computadores (porta {port}).\n\n\
         O firewall está bloqueando. Permita só se você confia neste programa."
    );
    let text = tr(&en, &pt);
    Command::new("zenity")
        .args([
            "--question",
            "--icon=security-medium",
            &format!("--title={}", tr("Firewall", "Firewall")),
            &format!("--text={text}"),
            &format!("--ok-label={}", tr("Allow", "Permitir")),
            &format!("--cancel-label={}", tr("Keep blocked", "Manter bloqueado")),
            "--width=420",
        ])
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Escapa o nome para a marcação do zenity (Pango).
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Começa a vigiar as portas (área de trabalho da sessão real).
pub fn start() {
    // Só o script do firewall precisa existir agora: o ufw pode chegar
    // depois (atualização instalada com a sessão aberta); sem ele não há
    // `firewall.conf` ligado e a volta não faz nada.
    if !std::path::Path::new(firewall::HELPER).exists() {
        return;
    }
    std::thread::spawn(|| {
        // Perguntadas nesta sessão (não repetir enquanto o programa roda) e
        // as vistas na volta anterior (só pergunta se continuar aberta).
        let mut asked: HashSet<u16> = HashSet::new();
        let mut seen: HashSet<u16> = HashSet::new();
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let Some(conf) = firewall::read().filter(|c| c.enabled) else {
                seen.clear();
                continue;
            };
            let mut now = Vec::new();
            for file in ["/proc/net/tcp", "/proc/net/tcp6"] {
                if let Ok(text) = fs::read_to_string(file) {
                    for l in parse_listeners(&text) {
                        if !now.iter().any(|n: &Listener| n.port == l.port) {
                            now.push(l);
                        }
                    }
                }
            }
            let ignored = blocked();
            let candidates: Vec<Listener> = now
                .into_iter()
                .filter(|l| l.port < EPHEMERAL && !conf.covers_tcp(l.port))
                .filter(|l| !ignored.contains(&l.port) && !asked.contains(&l.port))
                .collect();
            let previous = std::mem::replace(&mut seen, candidates.iter().map(|l| l.port).collect());
            for l in candidates.into_iter().filter(|l| previous.contains(&l.port)) {
                asked.insert(l.port);
                let name = owner(l.inode)
                    .or_else(|| known(l.port).map(str::to_string))
                    .unwrap_or_else(|| tr("A system service", "Um serviço do sistema").to_string());
                if ask(l.port, &escape(&name)) {
                    let port = format!("{}/tcp", l.port);
                    let ok = firewall::helper(&["permitir", &port, &name]);
                    let (en, pt) = (
                        format!("Port {} allowed in the firewall", l.port),
                        format!("Porta {} liberada no firewall", l.port),
                    );
                    crate::shell::capture::notify(if ok {
                        tr(&en, &pt)
                    } else {
                        tr("The firewall was not changed", "O firewall não foi alterado")
                    });
                } else {
                    remember_blocked(l.port);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portas_em_escuta_fora_do_loopback() {
        let tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 1111 1 0 100 0 0 10 0\n\
   1: 0100007F:0277 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 2222 1 0 100 0 0 10 0\n\
   2: 0F02000A:1F40 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 3333 1 0 100 0 0 10 0\n\
   3: 0F02000A:D2A4 0102000A:01BB 01 00000000:00000000 00:00000000 00000000  1000        0 4444 1 0 100 0 0 10 0\n";
        assert_eq!(
            parse_listeners(tcp),
            [Listener { port: 22, inode: 1111 }, Listener { port: 8000, inode: 3333 }]
        );
        let tcp6 = "  sl  local_address remote_address st\n\
   0: 00000000000000000000000001000000:0277 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 5555 1\n\
   1: 00000000000000000000000000000000:0016 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 6666 1\n";
        assert_eq!(parse_listeners(tcp6), [Listener { port: 22, inode: 6666 }]);
    }
}
