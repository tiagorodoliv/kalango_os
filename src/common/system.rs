//! Indicadores do sistema da barra superior: uso de CPU, bateria e rede.
//!
//! Lê direto do kernel Linux (`/proc/stat`, `/sys/class/power_supply`,
//! `/proc/net/route` e `/proc/net/wireless`), sem chamar programas.
//! Fora do Linux (ex.: desenvolvimento no macOS) esses arquivos não existem
//! e a barra mostra "--".

use std::cell::Cell;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

use crate::{AppWindow, Shell};

/// Intervalo entre leituras. O uso de CPU é a média nesse período.
const INTERVAL: Duration = Duration::from_secs(2);

/// Contadores acumulados de CPU (em "jiffies") desde o boot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub total: u64,
    pub idle: u64,
}

/// Extrai os contadores da linha agregada `cpu ...` do conteúdo de `/proc/stat`.
///
/// Ocioso = `idle` + `iowait` (4ª e 5ª colunas): esperar disco não é uso de CPU.
pub fn parse_proc_stat(content: &str) -> Option<CpuTimes> {
    let line = content.lines().find(|l| l.starts_with("cpu "))?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|v| v.parse().ok())
        .collect();
    if values.len() < 4 {
        return None;
    }
    Some(CpuTimes {
        total: values.iter().sum(),
        idle: values[3] + values.get(4).copied().unwrap_or(0),
    })
}

/// Percentual de uso entre duas leituras (0–100).
pub fn cpu_percent(prev: CpuTimes, now: CpuTimes) -> u32 {
    let total = now.total.saturating_sub(prev.total);
    let idle = now.idle.saturating_sub(prev.idle);
    if total == 0 {
        return 0;
    }
    ((total - idle.min(total)) * 100 / total) as u32
}

/// Lê os contadores atuais de CPU; `None` fora do Linux.
fn read_cpu() -> Option<CpuTimes> {
    parse_proc_stat(&fs::read_to_string("/proc/stat").ok()?)
}

/// Pasta da primeira bateria em `/sys/class/power_supply` (`None` sem bateria).
///
/// Procura a fonte com `type` = "Battery" (o nome varia: BAT0, BAT1, battery...).
pub(crate) fn battery_path() -> Option<PathBuf> {
    fs::read_dir("/sys/class/power_supply")
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            let read = |f: &str| fs::read_to_string(path.join(f)).unwrap_or_default();
            is_system_battery(&read("type"), &read("scope"))
        })
}

/// Bateria do próprio computador: tipo "Battery" e sem `scope=Device`, que é
/// a de mouse, teclado ou fone sem fio (num desktop, a do mouse aparecia na
/// barra como se fosse a do computador).
fn is_system_battery(kind: &str, scope: &str) -> bool {
    kind.trim() == "Battery" && scope.trim() != "Device"
}

/// Carga da primeira bateria encontrada (0–100); `None` se não houver bateria.
pub(crate) fn read_battery() -> Option<u32> {
    fs::read_to_string(battery_path()?.join("capacity"))
        .ok()
        .and_then(|c| c.trim().parse().ok())
}

/// A bateria está recebendo energia da tomada: carregando, cheia ou parada
/// pelo limite de carga ("Not charging" com o carregador ligado).
pub(crate) fn battery_on_ac() -> bool {
    let Some(path) = battery_path() else { return false };
    let status = fs::read_to_string(path.join("status")).unwrap_or_default();
    match status.trim() {
        "Charging" | "Full" => true,
        "Not charging" => ac_online(),
        _ => false,
    }
}

/// Algum carregador ligado (`/sys/class/power_supply/*/online` de um "Mains").
fn ac_online() -> bool {
    fs::read_dir("/sys/class/power_supply").is_ok_and(|dir| {
        dir.flatten().any(|e| {
            let p = e.path();
            fs::read_to_string(p.join("type")).is_ok_and(|t| t.trim() == "Mains")
                && fs::read_to_string(p.join("online")).is_ok_and(|o| o.trim() == "1")
        })
    })
}

/// Placa da rota padrão de menor métrica, entre as placas físicas
/// (`physical`: tem dispositivo no `/sys`; VPNs como `wg0`/`tun0` não têm).
/// `route` = conteúdo de `/proc/net/route`.
pub fn default_iface(route: &str, physical: impl Fn(&str) -> bool) -> Option<String> {
    route
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            // Destino 0.0.0.0 e máscara 0 = rota padrão; Flags 0x1 = ativa.
            let up = u32::from_str_radix(f.get(3)?, 16).ok()? & 1 == 1;
            (f.len() > 7 && f[1] == "00000000" && f[7] == "00000000" && up).then(|| (f[0], f[6].parse::<u32>().unwrap_or(0)))
        })
        .filter(|(iface, _)| physical(iface))
        .min_by_key(|(_, metric)| *metric)
        .map(|(iface, _)| iface.to_string())
}

/// Qualidade do sinal (0–100) de uma placa Wi-Fi, de `/proc/net/wireless`
/// ("wlan0: 0000   54.  -56. ..."; a qualidade vai até 70).
pub fn wifi_quality(wireless: &str, iface: &str) -> Option<u32> {
    let line = wireless.lines().find(|l| l.trim_start().starts_with(&format!("{iface}:")))?;
    let link: f32 = line.split_whitespace().nth(2)?.trim_end_matches('.').parse().ok()?;
    Some(((link / 70.0 * 100.0).round() as u32).min(100))
}

