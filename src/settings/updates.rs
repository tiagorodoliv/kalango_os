//! Configurações → Atualizações: sistema (apt) e apps (Flatpak), sem terminal.
//!
//! - **Lista**: `apt list --upgradable` (o Debian já baixa a lista de versões
//!   novas todo dia pelo `apt-daily.timer`, então ler não pede senha) e
//!   `flatpak remote-ls --updates`.
//! - **Procurar agora**: `pkexec /usr/local/sbin/kalango-atualizar procurar`
//!   (`apt-get update`), depois relê.
//! - **Instalar**: `pkexec kalango-atualizar instalar` (`apt-get full-upgrade`
//!   com `APT::Status-Fd`: as linhas `dlstatus`/`pmstatus` viram a barra de
//!   progresso) e `flatpak update`. Kernel, systemd ou glibc novos pedem para
//!   reiniciar.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::SystemTime;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::common::i18n::tr;
use crate::{AppWindow, Mode, UpdateItem, Updates};

/// Script de root (via pkexec).
pub const HELPER: &str = "/usr/local/sbin/kalango-atualizar";

/// Uma atualização do apt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub old: String,
    pub new: String,
}

/// Lê `apt list --upgradable`:
/// "firefox-esr/stable-security 128.5.0esr-1~deb13u1 amd64 [upgradable from: 128.4.0esr-1~deb13u1]".
pub fn parse_upgradable(text: &str) -> Vec<Package> {
    text.lines()
        .filter_map(|l| {
            let (name, rest) = l.split_once('/')?;
            let new = rest.split_whitespace().nth(1)?.to_string();
            let old = l.split_once("upgradable from: ")?.1.trim_end_matches(']').trim().to_string();
            Some(Package { name: name.to_string(), old, new })
        })
        .collect()
}

/// Lê `flatpak remote-ls --updates --columns=name,application,version`
/// (colunas separadas por tabulação).
pub fn parse_flatpak(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            let name = f.first()?.trim();
            (!name.is_empty()).then(|| (name.to_string(), f.get(2).map(|v| v.trim().to_string()).unwrap_or_default()))
        })
        .collect()
}

/// Progresso de uma linha do `APT::Status-Fd`: `(0..1, texto)`. O download
/// vale a primeira metade da barra; a instalação, a segunda.
pub fn parse_status(line: &str) -> Option<(f32, String)> {
    let mut f = line.splitn(4, ':');
    let kind = f.next()?;
    let pkg = f.next()?;
    let pct: f32 = f.next()?.trim().parse().ok()?;
    let desc = f.next().unwrap_or("").trim();
    match kind {
        "dlstatus" => Some(((pct / 200.0).clamp(0.0, 0.5), format!("{} {pkg}", tr("Downloading", "Baixando")))),
        "pmstatus" => Some((0.5 + (pct / 200.0).clamp(0.0, 0.5), if desc.is_empty() { pkg.to_string() } else { desc.to_string() })),
        _ => None,
    }
}

/// Atualizar estes pacotes pede reiniciar o computador.
pub fn needs_reboot(packages: &[Package]) -> bool {
    packages.iter().any(|p| {
        p.name.starts_with("linux-image") || matches!(p.name.as_str(), "libc6" | "systemd" | "dbus" | "dbus-daemon")
    })
}

fn read(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// Lê o que há para atualizar (apt e Flatpak).
fn read_updates() -> (Vec<Package>, Vec<(String, String)>) {
    let apt = parse_upgradable(&read("apt", &["list", "--upgradable"]));
    let flatpak = if crate::shell::session::find_in_path("flatpak").is_some() {
        parse_flatpak(&read("flatpak", &["remote-ls", "--updates", "--columns=name,application,version"]))
    } else {
        Vec::new()
    };
    (apt, flatpak)
}

/// Quando o apt atualizou a lista pela última vez ("hoje às 08:12").
fn last_check() -> String {
    let Ok(modified) = std::fs::metadata("/var/lib/apt/lists/partial")
        .or_else(|_| std::fs::metadata("/var/lib/apt/lists"))
        .and_then(|m| m.modified())
    else {
        return String::new();
    };
    let when: chrono::DateTime<chrono::Local> = modified.into();
    let now = chrono::Local::now();
    let age = SystemTime::now().duration_since(modified).map(|d| d.as_secs()).unwrap_or(0);
    let time = when.format("%H:%M").to_string();
    if when.date_naive() == now.date_naive() {
        format!("{} {time}", tr("Checked today at", "Procurado hoje às"))
    } else if age < 2 * 86_400 {
        format!("{} {time}", tr("Checked yesterday at", "Procurado ontem às"))
    } else {
        format!("{} {}", tr("Last checked on", "Procurado em"), when.format("%d/%m"))
    }
}

/// Mostra a lista na página.
fn show(ui: &AppWindow, apt: &[Package], flatpak: &[(String, String)]) {
    let u = ui.global::<Updates>();
    let mut items: Vec<UpdateItem> = apt
        .iter()
        .map(|p| UpdateItem { name: p.name.clone().into(), version: format!("{} → {}", p.old, p.new).into(), app: false })
        .collect();
    items.extend(flatpak.iter().map(|(name, v)| UpdateItem { name: name.clone().into(), version: v.clone().into(), app: true }));
    u.set_list(ModelRc::from(Rc::new(VecModel::from(items))));
    u.set_last_check(last_check().into());
}

/// Relê em segundo plano e mostra.
fn refresh(weak: slint::Weak<AppWindow>) {
    std::thread::spawn(move || {
        let (apt, flatpak) = read_updates();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                show(&ui, &apt, &flatpak);
            }
        });
    });
}

