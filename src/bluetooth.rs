//! Bluetooth (Configurações → Bluetooth e o botão do painel rápido), pelo
//! `bluetoothctl` do BlueZ.
//!
//! - Estado: `show` (ligado), `devices`, `devices Paired`, `devices Connected`
//!   e o `info` dos pareados (tipo do aparelho). Relido a cada 3 s.
//! - Procurar: `--timeout 12 scan on` numa thread; aparelhos sem nome (só o
//!   endereço) não aparecem, como no GNOME.
//! - Conectar um aparelho novo: uma sessão interativa do `bluetoothctl`
//!   (stdin/stdout) com agente `KeyboardDisplay`: `pair`, `trust`, `connect`.
//!   Se o aparelho pede código (teclado), o código aparece na tela; se pede
//!   para confirmar um número, o shell confirma (quem clicou em conectar foi a
//!   pessoa) e mostra o número.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};

use crate::device::output;
use crate::i18n::tr;
use crate::{AppWindow, BtDevice, Device, Mode};

/// Intervalo entre as leituras do estado.
const INTERVAL: Duration = Duration::from_secs(3);
/// Duração de uma busca por aparelhos.
const SCAN_SECONDS: &str = "12";
/// Tempo máximo de um pareamento + conexão.
const PAIR_TIMEOUT: Duration = Duration::from_secs(45);

/// Um aparelho visto pelo BlueZ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bt {
    pub mac: String,
    pub name: String,
    /// "audio", "input", "phone", "computer" ou "other".
    pub kind: String,
    pub paired: bool,
    pub connected: bool,
}

/// Tira as cores ANSI da saída do `bluetoothctl` (`\x1b[0;92m`...).
pub fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC [ ... letra
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        // O bluetoothctl interativo também usa \x01 e \x02 em volta do prompt.
        if c != '\x01' && c != '\x02' {
            out.push(c);
        }
    }
    out
}

/// Linhas `Device AA:BB:... Nome` (de `bluetoothctl devices`).
pub fn parse_devices(text: &str) -> Vec<(String, String)> {
    strip_ansi(text)
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("Device ")?;
            let (mac, name) = rest.split_once(' ').unwrap_or((rest, ""));
            (mac.len() == 17).then(|| (mac.to_string(), name.trim().to_string()))
        })
        .collect()
}

/// Nome de verdade? O BlueZ usa o endereço com hífens quando o aparelho não
/// anunciou nome.
pub fn has_real_name(mac: &str, name: &str) -> bool {
    !name.is_empty() && name != mac.replace(':', "-") && name != mac
}

/// `Powered: yes` de `bluetoothctl show` (`None` sem controlador).
pub fn parse_powered(show: &str) -> Option<bool> {
    let text = strip_ansi(show);
    if !text.contains("Controller ") {
        return None;
    }
    Some(text.lines().any(|l| l.trim() == "Powered: yes"))
}

/// Tipo do aparelho pelo `Icon:` do `bluetoothctl info`.
pub fn kind_from_info(info: &str) -> String {
    let icon = strip_ansi(info)
        .lines()
        .find_map(|l| l.trim().strip_prefix("Icon: ").map(str::to_string))
        .unwrap_or_default();
    let kind = if icon.starts_with("audio") {
        "audio"
    } else if icon.starts_with("input") {
        "input"
    } else if icon.starts_with("phone") {
        "phone"
    } else if icon.starts_with("computer") || icon.starts_with("video") {
        "computer"
    } else {
        "other"
    };
    kind.to_string()
}

