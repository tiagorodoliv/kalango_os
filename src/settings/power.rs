//! Energia (Configurações → Energia), como no Ubuntu.
//!
//! - **Modo de energia** (economia, equilibrado, desempenho) pelo
//!   `power-profiles-daemon` (`powerprofilesctl`; o polkit libera a sessão
//!   ativa sem senha).
//! - **Apagar a tela**, **suspender sozinho** (tempos diferentes na bateria e
//!   na tomada) e **bloquear ao apagar**: gravados em
//!   `~/.config/kalango/energia.conf`. A área de trabalho roda o `swayidle`
//!   (protocolo ext-idle-notify do labwc) com esses tempos e troca quando a
//!   escolha muda ou o carregador entra/sai. A tela apaga pelo `wlopm`
//!   (wlr-output-power-management); antes de suspender a tela é bloqueada.
//!   Vídeo tocando segura a tela acesa (idle-inhibit, no próprio labwc).
//! - **Tampa e botão de ligar** são do systemd-logind (do sistema todo):
//!   `pkexec /usr/local/sbin/kalango-energia` grava
//!   `/etc/systemd/logind.conf.d/50-kalango.conf` (política
//!   `org.kalango.energia`, senha do administrador).

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime};

use slint::{ComponentHandle, Timer, TimerMode};

use crate::common::i18n::tr;
use crate::shell::session::Mode;
use crate::{AppWindow, Power};

/// Minutos das opções de "Apagar a tela" (0 = nunca).
pub const BLANK_MINUTES: [u32; 8] = [0, 1, 2, 3, 5, 10, 15, 30];
/// Minutos das opções de "Suspender" (0 = nunca).
pub const SUSPEND_MINUTES: [u32; 8] = [0, 5, 10, 15, 20, 30, 45, 60];
/// Ações do logind na ordem das listas da tampa.
pub const LID_ACTIONS: [&str; 4] = ["suspend", "lock", "ignore", "poweroff"];
/// Ações do logind na ordem da lista do botão de ligar.
pub const BUTTON_ACTIONS: [&str; 3] = ["poweroff", "suspend", "ignore"];
/// Perfis do power-profiles-daemon na ordem da tela.
pub const PROFILES: [&str; 3] = ["power-saver", "balanced", "performance"];
/// Arquivo do logind gravado pelo `kalango-energia`.
pub const LOGIND_CONF: &str = "/etc/systemd/logind.conf.d/50-kalango.conf";

/// Escolhas da pessoa (`energia.conf`), em minutos.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub blank: u32,
    pub suspend_battery: u32,
    pub suspend_ac: u32,
    pub lock: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs { blank: 5, suspend_battery: 15, suspend_ac: 0, lock: true }
    }
}

impl Prefs {
    pub fn parse(text: &str) -> Prefs {
        let mut p = Prefs::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let n = v.parse::<u32>().ok();
            match k {
                "apagar-tela" => p.blank = n.unwrap_or(p.blank),
                "suspender-bateria" => p.suspend_battery = n.unwrap_or(p.suspend_battery),
                "suspender-tomada" => p.suspend_ac = n.unwrap_or(p.suspend_ac),
                "bloquear" => p.lock = v != "nao",
                _ => {}
            }
        }
        p
    }

    pub fn to_text(&self) -> String {
        format!(
            "# Energia do KalangOS (Configurações → Energia). Tempos em minutos; 0 = nunca.\n\
             apagar-tela={}\nsuspender-bateria={}\nsuspender-tomada={}\nbloquear={}\n",
            self.blank,
            self.suspend_battery,
            self.suspend_ac,
            if self.lock { "sim" } else { "nao" }
        )
    }
}

