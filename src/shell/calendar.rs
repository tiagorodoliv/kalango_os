//! Calendário que abre ao clicar no relógio da barra (`--calendario`).
//!
//! Como o painel rápido: um processo próprio, em tela cheia e transparente,
//! por cima dos apps (regra `kalango-calendario` no rc.xml); clicar no
//! relógio de novo ou fora do cartão fecha. Mostra o mês (setas para os
//! outros meses), o dia de hoje em destaque e um atalho para a Agenda.

use std::rc::Rc;

use chrono::{Datelike, Local, NaiveDate};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::common::i18n::{lang, Lang};
use crate::{AppWindow, Calendar, CalendarDay, Mode};

/// Nomes dos meses (inglês, português).
const MONTHS: [(&str, &str); 12] = [
    ("January", "janeiro"),
    ("February", "fevereiro"),
    ("March", "março"),
    ("April", "abril"),
    ("May", "maio"),
    ("June", "junho"),
    ("July", "julho"),
    ("August", "agosto"),
    ("September", "setembro"),
    ("October", "outubro"),
    ("November", "novembro"),
    ("December", "dezembro"),
];
/// Iniciais dos dias da semana, começando no domingo.
const WEEKDAYS: [(&str, &str); 7] = [("S", "D"), ("M", "S"), ("T", "T"), ("W", "Q"), ("T", "Q"), ("F", "S"), ("S", "S")];
/// Nomes dos dias da semana, começando no domingo.
const WEEKDAY_NAMES: [(&str, &str); 7] = [
    ("Sunday", "domingo"),
    ("Monday", "segunda-feira"),
    ("Tuesday", "terça-feira"),
    ("Wednesday", "quarta-feira"),
    ("Thursday", "quinta-feira"),
    ("Friday", "sexta-feira"),
    ("Saturday", "sábado"),
];

fn pick<'a>(pair: (&'a str, &'a str)) -> &'a str {
    if lang() == Lang::Portuguese { pair.1 } else { pair.0 }
}

/// Um dia na grade: número, se é do mês mostrado e se é hoje.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub day: u32,
    pub in_month: bool,
    pub today: bool,
}

/// Mês `months` meses depois (ou antes, se negativo) de `(ano, mês)`.
pub fn shift_month(year: i32, month: u32, months: i32) -> (i32, u32) {
    let total = year * 12 + month as i32 - 1 + months;
    (total.div_euclid(12), total.rem_euclid(12) as u32 + 1)
}

/// Grade de 6 semanas (42 dias, domingo primeiro) do mês, com os dias do mês
/// anterior e do seguinte nas pontas.
pub fn month_grid(year: i32, month: u32, today: NaiveDate) -> Vec<Cell> {
    let Some(first) = NaiveDate::from_ymd_opt(year, month, 1) else { return Vec::new() };
    let lead = first.weekday().num_days_from_sunday() as i64;
    let start = first - chrono::Duration::days(lead);
    (0..42)
        .map(|i| {
            let d = start + chrono::Duration::days(i);
            Cell { day: d.day(), in_month: d.month() == month && d.year() == year, today: d == today }
        })
        .collect()
}

/// "novembro de 2026" / "November 2026".
pub fn month_title(year: i32, month: u32) -> String {
    let name = pick(MONTHS[(month as usize).clamp(1, 12) - 1]);
    if lang() == Lang::Portuguese { format!("{name} de {year}") } else { format!("{name} {year}") }
}

/// "sábado, 14 de novembro de 2026" / "Saturday, November 14, 2026".
pub fn long_date(d: NaiveDate) -> String {
    let wd = pick(WEEKDAY_NAMES[d.weekday().num_days_from_sunday() as usize]);
    let month = pick(MONTHS[d.month0() as usize]);
    if lang() == Lang::Portuguese {
        format!("{wd}, {} de {month} de {}", d.day(), d.year())
    } else {
        format!("{wd}, {month} {}, {}", d.day(), d.year())
    }
}

thread_local! {
    /// Mês mostrado (ano, mês).
    static SHOWN: std::cell::Cell<(i32, u32)> = const { std::cell::Cell::new((2000, 1)) };
}

fn show(ui: &AppWindow) {
    let today = Local::now().date_naive();
    let (year, month) = SHOWN.with(|s| s.get());
    let c = ui.global::<Calendar>();
    c.set_title(month_title(year, month).into());
    c.set_today(long_date(today).into());
    c.set_is_current_month((year, month) == (today.year(), today.month()));
    let days: Vec<CalendarDay> = month_grid(year, month, today)
        .into_iter()
        .map(|cell| CalendarDay { number: cell.day as i32, in_month: cell.in_month, today: cell.today })
        .collect();
    c.set_days(ModelRc::from(Rc::new(VecModel::from(days))));
}

/// Liga o calendário (modo `--calendario` e o protótipo da área de trabalho).
pub fn install(ui: &AppWindow, mode: Mode) {
    if !matches!(mode, Mode::Calendar | Mode::Desktop) {
        return;
    }
    let today = Local::now().date_naive();
    SHOWN.with(|s| s.set((today.year(), today.month())));
    let c = ui.global::<Calendar>();
    let names: Vec<slint::SharedString> = WEEKDAYS.iter().map(|w| pick(*w).into()).collect();
    c.set_weekdays(ModelRc::from(Rc::new(VecModel::from(names))));
    c.set_agenda_available(crate::shell::session::command_for("calendar").is_some());
    show(ui);

    let weak = ui.as_weak();
    c.on_move_month(move |delta| {
        let Some(ui) = weak.upgrade() else { return };
        SHOWN.with(|s| {
            let (y, m) = s.get();
            s.set(shift_month(y, m, delta));
        });
        show(&ui);
    });
    let weak = ui.as_weak();
    c.on_go_today(move || {
        let Some(ui) = weak.upgrade() else { return };
        let today = Local::now().date_naive();
        SHOWN.with(|s| s.set((today.year(), today.month())));
        show(&ui);
    });
    c.on_open_agenda(|| {
        let _ = crate::shell::session::open("calendar");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grade_do_mes() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
        let g = month_grid(2026, 11, today);
        assert_eq!(g.len(), 42);
        // 1º de novembro de 2026 é domingo: primeira célula.
        assert_eq!(g[0], Cell { day: 1, in_month: true, today: false });
        assert!(g[13].today && g[13].day == 14);
        assert_eq!(g[29].day, 30);
        assert!(!g[30].in_month && g[30].day == 1);
        // Fevereiro de 2026 começa num domingo; março de 2026, também.
        let f = month_grid(2026, 3, today);
        assert!(f[0].in_month && f[0].day == 1);
        let j = month_grid(2027, 1, today);
        // 1º de janeiro de 2027 é sexta: cinco dias de dezembro antes.
        assert_eq!((j[0].day, j[0].in_month, j[5].day), (27, false, 1));
    }

    #[test]
    fn meses_para_frente_e_para_tras() {
        assert_eq!(shift_month(2026, 11, 1), (2026, 12));
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 3, -14), (2025, 1));
    }
}