/// Ícone de rede da barra, pela conexão principal (a da rota padrão), como
/// no Ubuntu: "ethernet", "wifi-1" a "wifi-3" (força do sinal), "wifi-0"
/// (Wi-Fi ligado, sem rede), "wifi-off" (rádio desligado) ou "ethernet-off"
/// (sem rede). Sem placa Wi-Fi, nunca mostra o Wi-Fi.
pub fn net_icon(primary: Option<(bool, Option<u32>)>, wifi_card: bool, wifi_radio_on: bool) -> &'static str {
    match primary {
        Some((false, _)) => "ethernet",
        Some((true, q)) => match q.unwrap_or(100) {
            0..=33 => "wifi-1",
            34..=66 => "wifi-2",
            _ => "wifi-3",
        },
        None if wifi_card && wifi_radio_on => "wifi-0",
        None if wifi_card => "wifi-off",
        None => "ethernet-off",
    }
}

/// Lê agora o ícone de rede da barra (ver [`net_icon`]).
fn read_net_icon() -> &'static str {
    let net = std::path::Path::new("/sys/class/net");
    let route = fs::read_to_string("/proc/net/route").unwrap_or_default();
    let primary = default_iface(&route, |i| net.join(i).join("device").exists()).map(|iface| {
        let wireless = net.join(&iface).join("wireless").exists() || net.join(&iface).join("phy80211").exists();
        let quality = wireless
            .then(|| fs::read_to_string("/proc/net/wireless").ok().and_then(|t| wifi_quality(&t, &iface)))
            .flatten();
        (wireless, quality)
    });
    let wifi_card = fs::read_dir(net)
        .map(|d| d.flatten().any(|e| e.path().join("wireless").exists() || e.path().join("phy80211").exists()))
        .unwrap_or(false);
    // Rádio pelo rfkill (o NetworkManager bloqueia o rádio ao desligar o Wi-Fi).
    let radio_on = fs::read_dir("/sys/class/rfkill").map_or(true, |dir| {
        let read = |p: PathBuf| fs::read_to_string(p).unwrap_or_default().trim().to_string();
        dir.flatten()
            .filter(|e| read(e.path().join("type")) == "wlan")
            .all(|e| read(e.path().join("soft")) == "0" && read(e.path().join("hard")) == "0")
    });
    net_icon(primary, wifi_card, radio_on)
}

/// Inicia o timer que atualiza CPU, bateria e rede na UI.
///
/// Retorna o `Timer`, que deve ser mantido vivo pelo chamador.
pub fn start(ui: &AppWindow) -> Timer {
    let weak = ui.as_weak();
    // Leitura anterior de CPU, necessária para calcular a diferença.
    let prev = Cell::new(read_cpu());

    let update = move || {
        let Some(ui) = weak.upgrade() else { return };
        let shell = ui.global::<Shell>();

        let now = read_cpu();
        let cpu = match (prev.get(), now) {
            (Some(p), Some(n)) => format!("{}%", cpu_percent(p, n)),
            _ => "--".to_string(),
        };
        prev.set(now);
        shell.set_cpu(cpu.into());

        let level = read_battery();
        let battery = level.map_or_else(|| "--".to_string(), |b| format!("{b}%"));
        shell.set_battery(battery.into());
        shell.set_battery_level(level.unwrap_or(0).min(100) as i32);
        shell.set_battery_charging(level.is_some() && battery_on_ac());
        shell.set_net_icon(read_net_icon().into());
    };

    update();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, INTERVAL, update);
    timer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bateria_do_computador_e_nao_do_mouse() {
        assert!(is_system_battery("Battery\n", ""));
        assert!(is_system_battery("Battery", "System\n"));
        assert!(!is_system_battery("Battery", "Device\n"));
        assert!(!is_system_battery("Mains", ""));
    }

    #[test]
    fn le_linha_agregada_do_proc_stat() {
        let content = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 50 0 25 400 25 0 0 0 0 0\n";
        assert_eq!(parse_proc_stat(content), Some(CpuTimes { total: 1000, idle: 850 }));
    }

    #[test]
    fn calcula_percentual_entre_leituras() {
        let a = CpuTimes { total: 1000, idle: 850 };
        let b = CpuTimes { total: 1200, idle: 1026 };
        assert_eq!(cpu_percent(a, b), 12);
        assert_eq!(cpu_percent(a, a), 0);
    }

    #[test]
    fn rede_principal_pela_rota_padrao() {
        let route = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
                     wg0\t00000000\t00000000\t0001\t0\t0\t50\t00000000\t0\t0\t0\n\
                     wlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
                     enp12s0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
                     enp12s0\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n";
        // A VPN (sem placa física) não conta; o cabo tem métrica menor.
        assert_eq!(super::default_iface(route, |i| i != "wg0").as_deref(), Some("enp12s0"));
        assert_eq!(super::default_iface(route, |i| i == "wlan0").as_deref(), Some("wlan0"));
        assert_eq!(super::default_iface("Iface\n", |_| true), None);
    }

    #[test]
    fn icone_de_rede() {
        let wireless = "Inter-| sta-|   Quality        |\n face | tus | link level noise |\n  wlan0: 0000   35.  -70.  -256  0 0 0 0 0 0\n";
        assert_eq!(super::wifi_quality(wireless, "wlan0"), Some(50));
        assert_eq!(super::net_icon(Some((false, None)), true, true), "ethernet");
        assert_eq!(super::net_icon(Some((true, Some(50))), true, true), "wifi-2");
        assert_eq!(super::net_icon(Some((true, Some(90))), true, true), "wifi-3");
        assert_eq!(super::net_icon(None, true, true), "wifi-0");
        assert_eq!(super::net_icon(None, true, false), "wifi-off");
        // Sem placa Wi-Fi (desktop com cabo): nunca mostra o Wi-Fi.
        assert_eq!(super::net_icon(None, false, true), "ethernet-off");
    }
}