/// Argumentos do `swayidle` para as escolhas (`on_ac` = na tomada).
pub fn swayidle_args(p: &Prefs, on_ac: bool) -> Vec<String> {
    let mut a: Vec<String> = vec!["-w".into()];
    let mut push = |v: &[&str]| a.extend(v.iter().map(|s| s.to_string()));
    if p.blank > 0 {
        let secs = (p.blank * 60).to_string();
        if p.lock {
            push(&["timeout", &secs, "kalango-bloquear &"]);
        }
        push(&["timeout", &secs, "wlopm --off '*'", "resume", "wlopm --on '*'"]);
    }
    let suspend = if on_ac { p.suspend_ac } else { p.suspend_battery };
    if suspend > 0 {
        push(&["timeout", &(suspend * 60).to_string(), "systemctl suspend"]);
    }
    if p.lock {
        // Bloqueia antes de dormir (e espera o bloqueio aparecer).
        push(&["before-sleep", "kalango-bloquear & sleep 1"]);
    }
    // "Bloquear" pedido pelo logind (tampa com a ação "lock").
    push(&["lock", "kalango-bloquear &"]);
    push(&["after-resume", "wlopm --on '*'"]);
    a
}

/// Ações da tampa e do botão no texto do logind.conf (padrões do systemd
/// quando faltam): (tampa na bateria, tampa na tomada, botão de ligar).
pub fn parse_logind(text: &str) -> (String, String, String) {
    let get = |key: &str| {
        text.lines()
            .filter_map(|l| l.trim().strip_prefix(key)?.trim_start().strip_prefix('=').map(|v| v.trim().to_string()))
            .next_back()
    };
    let lid = get("HandleLidSwitch").unwrap_or_else(|| "suspend".into());
    let lid_ac = get("HandleLidSwitchExternalPower").unwrap_or_else(|| lid.clone());
    let button = get("HandlePowerKey").unwrap_or_else(|| "poweroff".into());
    (lid, lid_ac, button)
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("kalango/energia.conf"))
}

fn read_prefs() -> Prefs {
    config_path().and_then(|p| fs::read_to_string(p).ok()).map(|t| Prefs::parse(&t)).unwrap_or_default()
}

/// Na tomada (algum carregador `Mains` ligado). Sem bateria do computador
/// (a de mouse ou fone sem fio não conta): sempre.
pub fn on_ac() -> bool {
    if crate::common::system::battery_path().is_none() {
        return true;
    }
    let Ok(dir) = fs::read_dir("/sys/class/power_supply") else { return true };
    dir.flatten().any(|e| {
        let p = e.path();
        matches!(fs::read_to_string(p.join("type")).unwrap_or_default().trim(), "Mains" | "USB")
            && fs::read_to_string(p.join("online")).is_ok_and(|v| v.trim() == "1")
    })
}

/// O computador tem tampa (notebook).
fn has_lid() -> bool {
    fs::read_dir("/proc/acpi/button/lid").is_ok_and(|mut d| d.next().is_some())
        || crate::common::system::battery_path().is_some()
}

