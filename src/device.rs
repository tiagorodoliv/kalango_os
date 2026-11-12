//! Dados reais do computador para as Configurações (global `Device`).
//!
//! - Rede e Wi-Fi: NetworkManager pelo `nmcli` (lista, liga/desliga, conecta).
//! - Som: PipeWire/WirePlumber pelo `wpctl` (volume e mudo da saída padrão).
//! - Energia: bateria do `/sys`, suspender/reiniciar pelo `systemctl`.
//! - Sobre o sistema: `/etc/os-release`, `/proc` e `df`.
//!
//! Os comandos rodam numa thread à parte (o `nmcli` pode levar segundos) e o
//! resultado volta para a UI com `slint::invoke_from_event_loop`. Fora do
//! Kalango OS (desenvolvimento), comando que não existe só deixa a página no
//! estado "indisponível".

use std::fs;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};

use crate::{AppWindow, Device, InfoItem, Mode, WifiNetwork};

/// Intervalo entre as leituras de rede, som e bateria (só nas Configurações).
const INTERVAL: Duration = Duration::from_secs(3);
/// Quantas redes Wi-Fi mostrar.
const MAX_NETWORKS: usize = 8;
/// Depois de mexer no volume, por quanto tempo a leitura periódica não
/// sobrescreve o slider (evita o puxador "pular" durante o arraste).
const VOLUME_HOLD_MS: u64 = 2500;

// ---------------------------------------------------------------------------
// Leitura de texto (funções puras, testadas)
// ---------------------------------------------------------------------------

/// Separa uma linha do modo "terse" do `nmcli` (`-t`): campos por `:`, com
/// `\:` e `\\` escapados dentro dos valores.
pub fn split_terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    fields.last_mut().unwrap().push(next);
                }
            }
            ':' => fields.push(String::new()),
            _ => fields.last_mut().unwrap().push(c),
        }
    }
    fields
}

/// Uma rede da lista do `nmcli`.
#[derive(Clone, Debug, PartialEq)]
pub struct Net {
    pub ssid: String,
    pub signal: i32,
    pub secure: bool,
    pub active: bool,
}

/// Lê a saída de `nmcli -t -f IN-USE,SSID,SIGNAL,SECURITY device wifi list`.
///
/// Junta redes repetidas (vários pontos de acesso com o mesmo nome) ficando
/// com o sinal mais forte, ignora redes ocultas e ordena: a conectada
/// primeiro, depois pelo sinal.
pub fn parse_wifi_list(text: &str) -> Vec<Net> {
    let mut nets: Vec<Net> = Vec::new();
    for line in text.lines() {
        let f = split_terse(line);
        if f.len() < 4 || f[1].is_empty() {
            continue;
        }
        let net = Net {
            ssid: f[1].clone(),
            signal: f[2].parse().unwrap_or(0),
            secure: !f[3].is_empty() && f[3] != "--",
            active: f[0] == "*",
        };
        match nets.iter_mut().find(|n| n.ssid == net.ssid) {
            Some(n) => {
                n.signal = n.signal.max(net.signal);
                n.active |= net.active;
            }
            None => nets.push(net),
        }
    }
    nets.sort_by(|a, b| b.active.cmp(&a.active).then(b.signal.cmp(&a.signal)));
    nets.truncate(MAX_NETWORKS);
    nets
}

/// Resumo dos dispositivos de rede, de `nmcli -t -f TYPE,STATE,CONNECTION device`.
#[derive(Clone, Debug, PartialEq)]
pub struct NetStatus {
    /// Frase para a tela ("Conectado por cabo", "Conectado a Casa"...).
    pub summary: String,
    /// Existe uma placa Wi-Fi.
    pub wifi_present: bool,
}

pub fn parse_devices(text: &str) -> NetStatus {
    let mut wired = None;
    let mut wifi = None;
    let mut wifi_present = false;
    for line in text.lines() {
        let f = split_terse(line);
        if f.len() < 3 {
            continue;
        }
        let connected = f[1] == "connected" || f[1].starts_with("connected ");
        match f[0].as_str() {
            "wifi" => {
                wifi_present = true;
                if connected {
                    wifi = Some(f[2].clone());
                }
            }
            "ethernet" if connected => wired = Some(f[2].clone()),
            _ => {}
        }
    }
    let summary = match (wifi, wired) {
        (Some(ssid), _) => format!("Conectado ao Wi-Fi {ssid}"),
        (None, Some(_)) => "Conectado por cabo".to_string(),
        (None, None) => "Sem conexão".to_string(),
    };
    NetStatus { summary, wifi_present }
}

