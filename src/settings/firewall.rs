//! Firewall do KalangOS (ufw), ligado por padrão: bloqueia o que chega de
//! fora e libera o que sai, como o Ubuntu com o ufw ligado.
//!
//! Quem mexe no ufw é o `/usr/local/sbin/kalango-firewall` (root, pelo
//! `pkexec`, política `org.kalango.firewall`). Ele grava o estado e as regras
//! em `/etc/kalango/firewall.conf`, que todos leem: daqui saem a página
//! Configurações → Firewall e o aviso de "programa quer receber conexões"
//! ([`crate::shell::firewall`]).
//!
//! Cada regra libera uma porta ou uma faixa (`6000:6010`), em TCP ou UDP
//! ("TCP e UDP" vira duas regras), para qualquer endereço ou só para a rede
//! local, e pode ser desativada sem sair da lista. O compartilhamento de
//! pastas libera e fecha as portas do Samba sozinho (só para a rede local).

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Weak};

use crate::common::i18n::tr;
use crate::{AppWindow, Device, FirewallRule, Mode};

/// Script de root (via pkexec) que mexe no ufw.
pub const HELPER: &str = "/usr/local/sbin/kalango-firewall";

/// Estado e regras (gravado pelo [`HELPER`]).
pub const CONF: &str = "/etc/kalango/firewall.conf";

/// Uma regra do [`CONF`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// Primeira e última porta (iguais numa porta só).
    pub first: u16,
    pub last: u16,
    /// "tcp" ou "udp".
    pub proto: String,
    /// Só da rede local (senão, de qualquer endereço).
    pub local: bool,
    /// Ativa (desativada fica na lista, fora do ufw).
    pub enabled: bool,
    /// Quem pediu ("Compartilhamento", "sshd", "python3").
    pub name: String,
}

impl Rule {
    /// Como o script a chama: "8000/tcp", "6000:6010/udp".
    pub fn key(&self) -> String {
        if self.first == self.last {
            format!("{}/{}", self.first, self.proto)
        } else {
            format!("{}:{}/{}", self.first, self.last, self.proto)
        }
    }

    /// Para mostrar: "8000 · TCP", "6000–6010 · UDP".
    pub fn label(&self) -> String {
        let ports = if self.first == self.last {
            self.first.to_string()
        } else {
            format!("{}–{}", self.first, self.last)
        };
        format!("{ports} · {}", self.proto.to_uppercase())
    }
}

/// Conteúdo do [`CONF`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Firewall ligado.
    pub enabled: bool,
    pub rules: Vec<Rule>,
}

/// "8000" ou "6000:6010" → (primeira, última).
fn parse_ports(text: &str) -> Option<(u16, u16)> {
    let (a, b) = text.split_once(':').unwrap_or((text, text));
    let (a, b) = (a.parse::<u16>().ok()?, b.parse::<u16>().ok()?);
    (a >= 1 && a <= b).then_some((a, b))
}

impl Config {
    /// Lê o texto do arquivo: `ligado=sim` e uma linha
    /// `permitir=PORTAS/PROTO ORIGEM ESTADO NOME` por regra (as antigas,
    /// `permitir=PORTAS/PROTO NOME`, valem como "qualquer" e "on").
    pub fn parse(text: &str) -> Config {
        let mut c = Config::default();
        for line in text.lines() {
            match line.trim().split_once('=') {
                Some(("ligado", v)) => c.enabled = v.trim() == "sim",
                Some(("permitir", v)) => {
                    let v = v.trim();
                    let (key, rest) = v.split_once(' ').unwrap_or((v, ""));
                    let Some((ports, proto)) = key.split_once('/') else { continue };
                    let Some((first, last)) = parse_ports(ports) else { continue };
                    let words: Vec<&str> = rest.split_whitespace().collect();
                    let new_format = words.len() >= 2
                        && matches!(words[0], "qualquer" | "local")
                        && matches!(words[1], "on" | "off");
                    let (local, enabled, name) = if new_format {
                        (words[0] == "local", words[1] == "on", words[2..].join(" "))
                    } else {
                        (false, true, rest.trim().to_string())
                    };
                    c.rules.push(Rule { first, last, proto: proto.to_string(), local, enabled, name });
                }
                _ => {}
            }
        }
        c
    }

    /// Alguma regra (ativa ou desativada) cobre a porta TCP: a pessoa já
    /// decidiu sobre ela e o aviso não pergunta de novo.
    pub fn covers_tcp(&self, port: u16) -> bool {
        self.rules.iter().any(|r| r.proto == "tcp" && (r.first..=r.last).contains(&port))
    }
}

/// Lê o [`CONF`]; `None` sem firewall instalado ou ainda não iniciado.
pub fn read() -> Option<Config> {
    fs::read_to_string(CONF).ok().map(|t| Config::parse(&t))
}

/// Firewall disponível: ufw e o script instalados.
pub fn available() -> bool {
    Path::new(HELPER).exists() && Path::new("/usr/sbin/ufw").exists()
}

/// Roda o script de root pelo pkexec (a janela de senha é do lxpolkit).
pub fn helper(args: &[&str]) -> bool {
    Command::new("pkexec")
        .arg(HELPER)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Confere o que foi digitado em "Abrir uma porta" e devolve o argumento do
/// script ("8000/tcp", "6000:6010/ambos"). Aceita "6000-6010" e espaços.
/// `proto`: 0 = TCP, 1 = UDP, 2 = os dois.
pub fn request(ports: &str, proto: i32) -> Result<String, &'static str> {
    let compact: String = ports.chars().filter(|c| !c.is_whitespace()).collect::<String>().replace(['-', '–'], ":");
    if compact.is_empty() {
        return Err("empty");
    }
    let (first, last) = parse_ports(&compact).ok_or("invalid")?;
    let proto = match proto {
        0 => "tcp",
        1 => "udp",
        _ => "ambos",
    };
    Ok(if first == last { format!("{first}/{proto}") } else { format!("{first}:{last}/{proto}") })
}

