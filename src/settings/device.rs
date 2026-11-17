//! Dados reais do computador para as Configurações (global `Device`).
//!
//! - Rede e Wi-Fi: NetworkManager pelo `nmcli` (lista, liga/desliga, conecta).
//! - Som: PipeWire/WirePlumber pelo `wpctl` (saída e entrada padrão, volume e mudo).
//! - Energia: bateria do `/sys`, suspender/reiniciar pelo `systemctl`.
//! - Brilho da tela: `/sys/class/backlight`, ajustado pelo logind
//!   (`SetBrightness`, sem precisar de root).
//! - Sobre o sistema: `/etc/os-release`, `/proc` e `df`.
//!
//! Os comandos rodam numa thread à parte (o `nmcli` pode levar segundos) e o
//! resultado volta para a UI com `slint::invoke_from_event_loop`. Fora do
//! KalangOS (desenvolvimento), comando que não existe só deixa a página no
//! estado "indisponível".

use crate::common::i18n::tr;
use std::fs;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel, Weak};

use crate::{AppWindow, Device, InfoItem, Mode, NetConnection, SoundDevice, VpnConnection, WifiNetwork};

/// Intervalo entre as leituras de rede, som e bateria (Configurações e painel rápido).
const INTERVAL: Duration = Duration::from_secs(3);
/// Quanto o painel rápido espera a primeira leitura antes de aparecer.
const FIRST_READ_WAIT: Duration = Duration::from_millis(600);
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

/// Nomes dos perfis Wi-Fi salvos, de `nmcli -t -f NAME,TYPE connection show`.
///
/// O `nmcli device wifi connect` (Configurações, painel, nmtui) dá ao perfil
/// o nome da rede, então o nome serve para achar a rede salva na lista.
pub fn parse_saved_wifi(text: &str) -> Vec<String> {
    text.lines()
        .map(split_terse)
        .filter(|f| f.len() >= 2 && f[1] == "802-11-wireless")
        .map(|f| f[0].clone())
        .collect()
}

/// Existe uma placa Wi-Fi (`/sys/class/net/<placa>/wireless`).
fn has_wifi_card() -> bool {
    fs::read_dir("/sys/class/net")
        .map(|dir| dir.flatten().any(|e| e.path().join("wireless").exists()))
        .unwrap_or(false)
}

/// Rádio Wi-Fi ligado, pelo rfkill (o NetworkManager bloqueia o rádio ao
/// desligar o Wi-Fi). Só a primeira imagem do painel; o nmcli confirma logo.
fn wifi_radio_on() -> bool {
    let Ok(dir) = fs::read_dir("/sys/class/rfkill") else { return true };
    let read = |p: std::path::PathBuf| fs::read_to_string(p).unwrap_or_default().trim().to_string();
    dir.flatten()
        .filter(|e| read(e.path().join("type")) == "wlan")
        .all(|e| read(e.path().join("soft")) == "0" && read(e.path().join("hard")) == "0")
}

/// Resumo dos dispositivos de rede, de
/// `nmcli -t -f TYPE,STATE,CONNECTION,DEVICE device`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetStatus {
    /// Frase para a tela ("Conectado por cabo", "Conectado a Casa"...).
    pub summary: String,
    /// Existe uma placa Wi-Fi com driver (o NetworkManager a gerencia).
    pub wifi_present: bool,
    /// Existe uma placa de rede cabeada gerenciada.
    pub wired_present: bool,
    /// O cabo está conectado (com conexão ativa).
    pub wired_connected: bool,
    /// Sem cabo na placa (estado "unavailable").
    pub wired_unplugged: bool,
    /// Nome da conexão cabeada ativa ("Wired connection 1").
    pub wired_name: String,
    /// Placa cabeada ("enp12s0"), para ligar/desligar.
    pub wired_device: String,
}