/// Lê `wpctl get-volume`: "Volume: 0.40" ou "Volume: 0.40 [MUTED]".
pub fn parse_wpctl_volume(text: &str) -> Option<(f32, bool)> {
    let rest = text.trim().strip_prefix("Volume:")?;
    let value: f32 = rest.split_whitespace().next()?.parse().ok()?;
    Some((value.clamp(0.0, 1.0), rest.contains("[MUTED]")))
}

/// Nome da saída em `wpctl inspect` (linha `node.description = "..."`).
pub fn parse_wpctl_description(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let l = l.trim().trim_start_matches('*').trim();
        let v = l.strip_prefix("node.description = ")?;
        Some(v.trim_matches('"').to_string())
    })
}

/// Valor de uma chave de `/etc/os-release` (sem aspas).
pub fn os_release_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let v = l.strip_prefix(key)?.strip_prefix('=')?;
        Some(v.trim_matches('"').to_string())
    })
}

/// Nome do processador em `/proc/cpuinfo`, sem "(R)"/"(TM)" e espaços extras.
pub fn cpu_model(cpuinfo: &str) -> Option<String> {
    let raw = cpuinfo
        .lines()
        .find_map(|l| l.strip_prefix("model name")?.split_once(':').map(|(_, v)| v.to_string()))?;
    let clean = raw.replace("(R)", "").replace("(TM)", "").replace("(tm)", "");
    Some(clean.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Memória total em bytes, da linha `MemTotal:` de `/proc/meminfo` (em kB).
pub fn mem_total(meminfo: &str) -> Option<u64> {
    let kb: u64 = meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(kb * 1024)
}

/// Tamanho legível em pt-BR, com vírgula decimal: "3,8 GB", "512 MB".
pub fn human_size(bytes: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB).replace('.', ",")
    } else {
        format!("{} MB", (b / 1_000_000.0).round() as u64)
    }
}

/// Tamanho e espaço livre de `df -B1 --output=size,avail /`.
pub fn parse_df(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().nth(1)?;
    let mut it = line.split_whitespace().map(|v| v.parse::<u64>());
    Some((it.next()?.ok()?, it.next()?.ok()?))
}

/// Saída de `id -Gn`: o usuário está no grupo `sudo` (Debian) ou `wheel`.
pub fn is_admin(groups: &str) -> bool {
    groups.split_whitespace().any(|g| g == "sudo" || g == "wheel")
}

// ---------------------------------------------------------------------------
// Comandos
// ---------------------------------------------------------------------------

/// Roda um comando e devolve a saída padrão (`None` se não existe ou falhou).
///
/// O `nmcli` roda com `LC_ALL=C`: os estados ("connected") viriam traduzidos
/// e a leitura depende deles. O `wpctl` mantém o idioma da sessão, porque o
/// nome da saída de som aparece na tela.
fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    if program == "nmcli" {
        cmd.env("LC_ALL", "C");
    }
    let out = cmd
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Inicia um comando sem esperar (uma thread recolhe o processo ao terminar).
fn spawn_detached(program: &str, args: &[&str]) {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
    if let Some(home) = std::env::var_os("HOME") {
        cmd.current_dir(home);
    }
    match cmd.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(err) => eprintln!("kalango-shell: falha ao rodar {program}: {err}"),
    }
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

// ---------------------------------------------------------------------------
// Sobre o sistema
// ---------------------------------------------------------------------------

/// Ficha do sistema (lida uma vez, ao abrir).
fn about_items() -> Vec<(String, String)> {
    let mut items = vec![("Versão".to_string(), format!("Kalango OS {}", env!("CARGO_PKG_VERSION")))];
    let read = |p: &str| fs::read_to_string(p).ok();

    if let Some(name) = read("/etc/os-release").and_then(|t| os_release_value(&t, "PRETTY_NAME")) {
        items.push(("Base".into(), name));
    }
    if let Some(k) = read("/proc/sys/kernel/osrelease") {
        items.push(("Kernel".into(), format!("Linux {}", k.trim())));
    }
    let vendor = read("/sys/class/dmi/id/sys_vendor").unwrap_or_default();
    let product = read("/sys/class/dmi/id/product_name").unwrap_or_default();
    let machine = format!("{} {}", vendor.trim(), product.trim()).trim().to_string();
    if !machine.is_empty() {
        items.push(("Computador".into(), machine));
    }
    if let Some(model) = read("/proc/cpuinfo").and_then(|t| cpu_model(&t)) {
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        let label = if cores == 1 { "núcleo" } else { "núcleos" };
        items.push(("Processador".into(), format!("{model} · {cores} {label}")));
    }
    if let Some(mem) = read("/proc/meminfo").and_then(|t| mem_total(&t)) {
        items.push(("Memória".into(), human_size(mem)));
    }
    if let Some((size, avail)) = output("df", &["-B1", "--output=size,avail", "/"]).and_then(|t| parse_df(&t)) {
        items.push(("Disco".into(), format!("{} livres de {}", human_size(avail), human_size(size))));
    }
    if let Some(host) = read("/proc/sys/kernel/hostname") {
        items.push(("Nome do computador".into(), host.trim().to_string()));
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        items.push(("Sessão".into(), "Wayland".into()));
    }
    items
}

