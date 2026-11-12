//! Relógio do sistema: hora local formatada em pt-BR.
//!
//! Preenche três textos do global `Shell` a cada segundo:
//! - `time`: "19:38" (tela de login);
//! - `long-date`: "quarta-feira, 11 de novembro" (tela de login);
//! - `bar-date`: "qua 11 nov 19:38" (barra superior).
//!
//! Os nomes de dias e meses ficam em tabelas próprias em vez de depender do
//! locale do sistema: no dispositivo o locale pode nem estar instalado.

use std::time::Duration;

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use slint::{ComponentHandle, Timer, TimerMode};

use crate::{AppWindow, Shell};

/// Dias da semana por extenso, começando no domingo (índice de `num_days_from_sunday`).
const WEEKDAYS: [&str; 7] = [
    "domingo",
    "segunda-feira",
    "terça-feira",
    "quarta-feira",
    "quinta-feira",
    "sexta-feira",
    "sábado",
];

/// Meses por extenso, janeiro = índice 0.
const MONTHS: [&str; 12] = [
    "janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro",
    "outubro", "novembro", "dezembro",
];

/// Textos de data/hora já formatados para a UI.
#[derive(Debug, PartialEq, Eq)]
pub struct ClockTexts {
    /// "19:38"
    pub time: String,
    /// "quarta-feira, 11 de novembro"
    pub long_date: String,
    /// "qua 11 nov 19:38"
    pub bar_date: String,
}

/// Formata um instante nos três textos usados pela UI.
///
/// Abreviações = 3 primeiras letras ("qua", "sáb", "nov"); por isso o corte é
/// feito por caracteres e não por bytes (acentos ocupam 2 bytes em UTF-8).
pub fn format<Tz: TimeZone>(now: &DateTime<Tz>) -> ClockTexts {
    let weekday = WEEKDAYS[now.weekday().num_days_from_sunday() as usize];
    let month = MONTHS[now.month0() as usize];
    let short = |s: &str| s.chars().take(3).collect::<String>();
    let time = format!("{:02}:{:02}", now.hour(), now.minute());

    ClockTexts {
        long_date: format!("{weekday}, {} de {month}", now.day()),
        bar_date: format!("{} {} {} {time}", short(weekday), now.day(), short(month)),
        time,
    }
}

/// Inicia o timer que mantém o relógio atualizado.
///
/// Retorna o `Timer`: ele para assim que é descartado, então quem chama
/// deve mantê-lo vivo durante toda a execução.
pub fn start(ui: &AppWindow) -> Timer {
    // Weak evita ciclo de referência entre o timer e a janela.
    let weak = ui.as_weak();
    let timer = Timer::default();

    update(&weak);
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || update(&weak));
    timer
}

/// Escreve a hora atual na UI, se a janela ainda existir.
fn update(weak: &slint::Weak<AppWindow>) {
    let Some(ui) = weak.upgrade() else { return };
    let texts = format(&Local::now());
    let shell = ui.global::<Shell>();
    shell.set_time(texts.time.into());
    shell.set_long_date(texts.long_date.into());
    shell.set_bar_date(texts.bar_date.into());
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn formata_como_no_layout() {
        // 11/11/2026 é uma quarta-feira, a data mostrada nas telas de referência.
        let t = Utc.with_ymd_and_hms(2026, 11, 11, 19, 38, 0).unwrap();
        assert_eq!(
            format(&t),
            ClockTexts {
                time: "19:38".into(),
                long_date: "quarta-feira, 11 de novembro".into(),
                bar_date: "qua 11 nov 19:38".into(),
            }
        );
    }

    #[test]
    fn abrevia_com_acento_sem_quebrar_utf8() {
        let t = Utc.with_ymd_and_hms(2026, 3, 7, 8, 5, 0).unwrap(); // sábado
        assert_eq!(format(&t).bar_date, "sáb 7 mar 08:05");
    }
}