pub fn parse_devices(text: &str) -> NetStatus {
    let mut s = NetStatus::default();
    let mut wifi = None;
    for line in text.lines() {
        let f = split_terse(line);
        if f.len() < 3 {
            continue;
        }
        // Placa sem driver ativo ou que o NetworkManager não usa (veth do
        // Docker, por exemplo): não aparece.
        if f[1] == "unmanaged" {
            continue;
        }
        let connected = f[1] == "connected" || f[1].starts_with("connected ");
        match f[0].as_str() {
            "wifi" => {
                s.wifi_present = true;
                if connected {
                    wifi = Some(f[2].clone());
                }
            }
            "ethernet" => {
                // A primeira placa cabeada conectada vale (senão, a primeira).
                if !s.wired_present || (connected && !s.wired_connected) {
                    s.wired_present = true;
                    s.wired_connected = connected;
                    s.wired_unplugged = f[1] == "unavailable";
                    s.wired_name = if connected { f[2].clone() } else { String::new() };
                    s.wired_device = f.get(3).cloned().unwrap_or_default();
                }
            }
            _ => {}
        }
    }
    s.summary = match (&wifi, s.wired_connected) {
        (_, true) => tr("Connected by cable", "Conectado por cabo").to_string(),
        (Some(ssid), false) => format!("{} {ssid}", tr("Connected to Wi-Fi", "Conectado ao Wi-Fi")),
        (None, false) => tr("Not connected", "Sem conexão").to_string(),
    };
    s
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

/// Uma saída ou entrada de som do PipeWire (linha de `wpctl status`).
#[derive(Clone, Debug, PartialEq)]
pub struct AudioNode {
    /// Id do nó no PipeWire (para `wpctl set-default`).
    pub id: u32,
    pub name: String,
    /// É a padrão agora (marcada com `*`).
    pub default: bool,
}

/// Lê as saídas (Sinks) e entradas (Sources) da seção "Audio" de
/// `wpctl status` (rodado com `LC_ALL=C`).
///
/// Linhas como `│  *   37. Built-in Audio Analog Stereo [vol: 0.65]`: o `*`
/// marca a padrão e o `[vol: ...]` do fim sai do nome. A seção "Video" (e o
/// que vier depois) é ignorada.
pub fn parse_wpctl_status(text: &str) -> (Vec<AudioNode>, Vec<AudioNode>) {
    let (mut sinks, mut sources) = (Vec::new(), Vec::new());
    let mut in_audio = false;
    // 0 = outra subseção, 1 = Sinks, 2 = Sources.
    let mut part = 0;
    for line in text.lines() {
        if !line.starts_with(|c: char| c.is_whitespace() || "│├└".contains(c)) {
            in_audio = line.trim() == "Audio";
            part = 0;
            continue;
        }
        if !in_audio {
            continue;
        }
        let item = line.trim_start_matches(|c: char| c.is_whitespace() || "│├└─".contains(c));
        if let Some(head) = item.strip_suffix(':') {
            part = match head {
                "Sinks" => 1,
                "Sources" => 2,
                _ => 0,
            };
            continue;
        }
        if part == 0 {
            continue;
        }
        let (default, item) = match item.strip_prefix('*') {
            Some(rest) => (true, rest.trim_start()),
            None => (false, item),
        };
        let Some((id, name)) = item.split_once(". ") else { continue };
        let Ok(id) = id.trim().parse() else { continue };
        let name = match name.rfind(" [vol:") {
            Some(i) => &name[..i],
            None => name,
        };
        let node = AudioNode { id, name: name.trim().to_string(), default };
        if part == 1 { sinks.push(node) } else { sources.push(node) }
    }
    (sinks, sources)
}

/// Nome curto de uma saída a partir do `wpctl inspect` dela.
///
/// HDMI/DisplayPort viram "HDMI · <monitor>" (o `alsa.name` traz o nome do
/// monitor lido do EDID: "25UM58G"), em vez de "Ellesmere HDMI Audio [Radeon
/// RX 470/480 / 570/580/590] Digital Stereo (HDMI 4)". O resto fica com o
/// `node.description`.
pub fn audio_node_label(inspect: &str) -> Option<String> {
    let value = |key: &str| {
        inspect.lines().find_map(|l| {
            let l = l.trim().trim_start_matches('*').trim();
            let v = l.strip_prefix(key)?.strip_prefix(" = ")?;
            Some(v.trim_matches('"').to_string())
        })
    };
    let description = value("node.description");
    let profile = value("device.profile.description").unwrap_or_default();
    let port = if profile.contains("HDMI") {
        Some("HDMI")
    } else if profile.contains("DisplayPort") {
        Some("DisplayPort")
    } else {
        None
    };
    match (port, value("alsa.name")) {
        (Some(port), Some(monitor)) if !monitor.is_empty() && !monitor.contains("HDMI") => {
            Some(format!("{port} · {monitor}"))
        }
        _ => description,
    }
}

/// Valor de uma chave de `/etc/os-release` (sem aspas).
/// Codinome da série da versão (planta do sertão), da tabela `distro/codinomes`
/// embutida no binário: "0.1.1" → "Mandacaru".
pub fn codename(version: &str) -> Option<&'static str> {
    let serie: String = version.split('.').take(2).collect::<Vec<_>>().join(".");
    include_str!("../../distro/codinomes")
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .find_map(|l| {
            let mut f = l.split_whitespace();
            (f.next()? == serie).then(|| f.next()).flatten()
        })
}

pub fn os_release_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let v = l.strip_prefix(key)?.strip_prefix('=')?;
        Some(v.trim_matches('"').to_string())
    })
}