// ---------------------------------------------------------------------------
// Leitura periódica (thread) e aplicação na UI
// ---------------------------------------------------------------------------

/// Tudo o que a leitura periódica descobre.
#[derive(Default)]
struct Snapshot {
    network: Option<NetStatus>,
    wifi_enabled: bool,
    networks: Vec<Net>,
    sound: Option<(f32, bool, String)>,
    battery: Option<(u32, bool)>,
}

fn read_snapshot(rescan: bool) -> Snapshot {
    let mut s = Snapshot::default();
    if let Some(dev) = output("nmcli", &["-t", "-f", "TYPE,STATE,CONNECTION", "device"]) {
        let status = parse_devices(&dev);
        s.wifi_enabled = output("nmcli", &["radio", "wifi"]).is_some_and(|t| t.trim() == "enabled");
        if status.wifi_present && s.wifi_enabled {
            // "auto": o NetworkManager só procura de novo se a última busca
            // tem mais de 30 s (conectado, ele quase não procura sozinho).
            let rescan = if rescan { "yes" } else { "auto" };
            if let Some(list) = output(
                "nmcli",
                &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", rescan],
            ) {
                s.networks = parse_wifi_list(&list);
            }
        }
        s.network = Some(status);
    }
    if let Some((vol, muted)) =
        output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]).and_then(|t| parse_wpctl_volume(&t))
    {
        let name = output("wpctl", &["inspect", "@DEFAULT_AUDIO_SINK@"])
            .and_then(|t| parse_wpctl_description(&t))
            .unwrap_or_else(|| "Saída padrão".into());
        s.sound = Some((vol, muted, name));
    }
    if let Some(path) = crate::system::battery_path() {
        let cap = fs::read_to_string(path.join("capacity")).ok().and_then(|c| c.trim().parse().ok());
        let charging = fs::read_to_string(path.join("status"))
            .is_ok_and(|t| matches!(t.trim(), "Charging" | "Full"));
        s.battery = cap.map(|c| (c, charging));
    }
    s
}

/// Estado compartilhado entre a UI e as threads de leitura.
struct Shared {
    /// Uma leitura em andamento (não empilhar outra).
    reading: AtomicBool,
    /// Momento (ms) em que o usuário mexeu no volume pela última vez.
    volume_touched: AtomicU64,
}

/// Faz uma leitura numa thread e aplica o resultado na UI.
fn refresh(weak: Weak<AppWindow>, shared: Arc<Shared>, rescan: bool) {
    if shared.reading.swap(true, Ordering::SeqCst) {
        return;
    }
    if rescan {
        set_busy(&weak, true);
    }
    std::thread::spawn(move || {
        let snap = read_snapshot(rescan);
        let hold_volume = now_ms().saturating_sub(shared.volume_touched.load(Ordering::SeqCst)) < VOLUME_HOLD_MS;
        shared.reading.store(false, Ordering::SeqCst);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            apply(&ui, snap, hold_volume, rescan);
        });
    });
}

fn set_busy(weak: &Weak<AppWindow>, busy: bool) {
    if let Some(ui) = weak.upgrade() {
        ui.global::<Device>().set_network_busy(busy);
    }
}

fn apply(ui: &AppWindow, s: Snapshot, hold_volume: bool, rescan: bool) {
    let d = ui.global::<Device>();
    match s.network {
        Some(net) => {
            d.set_network_available(true);
            d.set_network_status(net.summary.into());
            d.set_wifi_present(net.wifi_present);
            d.set_wifi_enabled(s.wifi_enabled);
        }
        None => d.set_network_available(false),
    }
    let nets: Vec<WifiNetwork> = s
        .networks
        .into_iter()
        .map(|n| WifiNetwork { ssid: n.ssid.into(), signal: n.signal, secure: n.secure, active: n.active })
        .collect();
    d.set_networks(ModelRc::from(Rc::new(VecModel::from(nets))));
    if rescan {
        d.set_network_busy(false);
    }

    match s.sound {
        Some((vol, muted, name)) => {
            d.set_sound_available(true);
            d.set_sound_output(name.into());
            if !hold_volume {
                d.set_volume(vol);
                d.set_muted(muted);
            }
        }
        None => d.set_sound_available(false),
    }

    match s.battery {
        Some((cap, charging)) => {
            d.set_battery(format!("{cap}%").into());
            d.set_charging(charging);
        }
        None => d.set_battery("Sem bateria".into()),
    }
}