/// Junta as listas: pareados primeiro (conectados no topo), depois os outros
/// com nome, em ordem alfabética.
pub fn merge(all: &[(String, String)], paired: &[(String, String)], connected: &[(String, String)], kinds: &[(String, String)]) -> Vec<Bt> {
    let is_in = |list: &[(String, String)], mac: &str| list.iter().any(|(m, _)| m == mac);
    let mut out: Vec<Bt> = all
        .iter()
        .filter(|(mac, name)| is_in(paired, mac) || has_real_name(mac, name))
        .map(|(mac, name)| Bt {
            mac: mac.clone(),
            name: if has_real_name(mac, name) { name.clone() } else { mac.clone() },
            kind: kinds.iter().find(|(m, _)| m == mac).map_or("other".into(), |(_, k)| k.clone()),
            paired: is_in(paired, mac),
            connected: is_in(connected, mac),
        })
        .collect();
    out.sort_by(|a, b| {
        b.paired
            .cmp(&a.paired)
            .then(b.connected.cmp(&a.connected))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

/// Estado atual: `None` sem BlueZ ou sem controlador.
fn read_state() -> Option<(bool, Vec<Bt>)> {
    let powered = parse_powered(&output("bluetoothctl", &["show"])?)?;
    let all = parse_devices(&output("bluetoothctl", &["devices"]).unwrap_or_default());
    let paired = parse_devices(&output("bluetoothctl", &["devices", "Paired"]).unwrap_or_default());
    let connected = if powered {
        parse_devices(&output("bluetoothctl", &["devices", "Connected"]).unwrap_or_default())
    } else {
        Vec::new()
    };
    // O tipo só dos pareados (poucos): um `info` por aparelho.
    let kinds: Vec<(String, String)> = paired
        .iter()
        .map(|(mac, _)| (mac.clone(), kind_from_info(&output("bluetoothctl", &["info", mac]).unwrap_or_default())))
        .collect();
    Some((powered, merge(&all, &paired, &connected, &kinds)))
}

/// Estado compartilhado entre a UI e as threads.
struct Shared {
    reading: AtomicBool,
    scanning: AtomicBool,
}

fn apply(ui: &AppWindow, state: Option<(bool, Vec<Bt>)>) {
    let d = ui.global::<Device>();
    let Some((powered, devices)) = state else {
        d.set_bt_available(false);
        return;
    };
    d.set_bt_available(true);
    d.set_bt_powered(powered);
    let connected = devices.iter().find(|b| b.connected).map(|b| b.name.clone()).unwrap_or_default();
    d.set_bt_connected(connected.into());
    let rows: Vec<BtDevice> = devices
        .into_iter()
        .map(|b| BtDevice {
            mac: b.mac.into(),
            name: b.name.into(),
            kind: b.kind.into(),
            paired: b.paired,
            connected: b.connected,
        })
        .collect();
    d.set_bt_devices(ModelRc::from(Rc::new(VecModel::from(rows))));
}

/// Relê o estado numa thread e aplica na UI.
fn refresh(weak: Weak<AppWindow>, shared: Arc<Shared>) {
    if shared.reading.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let state = read_state();
        shared.reading.store(false, Ordering::SeqCst);
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                apply(&ui, state);
            }
        });
    });
}

/// Mostra uma mensagem e relê (chamado de outra thread).
fn finish(weak: &Weak<AppWindow>, shared: &Arc<Shared>, message: String) {
    let weak = weak.clone();
    let shared = shared.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            let d = ui.global::<Device>();
            d.set_bt_busy_mac(SharedString::new());
            d.set_bt_message(message.into());
        }
        refresh(weak, shared);
    });
}

/// Mensagem durante o pareamento (código para digitar ou confirmar).
fn notify(weak: &Weak<AppWindow>, message: String) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            ui.global::<Device>().set_bt_message(message.into());
        }
    });
}

/// Procura aparelhos por alguns segundos.
fn scan(weak: Weak<AppWindow>, shared: Arc<Shared>) {
    if shared.scanning.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Some(ui) = weak.upgrade() {
        ui.global::<Device>().set_bt_scanning(true);
    }
    std::thread::spawn(move || {
        let _ = output("bluetoothctl", &["--timeout", SCAN_SECONDS, "scan", "on"]);
        shared.scanning.store(false, Ordering::SeqCst);
        let weak2 = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak2.upgrade() {
                ui.global::<Device>().set_bt_scanning(false);
            }
        });
        let _ = slint::invoke_from_event_loop(move || refresh(weak, shared));
    });
}

