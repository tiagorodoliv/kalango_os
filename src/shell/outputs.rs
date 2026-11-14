//! Vários monitores na sessão: barra e dock no monitor principal, papel de
//! parede em todos.
//!
//! A área de trabalho (barra, dock, fundo) é uma janela de tela cheia: ela
//! fica no monitor **principal** (escolhido em Configurações → Monitores,
//! `principal=` no `monitores.conf`; sem escolha, o primeiro da esquerda),
//! pelo `MoveTo` da regra `kalango-shell` no `rc.xml`, que o shell grava
//! antes de a janela aparecer. Cada monitor extra ganha uma janela só com o
//! papel de parede (`kalango-shell --fundo=<saída> --numero=<n>`, regra
//! `kalango-fundo-<saída>`). As margens da barra e da dock valem só no
//! principal.
//!
//! A cada 2 s confere os monitores (`wlr-randr`): ligou, desligou ou mudou
//! de posição, refaz as janelas de fundo; mudou o principal, a área de
//! trabalho abre outra dela mesma no lugar certo e sai.
//!
//! "Identificar" (Configurações): as Configurações tocam o arquivo
//! `$XDG_RUNTIME_DIR/kalango-identificar` e cada janela (área de trabalho e
//! fundos) mostra o número do seu monitor por 3 s.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use slint::{ComponentHandle, Timer, TimerMode};

use crate::settings::displays::{self, Output};
use crate::shell::session::Mode;
use crate::{AppWindow, Shell};

/// Origem do monitor principal (a dock se posiciona a partir dela).
static PRIMARY_ORIGIN: Mutex<(i32, i32)> = Mutex::new((0, 0));

/// Origem do monitor principal no arranjo, em px lógicos.
pub fn primary_origin() -> (i32, i32) {
    PRIMARY_ORIGIN.lock().map(|o| *o).unwrap_or((0, 0))
}

/// Arquivo que as Configurações tocam para "Identificar".
pub fn identify_file() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join("kalango-identificar"))
}

/// Pede às janelas da sessão que mostrem o número de cada monitor.
pub fn request_identify() {
    if let Some(f) = identify_file() {
        let _ = std::fs::write(f, "1");
    }
}

/// Plano dos monitores: o principal (nome, origem, número) e os extras.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub primary: (String, i32, i32, usize),
    pub extras: Vec<(String, i32, i32, usize)>,
}

/// Monta o plano a partir dos monitores e do `monitores.conf`.
pub fn plan(outputs: &[Output], config: &str) -> Option<Plan> {
    let primary = displays::primary(outputs, config)?;
    let order = displays::numbering(outputs);
    let number = |name: &str| order.iter().position(|&i| outputs[i].name == name).map_or(1, |p| p + 1);
    let extras = order
        .iter()
        .map(|&i| &outputs[i])
        .filter(|o| o.name != primary.name)
        .map(|o| (o.name.clone(), o.x, o.y, number(&o.name)))
        .collect();
    Some(Plan { primary: (primary.name.clone(), primary.x, primary.y, number(&primary.name)), extras })
}

impl Plan {
    /// Nomes dos monitores do plano (principal e extras).
    fn names(&self) -> Vec<String> {
        std::iter::once(self.primary.0.clone()).chain(self.extras.iter().map(|e| e.0.clone())).collect()
    }
}

fn read_plan() -> Option<Plan> {
    let out = Command::new("wlr-randr").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let outputs = displays::parse(&String::from_utf8_lossy(&out.stdout));
    let config = displays::config_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    plan(&outputs, &config)
}

/// Grava o plano no `rc.xml` (posições do principal e regras dos fundos).
fn write_plan(p: &Plan) -> bool {
    if let Ok(mut o) = PRIMARY_ORIGIN.lock() {
        *o = (p.primary.1, p.primary.2);
    }
    crate::shell::labwc::set_outputs(
        Some((p.primary.0.clone(), p.primary.1, p.primary.2)),
        p.extras.iter().map(|(n, x, y, _)| (n.clone(), *x, *y)).collect(),
    )
}

fn own_exe() -> PathBuf {
    std::env::current_exe().ok().filter(|p| p.exists()).unwrap_or_else(|| "kalango-shell".into())
}

/// Abre a janela de fundo de um monitor extra.
fn spawn_background(name: &str, number: usize) -> Option<Child> {
    Command::new(own_exe())
        .args([format!("--fundo={name}"), format!("--numero={number}")])
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| eprintln!("kalango-shell: falha ao abrir o fundo de {name}: {e}"))
        .ok()
}

fn modified(path: &Option<PathBuf>) -> Option<SystemTime> {
    path.as_ref().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok())
}