/// Sistema base para o "Sobre": "Debian 13.2 (trixie)" a partir do
/// `/etc/debian_version` e do codinome no os-release. Sem `debian_version`
/// (outro sistema), o `PRETTY_NAME`.
pub fn base_name(debian_version: Option<&str>, os_release: &str) -> Option<String> {
    match debian_version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => Some(match os_release_value(os_release, "VERSION_CODENAME") {
            Some(c) if !c.is_empty() => format!("Debian {v} ({c})"),
            _ => format!("Debian {v}"),
        }),
        None => os_release_value(os_release, "PRETTY_NAME"),
    }
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
/// e a leitura depende deles; o `wpctl status` também (títulos das seções).
/// O `wpctl inspect` mantém o idioma da sessão, porque o nome da saída de som
/// aparece na tela.
pub(crate) fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    if program == "nmcli" || (program == "wpctl" && args.first() == Some(&"status")) {
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
pub(crate) fn spawn_detached(program: &str, args: &[&str]) {
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

// ---------------------------------------------------------------------------
// Brilho da tela
// ---------------------------------------------------------------------------

/// Uma tela com brilho ajustável (`/sys/class/backlight/<name>`).
#[derive(Clone, Debug, PartialEq)]
pub struct Backlight {
    pub name: String,
    pub current: u32,
    pub max: u32,
}

/// Prioridade do tipo de controle, como o GNOME: o do firmware (ACPI)
/// primeiro, depois o da plataforma e por último o direto da placa ("raw").
fn backlight_rank(kind: &str) -> u8 {
    match kind.trim() {
        "firmware" => 0,
        "platform" => 1,
        _ => 2,
    }
}

/// Escolhe a tela pelo tipo, entre `(nome, tipo, atual, máximo)`; ignora as
/// de máximo 0.
pub fn pick_backlight(found: Vec<(String, String, u32, u32)>) -> Option<Backlight> {
    found
        .into_iter()
        .filter(|(_, _, _, max)| *max > 0)
        .min_by_key(|(name, kind, _, _)| (backlight_rank(kind), name.clone()))
        .map(|(name, _, current, max)| Backlight { name, current: current.min(max), max })
}

/// Lê a tela com brilho ajustável deste computador (notebooks); `None` no PC
/// de mesa e na VM.
pub fn backlight() -> Option<Backlight> {
    let read = |path: std::path::PathBuf| fs::read_to_string(path).ok();
    let num = |text: Option<String>| text.and_then(|t| t.trim().parse::<u32>().ok());
    let found = fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let dir = entry.path();
            Some((
                entry.file_name().to_string_lossy().into_owned(),
                read(dir.join("type")).unwrap_or_default(),
                num(read(dir.join("brightness")))?,
                num(read(dir.join("max_brightness")))?,
            ))
        })
        .collect();
    pick_backlight(found)
}

/// Fração 0–1 do slider → valor do `/sys`. Nunca apaga a tela de vez:
/// o mínimo é 5% (com a tela preta não daria para ver o slider).
pub fn brightness_value(fraction: f32, max: u32) -> u32 {
    let min = (max / 20).max(1);
    ((fraction.clamp(0.0, 1.0) * max as f32).round() as u32).clamp(min, max)
}

/// Pede ao logind para mudar o brilho (vale para a sessão ativa, sem root).
fn set_backlight(name: &str, value: u32) {
    let value = value.to_string();
    spawn_detached(
        "busctl",
        &[
            "call",
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
            "SetBrightness",
            "ssu",
            "backlight",
            name,
            &value,
        ],
    );
}

