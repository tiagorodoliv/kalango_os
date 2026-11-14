//! Relógio do sistema: hora local formatada no idioma da interface.
//!
//! Preenche os textos do global `Shell` a cada segundo (exemplos em inglês /
//! português):
//! - `time`: "19:38" (tela de login e barra superior);
//! - `long-date`: "Wednesday, November 11" / "quarta-feira, 11 de novembro"
//!   (tela de login);
//! - `bar-day`: "Wed Nov 11" / "qua 11 nov" e `bar-seconds`: "05" (barra
//!   superior, que monta "Wed Nov 11 19:38" conforme as opções da Barra de
//!   tarefas).
//!
//! Os nomes de dias e meses ficam em tabelas próprias em vez de depender do
//! locale do sistema: no dispositivo o locale pode nem estar instalado.

use std::time::Duration;

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use slint::{ComponentHandle, Timer, TimerMode};

use crate::common::i18n::{self, Lang};
use crate::{AppWindow, Shell};

/// Dias da semana em inglês, começando no domingo.
const WEEKDAYS_EN: [&str; 7] =
    ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// Meses em inglês, janeiro = índice 0.
const MONTHS_EN: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];

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
    /// "Wednesday, November 11" / "quarta-feira, 11 de novembro"
    pub long_date: String,
    /// "Wed Nov 11" / "qua 11 nov"
    pub bar_day: String,
    /// "05"
    pub seconds: String,
}

/// Formata um instante nos textos usados pela UI, no idioma `lang`.
///
/// Abreviações = 3 primeiras letras ("Wed", "qua", "sáb"); por isso o corte é
/// feito por caracteres e não por bytes (acentos ocupam 2 bytes em UTF-8).
pub fn format<Tz: TimeZone>(now: &DateTime<Tz>, lang: Lang) -> ClockTexts {
    let day = now.weekday().num_days_from_sunday() as usize;
    let short = |s: &str| s.chars().take(3).collect::<String>();
    let (long_date, bar_day) = match lang {
        Lang::English => {
            let (weekday, month) = (WEEKDAYS_EN[day], MONTHS_EN[now.month0() as usize]);
            (
                format!("{weekday}, {month} {}", now.day()),
                format!("{} {} {}", short(weekday), short(month), now.day()),
            )
        }
        Lang::Portuguese => {
            let (weekday, month) = (WEEKDAYS[day], MONTHS[now.month0() as usize]);
            (
                format!("{weekday}, {} de {month}", now.day()),
                format!("{} {} {}", short(weekday), now.day(), short(month)),
            )
        }
    };
    ClockTexts {
        time: format!("{:02}:{:02}", now.hour(), now.minute()),
        long_date,
        bar_day,
        seconds: format!("{:02}", now.second()),
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
    let texts = format(&Local::now(), i18n::lang());
    let shell = ui.global::<Shell>();
    shell.set_bar_time(texts.time.clone().into());
    shell.set_time(texts.time.into());
    shell.set_long_date(texts.long_date.into());
    shell.set_bar_day(texts.bar_day.into());
    shell.set_bar_seconds(texts.seconds.into());
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
            format(&t, Lang::Portuguese),
            ClockTexts {
                time: "19:38".into(),
                long_date: "quarta-feira, 11 de novembro".into(),
                bar_day: "qua 11 nov".into(),
                seconds: "00".into(),
            }
        );
    }

    #[test]
    fn abrevia_com_acento_sem_quebrar_utf8() {
        let t = Utc.with_ymd_and_hms(2026, 3, 7, 8, 5, 0).unwrap(); // sábado
        assert_eq!(format(&t, Lang::Portuguese).bar_day, "sáb 7 mar");
        assert_eq!(format(&t, Lang::Portuguese).time, "08:05");
    }

    #[test]
    fn formata_em_ingles() {
        let t = Utc.with_ymd_and_hms(2026, 11, 11, 19, 38, 0).unwrap();
        let texts = format(&t, Lang::English);
        assert_eq!(texts.long_date, "Wednesday, November 11");
        assert_eq!(texts.bar_day, "Wed Nov 11");
        assert_eq!(texts.time, "19:38");
    }
}