/// Código que o aparelho pede, numa linha do agente do `bluetoothctl`:
/// `("type", "123456")` para digitar no aparelho, `("confirm", "123456")`
/// para conferir nos dois lados.
pub fn agent_prompt(line: &str) -> Option<(&'static str, String)> {
    let line = strip_ansi(line);
    let code = |after: &str| -> Option<String> {
        let rest = line.split(after).nth(1)?;
        let digits: String = rest.trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
        (!digits.is_empty()).then_some(digits)
    };
    if line.contains("Confirm passkey") {
        return code("Confirm passkey").map(|c| ("confirm", c));
    }
    if line.contains("Passkey:") {
        return code("Passkey:").map(|c| ("type", c));
    }
    if line.contains("PIN code:") {
        return code("PIN code:").map(|c| ("type", c));
    }
    None
}

/// Pareia (se preciso), confia e conecta, numa sessão interativa do
/// `bluetoothctl`. Devolve a mensagem final.
fn pair_and_connect(weak: &Weak<AppWindow>, mac: &str, name: &str, paired: bool) -> String {
    let fail = || format!("{} {name}", tr("Could not connect to", "Não foi possível conectar a"));
    let Ok(mut child) = Command::new("bluetoothctl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return fail();
    };
    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut send = |cmd: &str| {
        let _ = stdin.write_all(format!("{cmd}\n").as_bytes());
        let _ = stdin.flush();
    };
    send("agent KeyboardDisplay");
    send("default-agent");
    let deadline = Instant::now() + PAIR_TIMEOUT;
    // Espera uma das respostas (`ok` ou `err`), atendendo o agente no meio.
    let wait = |ok: &[&str], err: &[&str], send: &mut dyn FnMut(&str)| -> bool {
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            let Ok(line) = rx.recv_timeout(left) else { return false };
            let clean = strip_ansi(&line);
            if let Some((what, code)) = agent_prompt(&clean) {
                if what == "confirm" {
                    send("yes");
                    notify(weak, format!("{} {code}", tr("Pairing code:", "Código de pareamento:")));
                } else {
                    notify(
                        weak,
                        format!(
                            "{} {code} {}",
                            tr("Type", "Digite"),
                            tr("on the device and press Enter", "no aparelho e aperte Enter")
                        ),
                    );
                }
                continue;
            }
            if clean.contains("Authorize service") || clean.contains("(yes/no)") {
                send("yes");
                continue;
            }
            if ok.iter().any(|s| clean.contains(s)) {
                return true;
            }
            if err.iter().any(|s| clean.contains(s)) {
                return false;
            }
        }
        false
    };

    let mut result = true;
    if !paired {
        send(&format!("pair {mac}"));
        result = wait(&["Pairing successful", "AlreadyExists"], &["Failed to pair", "not available"], &mut send);
        if result {
            send(&format!("trust {mac}"));
            wait(&["trust succeeded"], &["Failed to set trusted", "not available"], &mut send);
        }
    }
    if result {
        send(&format!("connect {mac}"));
        result = wait(&["Connection successful"], &["Failed to connect", "not available"], &mut send);
    }
    send("quit");
    let _ = child.wait();
    if result {
        format!("{} {name}", tr("Connected to", "Conectado a"))
    } else {
        fail()
    }
}