/// Liga o slider de brilho, se houver tela ajustável. Arrastar o slider
/// manda no máximo um pedido a cada 80 ms (o último valor sempre vai).
fn install_brightness(ui: &AppWindow) {
    let Some(screen) = backlight() else { return };
    let d = ui.global::<Device>();
    d.set_brightness_available(true);
    d.set_brightness(screen.current as f32 / screen.max as f32);
    let pending: Rc<std::cell::Cell<Option<u32>>> = Rc::default();
    let timer = Rc::new(Timer::default());
    d.on_set_brightness(move |fraction| {
        pending.set(Some(brightness_value(fraction, screen.max)));
        if timer.running() {
            return;
        }
        let (pending, name) = (pending.clone(), screen.name.clone());
        let flush = move || {
            if let Some(value) = pending.take() {
                set_backlight(&name, value);
            }
        };
        flush();
        let flush = flush.clone();
        timer.start(TimerMode::SingleShot, Duration::from_millis(80), flush);
    });
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

// ---------------------------------------------------------------------------
// Sobre o sistema
// ---------------------------------------------------------------------------

/// Ficha do sistema (lida uma vez, ao abrir).
fn about_items() -> Vec<(String, String)> {
    let version = match codename(env!("CARGO_PKG_VERSION")) {
        Some(name) => format!("KalangOS {} {name}", env!("CARGO_PKG_VERSION")),
        None => format!("KalangOS {}", env!("CARGO_PKG_VERSION")),
    };
    let mut items = vec![(tr("Version", "Versão").to_string(), version)];
    let read = |p: &str| fs::read_to_string(p).ok();

    // O os-release é o do KalangOS; a base vem do /etc/debian_version.
    let os = read("/etc/os-release").unwrap_or_default();
    if let Some(base) = base_name(read("/etc/debian_version").as_deref(), &os) {
        items.push(("Base".into(), base));
    }
    if let Some(k) = read("/proc/sys/kernel/osrelease") {
        items.push(("Kernel".into(), format!("Linux {}", k.trim())));
    }
    let vendor = read("/sys/class/dmi/id/sys_vendor").unwrap_or_default();
    let product = read("/sys/class/dmi/id/product_name").unwrap_or_default();
    let machine = format!("{} {}", vendor.trim(), product.trim()).trim().to_string();
    if !machine.is_empty() {
        items.push((tr("Computer", "Computador").into(), machine));
    }
    if let Some(model) = read("/proc/cpuinfo").and_then(|t| cpu_model(&t)) {
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        let label = match cores {
            1 => tr("core", "núcleo"),
            _ => tr("cores", "núcleos"),
        };
        items.push((tr("Processor", "Processador").into(), format!("{model} · {cores} {label}")));
    }
    if let Some(mem) = read("/proc/meminfo").and_then(|t| mem_total(&t)) {
        items.push((tr("Memory", "Memória").into(), human_size(mem)));
    }
    if let Some((size, avail)) = output("df", &["-B1", "--output=size,avail", "/"]).and_then(|t| parse_df(&t)) {
        items.push((
            tr("Disk", "Disco").into(),
            format!("{} {} {}", human_size(avail), tr("free of", "livres de"), human_size(size)),
        ));
    }
    if let Some(host) = read("/proc/sys/kernel/hostname") {
        items.push((tr("Computer name", "Nome do computador").into(), host.trim().to_string()));
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        items.push((tr("Session", "Sessão").into(), "Wayland".into()));
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
    /// Perfis Wi-Fi salvos (redes que dá para esquecer).
    saved: Vec<String>,
    /// Conexões salvas de rede (cabo e Wi-Fi) e VPNs (painel rápido).
    connections: Option<(Vec<crate::settings::netconfig::SavedConnection>, Vec<crate::settings::netconfig::SavedConnection>)>,
    sound: Option<(f32, bool, String)>,
    /// Saídas e entradas de som (`wpctl status`), com o nome para a tela.
    sinks: Vec<AudioNode>,
    sources: Vec<AudioNode>,
    /// Volume e mudo da entrada padrão (microfone).
    mic: Option<(f32, bool)>,
    battery: Option<(u32, bool)>,
}

/// Como a lista de redes é lida (`nmcli ... --rescan`).
#[derive(Clone, Copy, PartialEq)]
enum Scan {
    /// O que o NetworkManager já sabe, na hora (`no`).
    Cached,
    /// Busca de novo só se a última tem mais de 30 s (`auto`).
    Auto,
    /// Busca nova, esperando ela terminar (`yes`; 4 a 8 s).
    Fresh,
}

fn read_snapshot(scan: Scan) -> Snapshot {
    let mut s = Snapshot::default();
    if let Some(dev) = output("nmcli", &["-t", "-f", "TYPE,STATE,CONNECTION,DEVICE", "device"]) {
        let status = parse_devices(&dev);
        s.wifi_enabled = output("nmcli", &["radio", "wifi"]).is_some_and(|t| t.trim() == "enabled");
        if status.wifi_present && s.wifi_enabled {
            // "auto": o NetworkManager só procura de novo se a última busca
            // tem mais de 30 s (conectado, ele quase não procura sozinho).
            let rescan = match scan {
                Scan::Cached => "no",
                Scan::Auto => "auto",
                Scan::Fresh => "yes",
            };
            if let Some(list) = output(
                "nmcli",
                &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", rescan],
            ) {
                s.networks = parse_wifi_list(&list);
            }
        }
        // Um nmcli só: redes Wi-Fi salvas, conexões de cabo e VPNs.
        if let Some(text) = output("nmcli", &["-t", "-f", "NAME,TYPE,ACTIVE", "connection", "show"]) {
            s.saved = parse_saved_wifi(&text);
            s.connections = Some(crate::settings::netconfig::parse_connections(&text));
        }
        s.network = Some(status);
    }
    if let Some((vol, muted)) =
        output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]).and_then(|t| parse_wpctl_volume(&t))
    {
        let name = output("wpctl", &["inspect", "@DEFAULT_AUDIO_SINK@"])
            .and_then(|t| parse_wpctl_description(&t))
            .unwrap_or_else(|| tr("Default output", "Saída padrão").into());
        s.sound = Some((vol, muted, name));
        if let Some(text) = output("wpctl", &["status"]) {
            let (sinks, sources) = parse_wpctl_status(&text);
            s.sinks = sinks.into_iter().map(|n| AudioNode { name: node_label(n.id, &n.name), ..n }).collect();
            s.sources = sources.into_iter().map(|n| AudioNode { name: node_label(n.id, &n.name), ..n }).collect();
        }
        s.mic = output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SOURCE@"]).and_then(|t| parse_wpctl_volume(&t));
    }
    if let Some(path) = crate::common::system::battery_path() {
        let cap = fs::read_to_string(path.join("capacity")).ok().and_then(|c| c.trim().parse().ok());
        let charging = fs::read_to_string(path.join("status"))
            .is_ok_and(|t| matches!(t.trim(), "Charging" | "Full"));
        s.battery = cap.map(|c| (c, charging));
    }
    s
}