/// Conecta a uma rede Wi-Fi numa thread e mostra o resultado.
///
/// A senha vai como argumento do `nmcli` (fica visível na lista de processos
/// por instantes, só para o próprio computador); o NetworkManager a guarda
/// para as próximas vezes.
fn connect(weak: Weak<AppWindow>, shared: Arc<Shared>, ssid: SharedString, password: SharedString) {
    if let Some(ui) = weak.upgrade() {
        let d = ui.global::<Device>();
        d.set_network_busy(true);
        d.set_network_message(format!("Conectando a {ssid}...").into());
    }
    std::thread::spawn(move || {
        let mut args = vec!["device", "wifi", "connect", ssid.as_str()];
        if !password.is_empty() {
            args.extend(["password", password.as_str()]);
            // Um perfil antigo com o mesmo nome (ex.: de uma tentativa com a
            // senha errada) faz o `nmcli` recusar a senha nova com
            // "key-mgmt: property is missing". Senha digitada = perfil novo.
            forget_network(&ssid);
        }
        let ok = output("nmcli", &args).is_some();
        if !ok && !password.is_empty() {
            // Não deixa salvo um perfil com a senha errada.
            forget_network(&ssid);
        }
        let message = if ok {
            format!("Conectado a {ssid}")
        } else if password.is_empty() {
            format!("Não foi possível conectar a {ssid}")
        } else {
            format!("Não foi possível conectar a {ssid}. Confira a senha.")
        };
        let weak2 = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak2.upgrade() {
                let d = ui.global::<Device>();
                d.set_network_busy(false);
                d.set_network_message(message.into());
            }
        });
        refresh_from_thread(weak, shared);
    });
}

/// Apaga os perfis salvos do NetworkManager com o nome da rede (se houver).
fn forget_network(ssid: &str) {
    let _ = output("nmcli", &["connection", "delete", "id", ssid]);
}

/// Pede uma leitura a partir de outra thread (a leitura começa no laço da UI).
fn refresh_from_thread(weak: Weak<AppWindow>, shared: Arc<Shared>) {
    let _ = slint::invoke_from_event_loop(move || refresh(weak, shared, false));
}

