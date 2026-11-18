//! Firewall do KalangOS (ufw), ligado por padrão: bloqueia o que chega de
//! fora e libera o que sai, como o Ubuntu com o ufw ligado.
//!
//! Quem mexe no ufw é o `/usr/local/sbin/kalango-firewall` (root, pelo
//! `pkexec`, política `org.kalango.firewall`). Ele grava o estado e as portas
//! liberadas em `/etc/kalango/firewall.conf`, que todos leem: daqui saem a
//! seção Firewall de Configurações → Compartilhamento e o aviso de "programa
//! quer receber conexões" ([`crate::shell::firewall`]).
//!
//! O compartilhamento de pastas libera e fecha as portas do Samba sozinho
//! (`kalango-compartilhar ligar|desligar`).

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Weak};

use crate::common::i18n::tr;
use crate::{AppWindow, Device, FirewallRule, Mode};

/// Script de root (via pkexec) que mexe no ufw.
pub const HELPER: &str = "/usr/local/sbin/kalango-firewall";

/// Estado e portas liberadas (gravado pelo [`HELPER`]).
pub const CONF: &str = "/etc/kalango/firewall.conf";

/// Conteúdo do [`CONF`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// Firewall ligado.
    pub enabled: bool,
    /// Portas liberadas: ("8000/tcp", "python3").
    pub rules: Vec<(String, String)>,
}

impl Config {
    /// Lê o texto do arquivo (`ligado=sim`, `permitir=PORTA/PROTO NOME`).
    pub fn parse(text: &str) -> Config {
        let mut c = Config::default();
        for line in text.lines() {
            match line.trim().split_once('=') {
                Some(("ligado", v)) => c.enabled = v.trim() == "sim",
                Some(("permitir", v)) => {
                    let (port, name) = v.trim().split_once(' ').unwrap_or((v.trim(), ""));
                    if !port.is_empty() {
                        c.rules.push((port.to_string(), name.trim().to_string()));
                    }
                }
                _ => {}
            }
        }
        c
    }

    /// A porta TCP já está liberada.
    pub fn allows_tcp(&self, port: u16) -> bool {
        let key = format!("{port}/tcp");
        self.rules.iter().any(|(p, _)| *p == key)
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

/// Mostra o estado e a lista de portas liberadas.
fn show(d: &Device) {
    let c = read().unwrap_or_default();
    d.set_firewall_enabled(c.enabled);
    let rules: Vec<FirewallRule> = c
        .rules
        .into_iter()
        .map(|(port, name)| FirewallRule { port: port.into(), name: name.into() })
        .collect();
    d.set_firewall_rules(ModelRc::from(Rc::new(VecModel::from(rules))));
}

/// Roda `work` numa thread e, no fim, atualiza a seção (com a mensagem de
/// erro se não deu certo: senha cancelada, ufw com problema).
fn in_background(weak: &Weak<AppWindow>, work: impl FnOnce() -> bool + Send + 'static) {
    if let Some(ui) = weak.upgrade() {
        ui.global::<Device>().set_firewall_busy(true);
    }
    let weak = weak.clone();
    std::thread::spawn(move || {
        let ok = work();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                let d = ui.global::<Device>();
                d.set_firewall_busy(false);
                show(&d);
                d.set_firewall_message(if ok {
                    SharedString::new()
                } else {
                    tr("The firewall was not changed.", "O firewall não foi alterado.").into()
                });
            }
        });
    });
}

/// Liga a seção Firewall ao global `Device` (Configurações da sessão real).
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
        let port = rule.port.to_string();
        in_background(&weak, move || helper(&["remover", &port]));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_o_arquivo() {
        let c = Config::parse(
            "# Firewall do KalangOS\nligado=sim\npermitir=22/tcp sshd\npermitir=445/tcp Compartilhamento\npermitir=137/udp\n",
        );
        assert!(c.enabled);
        assert_eq!(c.rules.len(), 3);
        assert_eq!(c.rules[0], ("22/tcp".to_string(), "sshd".to_string()));
        assert_eq!(c.rules[2], ("137/udp".to_string(), String::new()));
        assert!(c.allows_tcp(445));
        assert!(!c.allows_tcp(137));
        assert!(!Config::parse("ligado=nao\n").enabled);
    }
}