fn command_ok(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn index_of(list: &[u32], value: u32) -> i32 {
    list.iter().position(|v| *v == value).unwrap_or(0) as i32
}

/// Mostra as escolhas e liga os callbacks (Configurações).
fn install_settings(ui: &AppWindow) {
    let pw = ui.global::<Power>();
    let p = read_prefs();
    pw.set_blank(index_of(&BLANK_MINUTES, p.blank));
    pw.set_suspend_battery(index_of(&SUSPEND_MINUTES, p.suspend_battery));
    pw.set_suspend_ac(index_of(&SUSPEND_MINUTES, p.suspend_ac));
    pw.set_lock_on_blank(p.lock);
    pw.set_has_battery(crate::common::system::battery_path().is_some());
    pw.set_has_lid(has_lid());

    let (lid, lid_ac, button) = parse_logind(&fs::read_to_string(LOGIND_CONF).unwrap_or_default());
    let pos = |list: &[&str], v: &str| list.iter().position(|a| *a == v).unwrap_or(0) as i32;
    pw.set_lid_battery(pos(&LID_ACTIONS, &lid));
    pw.set_lid_ac(pos(&LID_ACTIONS, &lid_ac));
    pw.set_power_button(pos(&BUTTON_ACTIONS, &button));

    // Modo de energia (sem o power-profiles-daemon, a seção some). Numa
    // thread: o powerprofilesctl é um script Python e leva ~0,8 s para as duas
    // leituras; feitas aqui, seguravam a janela inteira das Configurações.
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let Some(list) = command_ok("powerprofilesctl", &["list"]) else { return };
        let current = command_ok("powerprofilesctl", &["get"]);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let pw = ui.global::<Power>();
            pw.set_profiles_available(true);
            pw.set_has_performance(list.contains("performance:"));
            if let Some(cur) = current {
                pw.set_profile(PROFILES.iter().position(|p| *p == cur.trim()).unwrap_or(1) as i32);
            }
        });
    });
    let weak = ui.as_weak();
    pw.on_set_profile(move |i| {
        let Some(name) = PROFILES.get(i.max(0) as usize) else { return };
        // Mostra a escolha na hora; o powerprofilesctl roda numa thread.
        let before = weak.upgrade().map(|ui| {
            let pw = ui.global::<Power>();
            let before = pw.get_profile();
            pw.set_profile(i);
            before
        });
        let weak = weak.clone();
        std::thread::spawn(move || {
            let ok = command_ok("powerprofilesctl", &["set", name]).is_some();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                let pw = ui.global::<Power>();
                if ok {
                    pw.set_message("".into());
                } else {
                    if let Some(b) = before {
                        pw.set_profile(b);
                    }
                    pw.set_message(tr("Could not change the power mode.", "Não deu para trocar o modo de energia.").into());
                }
            });
        });
    });

    let weak = ui.as_weak();
    pw.on_save(move || {
        let Some(ui) = weak.upgrade() else { return };
        let pw = ui.global::<Power>();
        let pick = |list: &[u32], i: i32| list.get(i.max(0) as usize).copied().unwrap_or(0);
        let p = Prefs {
            blank: pick(&BLANK_MINUTES, pw.get_blank()),
            suspend_battery: pick(&SUSPEND_MINUTES, pw.get_suspend_battery()),
            suspend_ac: pick(&SUSPEND_MINUTES, pw.get_suspend_ac()),
            lock: pw.get_lock_on_blank(),
        };
        if let Some(path) = config_path() {
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(path, p.to_text());
        }
    });

    let weak = ui.as_weak();
    pw.on_save_system(move || {
        let Some(ui) = weak.upgrade() else { return };
        let pw = ui.global::<Power>();
        let lid = LID_ACTIONS[pw.get_lid_battery().clamp(0, 3) as usize];
        let lid_ac = LID_ACTIONS[pw.get_lid_ac().clamp(0, 3) as usize];
        let button = BUTTON_ACTIONS[pw.get_power_button().clamp(0, 2) as usize];
        pw.set_system_busy(true);
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            let ok = Command::new("pkexec")
                .args(["/usr/local/sbin/kalango-energia", lid, lid_ac, button])
                .stdin(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                let pw = ui.global::<Power>();
                pw.set_system_busy(false);
                if ok {
                    pw.set_message(tr("Saved.", "Salvo.").into());
                } else {
                    // Cancelou a senha: volta ao que está no sistema.
                    let (lid, lid_ac, button) = parse_logind(&fs::read_to_string(LOGIND_CONF).unwrap_or_default());
                    let pos = |list: &[&str], v: &str| list.iter().position(|a| *a == v).unwrap_or(0) as i32;
                    pw.set_lid_battery(pos(&LID_ACTIONS, &lid));
                    pw.set_lid_ac(pos(&LID_ACTIONS, &lid_ac));
                    pw.set_power_button(pos(&BUTTON_ACTIONS, &button));
                    pw.set_message(tr("Not saved (no permission).", "Não foi salvo (sem permissão).").into());
                }
            });
        });
    });
}

/// `swayidle` que a área de trabalho rodou (o PID fica num arquivo: se a
/// área de trabalho reiniciar, a nova encerra o antigo).
fn pidfile() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join("kalango-swayidle.pid"))
}