/// Liga o global `Device` ao sistema.
///
/// Preenche "Sobre o sistema" e o login em todos os modos com tela de
/// Configurações; a leitura periódica (rede, som, bateria) só roda no
/// processo das Configurações (`--settings`). O timer devolvido precisa
/// ficar vivo.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if matches!(mode, Mode::Launcher | Mode::Greeter) {
        return None;
    }
    let d = ui.global::<Device>();
    let about: Vec<InfoItem> = about_items()
        .into_iter()
        .map(|(label, value)| InfoItem { label: label.into(), value: value.into() })
        .collect();
    d.set_about(ModelRc::from(Rc::new(VecModel::from(about))));
    if let Ok(user) = std::env::var("USER") {
        d.set_login(user.into());
    }
    if let Some(groups) = output("id", &["-Gn"]) {
        d.set_admin(is_admin(&groups));
    }
    d.set_bluetooth_present(fs::read_dir("/sys/class/bluetooth").is_ok_and(|mut r| r.next().is_some()));

    let native = crate::session::is_native();
    d.on_power_action(move |action| {
        let verb = match action.as_str() {
            "suspend" => "suspend",
            "reboot" => "reboot",
            _ => return,
        };
        if native {
            spawn_detached("systemctl", &[verb]);
        } else {
            eprintln!("kalango-shell: protótipo, ignorando systemctl {verb}");
        }
    });
    d.on_change_password(|| {
        spawn_detached(
            "foot",
            &[
                "--title=Alterar senha",
                "sh",
                "-c",
                "passwd; printf '\\nEnter para fechar'; read x",
            ],
        );
    });

    if mode != Mode::Settings {
        return None;
    }

    let shared = Arc::new(Shared {
        reading: AtomicBool::new(false),
        volume_touched: AtomicU64::new(0),
    });

    {
        let shared = shared.clone();
        d.on_set_volume(move |v| {
            shared.volume_touched.store(now_ms(), Ordering::SeqCst);
            let v = format!("{:.2}", v.clamp(0.0, 1.0));
            spawn_detached("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &v]);
        });
    }
    {
        let shared = shared.clone();
        d.on_set_muted(move |m| {
            shared.volume_touched.store(now_ms(), Ordering::SeqCst);
            spawn_detached("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", if m { "1" } else { "0" }]);
        });
    }
    {
        let weak = ui.as_weak();
        let shared = shared.clone();
        d.on_set_wifi(move |on| {
            let weak = weak.clone();
            let shared = shared.clone();
            std::thread::spawn(move || {
                let _ = output("nmcli", &["radio", "wifi", if on { "on" } else { "off" }]);
                refresh_from_thread(weak, shared);
            });
        });
    }
    {
        let weak = ui.as_weak();
        let shared = shared.clone();
        d.on_rescan(move || refresh(weak.clone(), shared.clone(), true));
    }
    {
        let weak = ui.as_weak();
        let shared = shared.clone();
        d.on_connect_wifi(move |ssid, password| connect(weak.clone(), shared.clone(), ssid, password));
    }

    let weak = ui.as_weak();
    // Ao abrir, procura redes de verdade: a lista guardada pode estar velha.
    refresh(weak.clone(), shared.clone(), true);
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, INTERVAL, move || refresh(weak.clone(), shared.clone(), false));
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separa_campos_do_nmcli_com_escape() {
        assert_eq!(split_terse(r"*:Casa\: 5G:80:WPA2"), ["*", "Casa: 5G", "80", "WPA2"]);
        assert_eq!(split_terse(r"a\\b:"), ["a\\b", ""]);
    }

    #[test]
    fn lista_wifi_junta_repetidas_e_ordena() {
        let text = " :Vizinho:40:WPA2\n*:Casa:70:WPA2\n :Casa:75:WPA2\n ::90:WPA2\n :Cafe:60:\n";
        let nets = parse_wifi_list(text);
        let names: Vec<_> = nets.iter().map(|n| n.ssid.as_str()).collect();
        assert_eq!(names, ["Casa", "Cafe", "Vizinho"]);
        assert_eq!(nets[0].signal, 75);
        assert!(nets[0].active && nets[0].secure);
        assert!(!nets[1].secure);
    }

    #[test]
    fn resume_dispositivos() {
        let s = parse_devices("ethernet:connected:Wired connection 1\nloopback:connected (externally):lo\n");
        assert_eq!(s, NetStatus { summary: "Conectado por cabo".into(), wifi_present: false });
        let s = parse_devices("wifi:connected:Casa\nethernet:unavailable:\n");
        assert_eq!(s.summary, "Conectado ao Wi-Fi Casa");
        assert!(s.wifi_present);
        assert_eq!(parse_devices("wifi:disconnected:\n").summary, "Sem conexão");
    }

    #[test]
    fn le_volume_do_wpctl() {
        assert_eq!(parse_wpctl_volume("Volume: 0.40\n"), Some((0.40, false)));
        assert_eq!(parse_wpctl_volume("Volume: 1.20 [MUTED]\n"), Some((1.0, true)));
        assert_eq!(parse_wpctl_volume("erro"), None);
        let inspect = "id 52, type PipeWire:Interface:Node\n  * node.description = \"Áudio interno\"\n";
        assert_eq!(parse_wpctl_description(inspect).as_deref(), Some("Áudio interno"));
    }

    #[test]
    fn reconhece_administrador() {
        assert!(is_admin("user cdrom sudo audio\n"));
        assert!(!is_admin("user cdrom sudoers audio\n"));
    }

    #[test]
    fn le_ficha_do_sistema() {
        let os = "NAME=\"Debian GNU/Linux\"\nPRETTY_NAME=\"Debian GNU/Linux 13 (trixie)\"\n";
        assert_eq!(os_release_value(os, "PRETTY_NAME").as_deref(), Some("Debian GNU/Linux 13 (trixie)"));
        let cpu = "processor : 0\nmodel name\t: Intel(R) Core(TM)  i5-8250U CPU @ 1.60GHz\n";
        assert_eq!(cpu_model(cpu).as_deref(), Some("Intel Core i5-8250U CPU @ 1.60GHz"));
        assert_eq!(mem_total("MemTotal:        4000000 kB\n"), Some(4_096_000_000));
        assert_eq!(human_size(4_096_000_000), "4,1 GB");
        assert_eq!(human_size(512_000_000), "512 MB");
        assert_eq!(parse_df("1B-blocks Avail\n20000000000 12345000000\n"), Some((20_000_000_000, 12_345_000_000)));
    }
}