/// Mostra o número do monitor por 3 s quando as Configurações pedem.
/// Usado pela área de trabalho e pelas janelas de fundo.
fn watch_identify(ui: &AppWindow) -> Timer {
    let weak = ui.as_weak();
    let file = identify_file();
    let mut last = modified(&file);
    let mut hide_at: Option<std::time::Instant> = None;
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(500), move || {
        let Some(ui) = weak.upgrade() else { return };
        let now = modified(&file);
        if now != last {
            last = now;
            if now.is_some() {
                ui.global::<Shell>().set_identify_visible(true);
                hide_at = Some(std::time::Instant::now() + Duration::from_secs(3));
            }
        }
        if hide_at.is_some_and(|t| std::time::Instant::now() >= t) {
            hide_at = None;
            ui.global::<Shell>().set_identify_visible(false);
        }
    });
    timer
}

/// Janela de fundo de um monitor extra (`--fundo=<saída> --numero=<n>`).
pub fn install_background(ui: &AppWindow, name: &str, number: i32) -> Vec<Timer> {
    let shell = ui.global::<Shell>();
    shell.set_identify_number(number);
    shell.set_identify_name(name.into());
    vec![watch_identify(ui)]
}

/// Cuida dos monitores na área de trabalho da sessão real.
pub fn install(ui: &AppWindow, mode: Mode) -> Vec<Timer> {
    if mode != Mode::Desktop || !crate::shell::session::is_native() {
        return Vec::new();
    }
    let mut timers = vec![watch_identify(ui)];
    // Antes de a janela aparecer: a regra do labwc a põe no principal.
    let first = read_plan();
    if let Some(p) = &first {
        write_plan(p);
        let shell = ui.global::<Shell>();
        shell.set_identify_number(p.primary.3 as i32);
        shell.set_identify_name(p.primary.0.clone().into());
    }

    let weak = ui.as_weak();
    let mut current = first;
    let mut children: Vec<Child> = Vec::new();
    let mut started = false;
    let mut known: Vec<String> = current.as_ref().map(Plan::names).unwrap_or_default();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(2), move || {
        let Some(ui) = weak.upgrade() else { return };
        let Some(new) = read_plan() else { return };
        // Monitor que apareceu (cabo ligado de novo): o labwc o põe na
        // posição padrão; volta o que a pessoa escolheu (monitores.conf) e
        // deixa o plano para a próxima volta, já com as posições certas.
        let names = new.names();
        let arrived = names.iter().any(|n| !known.contains(n));
        known = names;
        if arrived && started {
            displays::apply_saved();
            return;
        }
        let primary_moved = current.as_ref().is_some_and(|c| (c.primary.0.as_str(), c.primary.1, c.primary.2) != (new.primary.0.as_str(), new.primary.1, new.primary.2));
        let fundo_died = children.iter_mut().any(|c| c.try_wait().ok().flatten().is_some());
        if started && Some(&new) == current.as_ref() && !fundo_died {
            return;
        }
        started = true;
        // Fecha os fundos antigos e grava o plano novo.
        for mut c in children.drain(..) {
            let _ = c.kill();
            let _ = c.wait();
        }
        if !write_plan(&new) {
            return;
        }
        let shell = ui.global::<Shell>();
        shell.set_identify_number(new.primary.3 as i32);
        shell.set_identify_name(new.primary.0.clone().into());
        if primary_moved {
            // A área de trabalho precisa ir para outro monitor: abre outra
            // (a regra com o MoveTo novo vale ao mapear) e sai.
            let _ = Command::new(own_exe()).stdin(Stdio::null()).spawn();
            let _ = slint::quit_event_loop();
            return;
        }
        for (name, _, _, n) in &new.extras {
            if let Some(c) = spawn_background(name, *n) {
                children.push(c);
            }
        }
        current = Some(new);
    });
    timers.push(timer);
    timers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(name: &str, x: i32, y: i32) -> Output {
        Output {
            name: name.into(),
            enabled: true,
            x,
            y,
            scale: 1.0,
            transform: "normal".into(),
            ..Default::default()
        }
    }

    #[test]
    fn plano_dos_monitores() {
        let outs = vec![out("HDMI-A-1", 1366, 0), out("eDP-1", 0, 0)];
        let p = plan(&outs, "").unwrap();
        assert_eq!(p.primary, ("eDP-1".into(), 0, 0, 1));
        assert_eq!(p.extras, vec![("HDMI-A-1".into(), 1366, 0, 2)]);
        let p = plan(&outs, "principal=HDMI-A-1\n").unwrap();
        assert_eq!(p.primary, ("HDMI-A-1".into(), 1366, 0, 2));
        assert_eq!(p.extras, vec![("eDP-1".into(), 0, 0, 1)]);
    }
}