/// Liga o Bluetooth às Configurações e ao painel rápido (sessão real).
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if !matches!(mode, Mode::Settings | Mode::QuickSettings) {
        return None;
    }
    let d = ui.global::<Device>();
    d.on_bt_paired_count(|list| list.iter().filter(|b| b.paired).count() as i32);
    let present = std::fs::read_dir("/sys/class/bluetooth").is_ok_and(|mut r| r.next().is_some());
    d.set_bluetooth_present(present);
    if !present || !crate::session::is_native() {
        return None;
    }
    let shared = Arc::new(Shared { reading: AtomicBool::new(false), scanning: AtomicBool::new(false) });

    let weak = ui.as_weak();
    let sh = shared.clone();
    d.on_set_bt(move |on| {
        let weak = weak.clone();
        let sh = sh.clone();
        std::thread::spawn(move || {
            let ok = output("bluetoothctl", &["power", if on { "on" } else { "off" }]).is_some();
            let msg = if ok {
                String::new()
            } else {
                tr("Could not change Bluetooth (blocked?)", "Não foi possível mudar o Bluetooth (bloqueado?)").into()
            };
            finish(&weak, &sh, msg);
        });
    });

    let weak = ui.as_weak();
    let sh = shared.clone();
    d.on_bt_scan(move || scan(weak.clone(), sh.clone()));

    let weak = ui.as_weak();
    let sh = shared.clone();
    d.on_bt_connect(move |mac, name, paired| {
        let Some(ui) = weak.upgrade() else { return };
        let d = ui.global::<Device>();
        if !d.get_bt_busy_mac().is_empty() {
            return;
        }
        d.set_bt_busy_mac(mac.clone());
        d.set_bt_message(format!("{} {name}...", tr("Connecting to", "Conectando a")).into());
        let weak = weak.clone();
        let sh = sh.clone();
        std::thread::spawn(move || {
            let msg = pair_and_connect(&weak, &mac, &name, paired);
            finish(&weak, &sh, msg);
        });
    });

    let weak = ui.as_weak();
    let sh = shared.clone();
    d.on_bt_disconnect(move |mac| {
        let weak = weak.clone();
        let sh = sh.clone();
        std::thread::spawn(move || {
            let _ = output("bluetoothctl", &["disconnect", &mac]);
            finish(&weak, &sh, String::new());
        });
    });

    let weak = ui.as_weak();
    let sh = shared.clone();
    d.on_bt_forget(move |mac| {
        let weak = weak.clone();
        let sh = sh.clone();
        std::thread::spawn(move || {
            let ok = output("bluetoothctl", &["remove", &mac]).is_some();
            let msg = if ok { String::new() } else { tr("Could not forget the device", "Não foi possível esquecer o aparelho").into() };
            finish(&weak, &sh, msg);
        });
    });

    // Painel rápido: lê antes de a janela aparecer (como o Wi-Fi, ver
    // device::install), para o botão do Bluetooth já abrir no estado certo.
    if mode == Mode::QuickSettings && Path::new("/usr/bin/bluetoothctl").exists() {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(read_state());
        });
        if let Ok(state) = rx.recv_timeout(Duration::from_millis(500)) {
            apply(ui, state);
        }
    }
    refresh(ui.as_weak(), shared.clone());
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, INTERVAL, move || refresh(weak.clone(), shared.clone()));
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_aparelhos_e_esconde_os_sem_nome() {
        let all = parse_devices(
            "Device 3A:1E:21:41:1E:74 3A-1E-21-41-1E-74\n\
             Device 24:FC:E5:7E:AE:EB [TV] Samsung 7 Series (50)\n\
             \x1b[0;92mDevice 11:22:33:44:55:66 Fone JBL\x1b[0m\n",
        );
        assert_eq!(all.len(), 3);
        assert_eq!(all[2], ("11:22:33:44:55:66".to_string(), "Fone JBL".to_string()));
        let paired = vec![("11:22:33:44:55:66".to_string(), "Fone JBL".to_string())];
        let kinds = vec![("11:22:33:44:55:66".to_string(), "audio".to_string())];
        let list = merge(&all, &paired, &paired, &kinds);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Fone JBL");
        assert!(list[0].paired && list[0].connected);
        assert_eq!(list[0].kind, "audio");
        assert_eq!(list[1].name, "[TV] Samsung 7 Series (50)");
    }

    #[test]
    fn le_ligado_e_tipo() {
        assert_eq!(parse_powered("Controller A0:88:69:CC:F8:AC (public)\n\tPowered: yes\n"), Some(true));
        assert_eq!(parse_powered("Controller A0:88 (public)\n\tPowered: no\n"), Some(false));
        assert_eq!(parse_powered("No default controller available\n"), None);
        assert_eq!(kind_from_info("\tIcon: audio-headset\n"), "audio");
        assert_eq!(kind_from_info("\tIcon: input-keyboard\n"), "input");
        assert_eq!(kind_from_info(""), "other");
    }

    #[test]
    fn le_codigo_do_agente() {
        assert_eq!(agent_prompt("[agent] Passkey: 123456"), Some(("type", "123456".to_string())));
        assert_eq!(
            agent_prompt("\x1b[0;94m[agent]\x1b[0m Confirm passkey 004211 (yes/no): "),
            Some(("confirm", "004211".to_string()))
        );
        assert_eq!(agent_prompt("[agent] PIN code: 0000"), Some(("type", "0000".to_string())));
        assert_eq!(agent_prompt("Pairing successful"), None);
    }
}