/// Mensagem de fim (estado `done`/`error`) e relê a lista.
fn finish(weak: slint::Weak<AppWindow>, ok: bool, message: String, reboot: bool) {
    let (apt, flatpak) = read_updates();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(ui) = weak.upgrade() else { return };
        let u = ui.global::<Updates>();
        u.set_state(if ok { "done" } else { "error" }.into());
        u.set_message(message.into());
        u.set_status("".into());
        if reboot {
            u.set_reboot_needed(true);
        }
        show(&ui, &apt, &flatpak);
    });
}

/// "Procurar agora".
fn check(ui: &AppWindow) {
    let u = ui.global::<Updates>();
    if matches!(u.get_state().as_str(), "checking" | "installing") {
        return;
    }
    u.set_state("checking".into());
    u.set_message("".into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let ok = Command::new("pkexec")
            .args([HELPER, "procurar"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        let (apt, flatpak) = read_updates();
        let n = apt.len() + flatpak.len();
        let message = if !ok {
            tr("Could not check for updates (no internet or not authorized).", "Não foi possível procurar (sem internet ou não autorizado).").to_string()
        } else {
            // O título já diz "Tudo atualizado" ou quantas há.
            let _ = n;
            String::new()
        };
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let u = ui.global::<Updates>();
            u.set_state(if ok { "idle" } else { "error" }.into());
            u.set_message(message.into());
            show(&ui, &apt, &flatpak);
        });
    });
}

/// "Instalar": apt com barra de progresso, depois os apps do Flatpak.
fn install(ui: &AppWindow) {
    let u = ui.global::<Updates>();
    if matches!(u.get_state().as_str(), "checking" | "installing") {
        return;
    }
    u.set_state("installing".into());
    u.set_progress(0.0);
    u.set_message("".into());
    u.set_status(tr("Waiting for the administrator password...", "Esperando a senha do administrador...").into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let (apt, flatpak) = read_updates();
        let reboot = needs_reboot(&apt);
        if !apt.is_empty() {
            let child = Command::new("pkexec")
                .args([HELPER, "instalar"])
                .env("LC_ALL", "C")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else {
                return finish(weak, false, tr("pkexec is not installed", "O pkexec não está instalado").into(), false);
            };
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if let Some((p, text)) = parse_status(&line) {
                        let weak = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = weak.upgrade() {
                                let u = ui.global::<Updates>();
                                u.set_progress(p);
                                u.set_status(text.into());
                            }
                        });
                    }
                }
            }
            let ok = child.wait().is_ok_and(|s| s.success());
            if !ok {
                return finish(
                    weak,
                    false,
                    tr("The update was not installed (not authorized or an error in apt).", "A atualização não foi instalada (não autorizado ou erro no apt).").into(),
                    false,
                );
            }
        }
        if !flatpak.is_empty() {
            let weak2 = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = weak2.upgrade() {
                    ui.global::<Updates>().set_status(tr("Updating apps (Flatpak)...", "Atualizando os apps (Flatpak)...").into());
                }
            });
            let _ = Command::new("flatpak").args(["update", "-y", "--noninteractive"]).stdin(Stdio::null()).status();
        }
        let message = if reboot {
            tr("Updates installed. Restart to finish.", "Atualizações instaladas. Reinicie para concluir.")
        } else {
            tr("Updates installed.", "Atualizações instaladas.")
        };
        finish(weak, true, message.into(), reboot);
    });
}

/// Liga a página Atualizações (Configurações, sessão real).
pub fn install_page(ui: &AppWindow, mode: Mode) {
    if !(crate::shell::session::is_native() && mode == Mode::Settings) {
        return;
    }
    refresh(ui.as_weak());
    let u = ui.global::<Updates>();
    let weak = ui.as_weak();
    u.on_check(move || {
        if let Some(ui) = weak.upgrade() {
            check(&ui);
        }
    });
    let weak = ui.as_weak();
    u.on_install(move || {
        if let Some(ui) = weak.upgrade() {
            install(&ui);
        }
    });
    u.on_reboot(|| {
        crate::settings::device::spawn_detached("systemctl", &["reboot"]);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lista_do_apt() {
        let text = "Listing...\n\
                    firefox-esr/stable-security 128.5.0esr-1~deb13u1 amd64 [upgradable from: 128.4.0esr-1~deb13u1]\n\
                    linux-image-amd64/stable 6.12.120-1 amd64 [upgradable from: 6.12.117-1]\n";
        let p = parse_upgradable(text);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], Package { name: "firefox-esr".into(), old: "128.4.0esr-1~deb13u1".into(), new: "128.5.0esr-1~deb13u1".into() });
        assert!(needs_reboot(&p));
        assert!(!needs_reboot(&p[..1]));
    }

    #[test]
    fn flatpak_e_progresso() {
        let f = parse_flatpak("Firefox\torg.mozilla.firefox\t133.0\nGIMP\torg.gimp.GIMP\t\n");
        assert_eq!(f, [("Firefox".to_string(), "133.0".to_string()), ("GIMP".to_string(), String::new())]);
        let (p, t) = parse_status("dlstatus:1:50.0:Retrieving file 1 of 2").unwrap();
        assert!((p - 0.25).abs() < 1e-6 && t.contains("1"));
        let (p, t) = parse_status("pmstatus:firefox-esr:80.0:Installing firefox-esr (amd64)").unwrap();
        assert!((p - 0.9).abs() < 1e-6);
        assert_eq!(t, "Installing firefox-esr (amd64)");
        assert_eq!(parse_status("Reading package lists..."), None);
    }
}
