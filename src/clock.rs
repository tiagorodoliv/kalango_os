//! Relógio da barra de status.
//!
//! Atualiza a propriedade `clock-text` da janela a cada segundo. Usa só a
//! biblioteca padrão (hora UTC) para não puxar dependência de fuso agora;
//! quando o layout definir o formato final, trocar por `chrono`/`time`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slint::{ComponentHandle, Timer, TimerMode};

use crate::AppWindow;

/// Inicia o timer que atualiza o relógio da UI.
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

/// Escreve a hora atual (HH:MM, UTC) na UI, se a janela ainda existir.
fn update(weak: &slint::Weak<AppWindow>) {
    let Some(ui) = weak.upgrade() else { return };
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (h, m) = ((secs / 3600) % 24, (secs / 60) % 60);
    ui.set_clock_text(format!("{h:02}:{m:02}").into());
}