/// Mostra o estado e a lista de regras.
fn show(d: &Device) {
    let c = read().unwrap_or_default();
    d.set_firewall_enabled(c.enabled);
    let rules: Vec<FirewallRule> = c
        .rules
        .iter()
        .map(|r| FirewallRule {
            port: r.label().into(),
            key: r.key().into(),
            name: r.name.clone().into(),
            local: r.local,
            enabled: r.enabled,
        })
        .collect();
    d.set_firewall_rules(ModelRc::from(Rc::new(VecModel::from(rules))));
}

/// Roda `work` numa thread e, no fim, atualiza a página (com a mensagem de
/// erro se não deu certo: senha cancelada, ufw com problema).
fn in_background(weak: &Weak<AppWindow>, work: impl FnOnce() -> bool + Send + 'static) {
    if let Some(ui) = weak.upgrade() {
        let d = ui.global::<Device>();
        d.set_firewall_busy(true);
        d.set_firewall_message(SharedString::new());
    }
    let weak = weak.clone();
    std::thread::spawn(move || {
        let ok = work();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                let d = ui.global::<Device>();
                d.set_firewall_busy(false);
                show(&d);
                if ok {
                    d.set_firewall_added(d.get_firewall_added() + 1);
                } else {
                    d.set_firewall_message(tr("The firewall was not changed.", "O firewall não foi alterado.").into());
                }
            }
        });
    });
}

/// Liga a página Firewall ao global `Device` (Configurações da sessão real).
pub fn install(ui: &AppWindow, mode: Mode) {
    if !(crate::shell::session::is_native() && mode == Mode::Settings) {
        return;
    }
    let d = ui.global::<Device>();
    d.set_firewall_available(available());
    show(&d);

    let weak = ui.as_weak();
    d.on_set_firewall(move |on| {
        in_background(&weak, move || helper(&[if on { "ligar" } else { "desligar" }]));
    });

    let weak = ui.as_weak();
    d.on_remove_firewall_rule(move |i| {
        let Some(ui) = weak.upgrade() else { return };
        let Some(rule) = ui.global::<Device>().get_firewall_rules().row_data(i as usize) else { return };
        let key = rule.key.to_string();
        in_background(&weak, move || helper(&["remover", &key]));
    });

    let weak = ui.as_weak();
    d.on_set_firewall_rule(move |i, on| {
        let Some(ui) = weak.upgrade() else { return };
        let Some(rule) = ui.global::<Device>().get_firewall_rules().row_data(i as usize) else { return };
        let key = rule.key.to_string();
        in_background(&weak, move || helper(&[if on { "ativar" } else { "desativar" }, &key]));
    });

    let weak = ui.as_weak();
    d.on_add_firewall_rule(move |name, ports, proto, local| {
        let Some(ui) = weak.upgrade() else { return };
        let arg = match request(&ports, proto) {
            Ok(arg) => arg,
            Err(_) => {
                ui.global::<Device>().set_firewall_message(
                    tr(
                        "Type a port from 1 to 65535, or a range such as 6000-6010.",
                        "Digite uma porta de 1 a 65535 ou uma faixa, como 6000-6010.",
                    )
                    .into(),
                );
                return;
            }
        };
        let name = name.trim().to_string();
        in_background(&weak, move || {
            let mut args = vec!["permitir", arg.as_str(), name.as_str()];
            if local {
                args.push("local");
            }
            helper(&args)
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_o_arquivo_novo_e_o_antigo() {
        let c = Config::parse(
            "# Firewall do KalangOS\nligado=sim\npermitir=22/tcp sshd\n\
             permitir=6000:6010/udp local off Jogo LAN\npermitir=445/tcp local on Compartilhamento\n\
             permitir=lixo/tcp x\npermitir=9/tcp\n",
        );
        assert!(c.enabled);
        assert_eq!(c.rules.len(), 4);
        assert_eq!(c.rules[0], Rule { first: 22, last: 22, proto: "tcp".into(), local: false, enabled: true, name: "sshd".into() });
        assert_eq!(c.rules[1].key(), "6000:6010/udp");
        assert_eq!(c.rules[1].label(), "6000–6010 · UDP");
        assert!(c.rules[1].local && !c.rules[1].enabled);
        assert_eq!(c.rules[1].name, "Jogo LAN");
        assert_eq!(c.rules[3].name, "");
        assert!(c.covers_tcp(445) && c.covers_tcp(22) && !c.covers_tcp(6005));
        assert!(!Config::parse("ligado=nao\n").enabled);
    }

    #[test]
    fn confere_o_pedido() {
        assert_eq!(request("8000", 0).unwrap(), "8000/tcp");
        assert_eq!(request(" 6000 - 6010 ", 2).unwrap(), "6000:6010/ambos");
        assert_eq!(request("6000–6010", 1).unwrap(), "6000:6010/udp");
        assert!(request("", 0).is_err());
        assert!(request("70000", 0).is_err());
        assert!(request("10-5", 0).is_err());
        assert!(request("0", 0).is_err());
        assert!(request("80;rm", 0).is_err());
    }
}