/// Nome para a tela de um nó de som, guardado por id (o `wpctl inspect` só
/// roda na primeira vez que o nó aparece, não a cada leitura).
fn node_label(id: u32, fallback: &str) -> String {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<u32, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(name) = cache.lock().unwrap().get(&id) {
        return name.clone();
    }
    let name = output("wpctl", &["inspect", &id.to_string()])
        .and_then(|t| audio_node_label(&t))
        .unwrap_or_else(|| fallback.to_string());
    cache.lock().unwrap().insert(id, name.clone());
    name
}

/// Lista de saídas/entradas para a UI: os itens e o índice da padrão (-1).
fn sound_model(nodes: Vec<AudioNode>) -> (ModelRc<SoundDevice>, ModelRc<SharedString>, i32) {
    let index = nodes.iter().position(|n| n.default).map_or(-1, |i| i as i32);
    let names: Vec<SharedString> = nodes.iter().map(|n| n.name.as_str().into()).collect();
    let items: Vec<SoundDevice> = nodes
        .into_iter()
        .map(|n| SoundDevice {
            id: n.id as i32,
            hdmi: n.name.contains("HDMI") || n.name.contains("DisplayPort"),
            name: n.name.into(),
            active: n.default,
        })
        .collect();
    (ModelRc::from(Rc::new(VecModel::from(items))), ModelRc::from(Rc::new(VecModel::from(names))), index)
}

/// Sons do tema freedesktop (`sound-theme-freedesktop`).
const SOUNDS: &str = "/usr/share/sounds/freedesktop/stereo";
/// Espera depois do último movimento do volume antes do som de retorno
/// (arrastar o slider não vira uma rajada de sons).
const VOLUME_SOUND_DELAY: Duration = Duration::from_millis(150);

/// Toca um som do tema na saída padrão pelo `pw-play`.
///
/// `channel` ("FL", "FR"...) põe o som (mono) só naquele canal: é o teste
/// de alto-falantes. Sem o arquivo (tema não instalado), não faz nada.
fn play_sound(name: &str, channel: Option<&str>) {
    let path = format!("{SOUNDS}/{name}.oga");
    if !std::path::Path::new(&path).exists() {
        return;
    }
    match channel {
        Some(ch) => spawn_detached("pw-play", &["--channel-map", ch, &path]),
        None => spawn_detached("pw-play", &[&path]),
    }
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
        // Busca nova de redes: leva de 4 a 8 s na sessão gráfica (no HP).
        // Antes dela, mostra o que o NetworkManager já sabe (estado, lista
        // atual), senão a página de Rede inteira ficava vazia esse tempo.
        if rescan {
            let quick = read_snapshot(Scan::Cached);
            let weak = weak.clone();
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                apply(&ui, quick, false, false);
            });
        }
        let snap = read_snapshot(if rescan { Scan::Fresh } else { Scan::Auto });
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
    d.set_network_ready(true);
    match s.network {
        Some(net) => {
            d.set_network_available(true);
            d.set_network_status(net.summary.into());
            d.set_wifi_present(net.wifi_present);
            d.set_wifi_enabled(s.wifi_enabled);
            d.set_wired_present(net.wired_present);
            d.set_wired_connected(net.wired_connected);
            d.set_wired_unplugged(net.wired_unplugged);
            d.set_wired_name(net.wired_name.into());
            d.set_wired_device(net.wired_device.into());
        }
        None => d.set_network_available(false),
    }
    if let Some((nets, vpns)) = s.connections {
        let nets: Vec<NetConnection> = nets
            .into_iter()
            .map(|(name, kind, active)| NetConnection { name: name.into(), kind: kind.into(), active })
            .collect();
        let vpns: Vec<VpnConnection> = vpns
            .into_iter()
            .map(|(name, kind, active)| VpnConnection { name: name.into(), kind: kind.into(), active })
            .collect();
        d.set_connections(ModelRc::from(Rc::new(VecModel::from(nets))));
        d.set_vpn_active(vpns.iter().find(|v| v.active).map(|v| v.name.clone()).unwrap_or_default());
        d.set_vpns(ModelRc::from(Rc::new(VecModel::from(vpns))));
    }
    let active = s.networks.iter().find(|n| n.active).map(|n| n.ssid.clone()).unwrap_or_default();
    d.set_wifi_name(active.into());
    let nets: Vec<WifiNetwork> = s
        .networks
        .into_iter()
        .map(|n| WifiNetwork {
            saved: s.saved.contains(&n.ssid),
            ssid: n.ssid.into(),
            signal: n.signal,
            secure: n.secure,
            active: n.active,
        })
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
            let (items, names, index) = sound_model(s.sinks);
            d.set_sound_outputs(items);
            d.set_sound_output_names(names);
            d.set_sound_output_index(index);
            let (items, names, index) = sound_model(s.sources);
            d.set_sound_inputs(items);
            d.set_sound_input_names(names);
            d.set_sound_input_index(index);
            d.set_mic_available(s.mic.is_some());
            if let (Some((v, m)), false) = (s.mic, hold_volume) {
                d.set_mic_volume(v);
                d.set_mic_muted(m);
            }
        }
        None => d.set_sound_available(false),
    }

    match s.battery {
        Some((cap, charging)) => {
            d.set_battery(format!("{cap}%").into());
            d.set_charging(charging);
        }
        None => d.set_battery(tr("No battery", "Sem bateria").into()),
    }
}

