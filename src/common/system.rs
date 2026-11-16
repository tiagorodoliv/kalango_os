//! Indicadores do sistema da barra superior: uso de CPU e bateria.
//!
//! Lê direto do kernel Linux (`/proc/stat` e `/sys/class/power_supply`).
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

/// Inicia o timer que atualiza CPU e bateria na UI.
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
}