fn kill_previous() {
    let Some(file) = pidfile() else { return };
    if let Some(pid) = fs::read_to_string(&file).ok().and_then(|s| s.trim().parse::<u32>().ok()) {
        let is_swayidle = fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim() == "swayidle");
        if is_swayidle {
            let _ = Command::new("kill").arg(pid.to_string()).status();
        }
    }
    let _ = fs::remove_file(file);
}

fn spawn_swayidle(args: &[String]) -> Option<Child> {
    let child = Command::new("swayidle")
        .args(args)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| eprintln!("kalango-shell: swayidle: {e}"))
        .ok()?;
    if let Some(file) = pidfile() {
        let _ = fs::write(file, child.id().to_string());
    }
    Some(child)
}

/// Mantém o `swayidle` com as escolhas (área de trabalho da sessão real).
fn install_idle() -> Timer {
    kill_previous();
    let mut current: Option<(Vec<String>, Child)> = None;
    // Argumentos com que o swayidle não abriu (não instalado): sem tentar de
    // novo a cada 3 s, só quando a escolha mudar.
    let mut failed: Option<Vec<String>> = None;
    let mut last_mtime: Option<SystemTime> = None;
    let mut prefs = read_prefs();
    let mut first = true;
    let timer = Timer::default();
    let mut check = move || {
        let mtime = config_path().and_then(|p| fs::metadata(p).ok()).and_then(|m| m.modified().ok());
        if mtime != last_mtime || first {
            last_mtime = mtime;
            prefs = read_prefs();
        }
        first = false;
        let args = swayidle_args(&prefs, on_ac());
        let dead = current.as_mut().is_some_and(|(_, c)| c.try_wait().ok().flatten().is_some());
        if !dead && current.as_ref().is_some_and(|(a, _)| *a == args) || failed.as_ref() == Some(&args) {
            return;
        }
        if let Some((_, mut c)) = current.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        current = spawn_swayidle(&args).map(|c| (args.clone(), c));
        failed = if current.is_none() { Some(args) } else { None };
    };
    check();
    timer.start(TimerMode::Repeated, Duration::from_secs(3), check);
    timer
}

/// Liga a página Energia (Configurações) e o apagar/suspender da tela
/// (área de trabalho). O timer devolvido precisa ficar vivo.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    match mode {
        Mode::Settings => {
            install_settings(ui);
            None
        }
        Mode::Desktop if crate::shell::session::is_native() => Some(install_idle()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escolhas_no_arquivo() {
        let p = Prefs { blank: 10, suspend_battery: 20, suspend_ac: 0, lock: false };
        assert_eq!(Prefs::parse(&p.to_text()), p);
        assert_eq!(Prefs::parse(""), Prefs::default());
        assert_eq!(Prefs::parse("apagar-tela=x\nbloquear=sim\n").blank, 5);
    }

    #[test]
    fn argumentos_do_swayidle() {
        let p = Prefs { blank: 5, suspend_battery: 15, suspend_ac: 0, lock: true };
        let bat = swayidle_args(&p, false).join(" | ");
        assert!(bat.contains("timeout | 300 | kalango-bloquear &"));
        assert!(bat.contains("timeout | 300 | wlopm --off '*' | resume | wlopm --on '*'"));
        assert!(bat.contains("timeout | 900 | systemctl suspend"));
        assert!(bat.contains("before-sleep | kalango-bloquear & sleep 1"));
        // Na tomada, "nunca" suspende.
        let ac = swayidle_args(&p, true).join(" | ");
        assert!(!ac.contains("systemctl suspend"));
        // Sem apagar nem bloquear.
        let none = swayidle_args(&Prefs { blank: 0, suspend_battery: 0, suspend_ac: 0, lock: false }, false).join(" ");
        assert!(!none.contains("wlopm --off") && !none.contains("before-sleep"));
    }

    #[test]
    fn logind() {
        assert_eq!(parse_logind(""), ("suspend".into(), "suspend".into(), "poweroff".into()));
        let t = "[Login]\nHandleLidSwitch=lock\nHandleLidSwitchExternalPower = ignore\nHandlePowerKey=suspend\n";
        assert_eq!(parse_logind(t), ("lock".into(), "ignore".into(), "suspend".into()));
    }
}