/// Conecta a uma rede Wi-Fi numa thread e mostra o resultado.
///
/// A senha vai como argumento do `nmcli` (fica visível na lista de processos
/// por instantes, só para o próprio computador); o NetworkManager a guarda
/// para as próximas vezes. `hidden`: rede oculta (não anuncia o nome; o
/// `nmcli` pergunta por ela pelo nome).
fn connect(weak: Weak<AppWindow>, shared: Arc<Shared>, ssid: SharedString, password: SharedString, hidden: bool) {
    if let Some(ui) = weak.upgrade() {
        let d = ui.global::<Device>();
        d.set_network_busy(true);
        d.set_network_message(format!("{} {ssid}...", tr("Connecting to", "Conectando a")).into());
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
        if hidden {
            args.extend(["hidden", "yes"]);
        }
        // Sem senha: usa o perfil salvo, se houver (rede já conhecida).
        let ok = (password.is_empty()
            && output("nmcli", &["connection", "up", "id", ssid.as_str()]).is_some())
            || output("nmcli", &args).is_some();
        if !ok && !password.is_empty() {
            // Não deixa salvo um perfil com a senha errada.
            forget_network(&ssid);
        }
        let message = if ok {
            format!("{} {ssid}", tr("Connected to", "Conectado a"))
        } else if password.is_empty() {
            format!("{} {ssid}", tr("Could not connect to", "Não foi possível conectar a"))
        } else {
            format!(
                "{} {ssid}. {}",
                tr("Could not connect to", "Não foi possível conectar a"),
                tr("Check the password.", "Confira a senha.")
            )
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

/// Esquece uma rede salva (botão "Esquecer"): apaga o perfil numa thread e
/// relê a lista. Se era a rede conectada, o Wi-Fi desconecta.
fn forget(weak: Weak<AppWindow>, shared: Arc<Shared>, ssid: SharedString) {
    std::thread::spawn(move || {
        let ok = output("nmcli", &["connection", "delete", "id", ssid.as_str()]).is_some();
        let message = if ok {
            format!("{} {ssid}", tr("Forgot", "Rede esquecida:"))
        } else {
            format!("{} {ssid}", tr("Could not forget", "Não foi possível esquecer"))
        };
        let weak2 = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak2.upgrade() {
                ui.global::<Device>().set_network_message(message.into());
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
/// processo das Configurações (`--settings`) e no painel rápido
/// (`--quick-settings`), que também liga o brilho da tela. O timer devolvido precisa
/// ficar vivo.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if matches!(mode, Mode::Launcher | Mode::Greeter | Mode::Dock | Mode::DockTrigger | Mode::Fundo) {
        return None;
    }
    let d = ui.global::<Device>();
    d.on_network_index(|nets, ssid| {
        nets.iter().position(|n| n.ssid == ssid).map_or(-1, |i| i as i32)
    });
    // O painel rápido não mostra a ficha do sistema nem os usuários.
    if mode != Mode::QuickSettings {
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
    }

    let native = crate::shell::session::is_native();
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

    if !matches!(mode, Mode::Settings | Mode::QuickSettings) {
        return None;
    }
    if native {
        install_brightness(ui);
    }

    let shared = Arc::new(Shared {
        reading: AtomicBool::new(false),
        volume_touched: AtomicU64::new(0),
    });

    {
        let shared = shared.clone();
        // Som de retorno ao mexer no volume (como no GNOME), um só quando o
        // slider para. O `wpctl set-volume` roda antes, então o som já sai no
        // volume novo.
        let feedback = Rc::new(Timer::default());
        d.on_set_volume(move |v| {
            shared.volume_touched.store(now_ms(), Ordering::SeqCst);
            let v = format!("{:.2}", v.clamp(0.0, 1.0));
            spawn_detached("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &v]);
            feedback.start(TimerMode::SingleShot, VOLUME_SOUND_DELAY, || {
                play_sound("audio-volume-change", None);
            });
        });
    }
    // Teste de alto-falantes: uma voz em cada lado ("front left/right").
    d.on_sound_test(|side| match side.as_str() {
        "left" => play_sound("audio-channel-front-left", Some("FL")),
        "right" => play_sound("audio-channel-front-right", Some("FR")),
        _ => play_sound("audio-test-signal", None),
    });
    {
        let shared = shared.clone();
        d.on_set_muted(move |m| {
            shared.volume_touched.store(now_ms(), Ordering::SeqCst);
            spawn_detached("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", if m { "1" } else { "0" }]);
        });
    }
    {
        let shared = shared.clone();
        d.on_set_mic_volume(move |v| {
            shared.volume_touched.store(now_ms(), Ordering::SeqCst);
            let v = format!("{:.2}", v.clamp(0.0, 1.0));
            spawn_detached("wpctl", &["set-volume", "@DEFAULT_AUDIO_SOURCE@", &v]);
        });
    }
    {
        let shared = shared.clone();
        d.on_set_mic_muted(move |m| {
            shared.volume_touched.store(now_ms(), Ordering::SeqCst);
            spawn_detached("wpctl", &["set-mute", "@DEFAULT_AUDIO_SOURCE@", if m { "1" } else { "0" }]);
        });
    }
    {
        // Saída/entrada padrão (HDMI, fone, alto-falantes...). O WirePlumber
        // guarda a escolha para os próximos logins e leva junto o som que já
        // está tocando.
        let weak = ui.as_weak();
        let shared = shared.clone();
        d.on_set_sound_default(move |id| {
            let weak = weak.clone();
            let shared = shared.clone();
            std::thread::spawn(move || {
                let _ = output("wpctl", &["set-default", &id.to_string()]);
                refresh_from_thread(weak, shared);
            });
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
        // Bloco "Cabeada" do painel: liga/desliga a placa pelo NetworkManager.
        let weak = ui.as_weak();
        let shared = shared.clone();
        d.on_set_wired(move |on| {
            let Some(ui) = weak.upgrade() else { return };
            let dev = ui.global::<Device>().get_wired_device().to_string();
            if dev.is_empty() {
                return;
            }
            let weak = weak.clone();
            let shared = shared.clone();
            std::thread::spawn(move || {
                let _ = output("nmcli", &["device", if on { "connect" } else { "disconnect" }, &dev]);
                refresh_from_thread(weak, shared);
            });
        });
    }
    {
        // Listas do painel (cabo e VPN): ativa ou desativa uma conexão salva.
        let weak = ui.as_weak();
        let shared = shared.clone();
        d.on_connection_set(move |name, on| {
            let weak = weak.clone();
            let shared = shared.clone();
            std::thread::spawn(move || {
                let ok = output("nmcli", &["connection", if on { "up" } else { "down" }, "id", &name]).is_some();
                if !ok && on {
                    let msg = format!("{} {name}", tr("Could not connect:", "Não foi possível conectar:"));
                    let w = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = w.upgrade() {
                            ui.global::<Device>().set_network_message(msg.into());
                        }
                    });
                }
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
        d.on_connect_wifi(move |ssid, password| connect(weak.clone(), shared.clone(), ssid, password, false));
    }
    {
        let weak = ui.as_weak();
        let shared = shared.clone();
        d.on_connect_hidden(move |ssid, password| {
            let ssid: SharedString = ssid.trim().into();
            connect(weak.clone(), shared.clone(), ssid, password, true)
        });
    }
    {
        let weak = ui.as_weak();
        let shared = shared.clone();
        d.on_forget_wifi(move |ssid| forget(weak.clone(), shared.clone(), ssid));
    }

    let weak = ui.as_weak();
    // Nas Configurações, procura redes de verdade ao abrir (a lista guardada
    // pode estar velha); o painel rápido só precisa do estado atual.
    if mode == Mode::QuickSettings {
        // A placa Wi-Fi já se sabe pelo /sys (na hora): o bloco nasce como
        // Wi-Fi mesmo se o nmcli demorar, em vez de "Rede" trocando depois.
        if has_wifi_card() {
            d.set_wifi_present(true);
            d.set_wifi_enabled(wifi_radio_on());
        }
        // O painel lê o estado antes de a janela aparecer (~0,1 s): sem isso
        // ele abria só com o brilho (lido na hora) e o botão "Rede" no lugar
        // do Wi-Fi, e o resto surgia depois. Se o NetworkManager travar, não
        // espera mais que isso e segue como antes.
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(read_snapshot(Scan::Cached));
        });
        match rx.recv_timeout(FIRST_READ_WAIT) {
            Ok(snap) => apply(ui, snap, false, false),
            Err(_) => refresh(weak.clone(), shared.clone(), false),
        }
    } else {
        refresh(weak.clone(), shared.clone(), mode == Mode::Settings);
    }
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, INTERVAL, move || refresh(weak.clone(), shared.clone(), false));
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codinome_da_versao() {
        assert_eq!(codename("0.1.1"), Some("Mandacaru"));
        assert_eq!(codename("9.9.0"), None);
    }

    #[test]
    fn le_perfis_wifi_salvos() {
        let text = "Cats:802-11-wireless\nlo:loopback\nWired connection 1:802-3-ethernet\nCafe\\: 2:802-11-wireless\n";
        assert_eq!(parse_saved_wifi(text), vec!["Cats".to_string(), "Cafe: 2".to_string()]);
    }

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
        let s = parse_devices("ethernet:connected:Wired connection 1:enp12s0\nloopback:connected (externally):lo:lo\n");
        assert_eq!(s.summary, tr("Connected by cable", "Conectado por cabo"));
        assert!(!s.wifi_present && s.wired_present && s.wired_connected);
        assert_eq!((s.wired_name.as_str(), s.wired_device.as_str()), ("Wired connection 1", "enp12s0"));
        let s = parse_devices("wifi:connected:Casa:wlan0\nethernet:unavailable::eth0\n");
        assert_eq!(s.summary, format!("{} Casa", tr("Connected to Wi-Fi", "Conectado ao Wi-Fi")));
        assert!(s.wifi_present && s.wired_unplugged && !s.wired_connected);
        // Placa sem driver/gerência (veth do Docker, Wi-Fi sem firmware): some.
        let s = parse_devices("wifi:unmanaged::wlan0\nethernet:unmanaged::veth1\n");
        assert!(!s.wifi_present && !s.wired_present);
        assert_eq!(parse_devices("wifi:disconnected:\n").summary, tr("Not connected", "Sem conexão"));
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
    fn le_saidas_e_entradas_do_wpctl_status() {
        let text = "PipeWire 'pipewire-0' [1.4.2, tiago@desktop, cookie:1]\n \
 └─ Clients:\n        33. WirePlumber                         [1.4.2, tiago@desktop, pid:1238]\n\n\
Audio\n ├─ Devices:\n │      48. Ellesmere HDMI Audio [Radeon RX 470/480 / 570/580/590] [alsa]\n │  \n\
 ├─ Sinks:\n │  *   37. Built-in Audio Digital Stereo (IEC958) [vol: 0.65]\n\
 │      56. Ellesmere HDMI Audio [Radeon RX 470/480 / 570/580/590] Digital Stereo (HDMI 4) [vol: 0.40]\n │  \n\
 ├─ Sources:\n │      46. Built-in Audio Analog Stereo        [vol: 1.00]\n │  \n ├─ Filters:\n │  \n └─ Streams:\n\n\
Video\n ├─ Devices:\n ├─ Sinks:\n │      90. Câmera\n";
        let (sinks, sources) = parse_wpctl_status(text);
        assert_eq!(sinks.len(), 2);
        assert_eq!(sinks[0], AudioNode { id: 37, name: "Built-in Audio Digital Stereo (IEC958)".into(), default: true });
        assert_eq!(sinks[1].id, 56);
        assert_eq!(sinks[1].name, "Ellesmere HDMI Audio [Radeon RX 470/480 / 570/580/590] Digital Stereo (HDMI 4)");
        assert!(!sinks[1].default);
        assert_eq!(sources, vec![AudioNode { id: 46, name: "Built-in Audio Analog Stereo".into(), default: false }]);
    }

    #[test]
    fn nome_curto_da_saida_hdmi() {
        let hdmi = "id 56\n    alsa.name = \"25UM58G\"\n    device.profile.description = \"Digital Stereo (HDMI 4)\"\n  \
* node.description = \"Ellesmere HDMI Audio Digital Stereo (HDMI 4)\"\n";
        assert_eq!(audio_node_label(hdmi).as_deref(), Some("HDMI · 25UM58G"));
        // Sem o nome do monitor (alsa.name genérico): a descrição inteira.
        let sem = "    alsa.name = \"HDMI 0\"\n    device.profile.description = \"Digital Stereo (HDMI)\"\n  \
* node.description = \"Placa HDMI\"\n";
        assert_eq!(audio_node_label(sem).as_deref(), Some("Placa HDMI"));
        let interno = "    alsa.name = \"ALC892 Analog\"\n    device.profile.description = \"Analog Stereo\"\n  \
* node.description = \"Áudio interno\"\n";
        assert_eq!(audio_node_label(interno).as_deref(), Some("Áudio interno"));
    }

    #[test]
    fn escolhe_a_tela_e_o_brilho() {
        let telas = vec![
            ("intel_backlight".to_string(), "raw\n".to_string(), 300, 1000),
            ("acpi_video0".to_string(), "firmware\n".to_string(), 7, 15),
            ("vazia".to_string(), "firmware".to_string(), 0, 0),
        ];
        let b = pick_backlight(telas).unwrap();
        assert_eq!((b.name.as_str(), b.current, b.max), ("acpi_video0", 7, 15));
        assert!(pick_backlight(vec![]).is_none());
        assert_eq!(brightness_value(0.5, 1000), 500);
        assert_eq!(brightness_value(0.0, 1000), 50);
        assert_eq!(brightness_value(2.0, 15), 15);
        assert_eq!(brightness_value(0.0, 10), 1);
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
        let kos = "PRETTY_NAME=\"KalangOS 0.1\"\nVERSION_CODENAME=trixie\n";
        assert_eq!(base_name(Some("13.2\n"), kos).as_deref(), Some("Debian 13.2 (trixie)"));
        assert_eq!(base_name(None, os).as_deref(), Some("Debian GNU/Linux 13 (trixie)"));
        let cpu = "processor : 0\nmodel name\t: Intel(R) Core(TM)  i5-8250U CPU @ 1.60GHz\n";
        assert_eq!(cpu_model(cpu).as_deref(), Some("Intel Core i5-8250U CPU @ 1.60GHz"));
        assert_eq!(mem_total("MemTotal:        4000000 kB\n"), Some(4_096_000_000));
        assert_eq!(human_size(4_096_000_000), "4,1 GB");
        assert_eq!(human_size(512_000_000), "512 MB");
        assert_eq!(parse_df("1B-blocks Avail\n20000000000 12345000000\n"), Some((20_000_000_000, 12_345_000_000)));
    }
}
