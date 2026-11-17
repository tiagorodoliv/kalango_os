//! Dock em janela própria, sempre por cima das janelas (como a do Ubuntu).
//!
//! A área de trabalho é uma janela de tela cheia que fica *atrás* dos apps:
//! a dock desenhada nela ficava coberta por qualquer janela que descesse até
//! o rodapé. Com a dock "sempre visível", a área de trabalho abre
//! `kalango-shell --dock --faixa=LxA`: uma janela transparente do
//! comprimento da borda e da espessura da dock (a dock centralizada dentro),
//! que o labwc põe sempre por cima, em todas as áreas de trabalho, sem
//! moldura e sem poder ser arrastada (regra `kalango-dock` do `rc.xml`). A
//! posição (`MoveTo`) depende do tamanho da tela e da dock: o shell a grava no
//! `~/.config/labwc/rc.xml` antes de abrir a janela, e abre outra quando muda.
//!
//! Dock escondida ("Sempre visível" desligado), como no Ubuntu: em vez da dock
//! fica na borda uma faixa transparente de [`TRIGGER`] px (`--dock-gatilho`,
//! app_id `kalango-dock-gatilho`), sempre por cima. O mouse encosta nela e a
//! faixa abre a dock (`--dock --ocultar`) por cima das janelas, no lugar de
//! sempre; a dock some sozinha quando o mouse sai dela ([`install_autohide`]).
//! Uma janela Wayland não sabe onde está o mouse fora dela: por isso a faixa.
//!
//! Precisa de transparência (cantos arredondados, a faixa ao lado da dock),
//! que no Wayland só o renderer OpenGL tem: só com uma placa de vídeo de
//! verdade (`hardware_gl`). Sem ela (VM, máquina sem aceleração), a dock
//! continua desenhada na área de trabalho, como antes (escondida, ela fica
//! atrás das janelas).

use std::cell::Cell;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, Timer, TimerMode};

use crate::shell::session::Mode;
use crate::{AppWindow, Shell, Theme};

/// Drivers de vídeo com aceleração de verdade (o OpenGL não sai pelo llvmpipe).
const GL_DRIVERS: [&str; 6] = ["i915", "xe", "amdgpu", "radeon", "nouveau", "nvidia"];

/// Há uma placa de vídeo com aceleração (`/sys/class/drm/card*/device/driver`).
pub fn hardware_gl() -> bool {
    std::fs::read_dir("/sys/class/drm")
        .map(|dir| {
            dir.flatten().any(|e| {
                std::fs::read_link(e.path().join("device/driver"))
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .is_some_and(|d| GL_DRIVERS.contains(&d.as_str()))
            })
        })
        .unwrap_or(false)
}

/// Medidas da dock (as mesmas de `ui/components/dock.slint`), em px lógicos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DockSpec {
    /// 0 embaixo, 1 esquerda, 2 direita (`Theme.dock-position`).
    pub position: i32,
    /// Estilo barra (de ponta a ponta) em vez de dock centralizada.
    pub bar: bool,
    /// Lado do botão de app (`Theme.dock-item`).
    pub item: f32,
    /// Espessura da dock (`Theme.dock-thickness`).
    pub thickness: f32,
    /// Apps da dock (sem contar as Configurações).
    pub count: usize,
    /// Botão do menu de programas visível (mais um item, no começo ou no fim).
    pub apps_button: bool,
    /// Folga até a borda da tela (dock solta, `Theme.dock-gap`); 0 = encostada.
    pub gap: f32,
}

/// Janela da dock na tela: posição e tamanho exatos da dock (na barra, a
/// borda inteira), encostada na borda ou a `gap` dela. `screen` em px lógicos.
pub fn geometry(screen: (u32, u32), spec: DockSpec, bar_height: u32) -> crate::shell::labwc::DockGeometry {
    let (w, h) = (screen.0 as f32, screen.1 as f32);
    let side = h - bar_height as f32;
    let pad = (spec.thickness - spec.item) / 2.0;
    // Comprimento: apps + separador (17 px) + Configurações [+ botão do menu
    // de programas, um passo a mais] + respiro. Mesma conta de `run` em
    // ui/components/dock.slint.
    let step = spec.item + 8.0;
    let run = spec.count as f32 * step + 17.0 + spec.item + if spec.apps_button { step } else { 0.0 };
    let edge = if spec.position == 0 { w } else { side };
    let along = if spec.bar { edge } else { (run + 2.0 * pad).min(edge) };
    let t = spec.thickness.round() as u32;
    let along_px = along.round() as u32;
    match spec.position {
        1 | 2 => crate::shell::labwc::DockGeometry {
            x: if spec.position == 1 { spec.gap.round() as i32 } else { (w - spec.thickness - spec.gap).round() as i32 },
            y: bar_height as i32 + ((side - along) / 2.0).round() as i32,
            width: t,
            height: along_px,
        },
        _ => crate::shell::labwc::DockGeometry {
            x: ((w - along) / 2.0).round() as i32,
            y: (h - spec.thickness - spec.gap).round() as i32,
            width: along_px,
            height: t,
        },
    }
}

/// Espessura da faixa da dock escondida, em px lógicos.
pub const TRIGGER: u32 = 2;

/// Faixa da dock escondida: [`TRIGGER`] px na borda da tela, do comprimento da
/// dock (`dock` = janela da dock). `screen` em px lógicos; `origin` = canto do
/// monitor principal.
pub fn trigger_strip(screen: (u32, u32), origin: (i32, i32), position: i32, dock: crate::shell::labwc::DockGeometry) -> crate::shell::labwc::DockGeometry {
    let t = TRIGGER;
    match position {
        1 => crate::shell::labwc::DockGeometry { x: origin.0, y: dock.y, width: t, height: dock.height },
        2 => crate::shell::labwc::DockGeometry { x: origin.0 + (screen.0 - t) as i32, y: dock.y, width: t, height: dock.height },
        _ => crate::shell::labwc::DockGeometry { x: dock.x, y: origin.1 + (screen.1 - t) as i32, width: dock.width, height: t },
    }
}

/// O que a área de trabalho mantém aberto na borda.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Edge {
    /// A dock, sempre visível (reserva a margem).
    Dock,
    /// A faixa da dock escondida (a dock abre por cima quando o mouse encosta).
    Trigger,
}

/// O próprio executável (o caminho de quem está rodando).
fn own_exe() -> std::path::PathBuf {
    std::env::current_exe().ok().filter(|p| p.exists()).unwrap_or_else(|| "kalango-shell".into())
}

/// Abre a janela da dock (ou a faixa da dock escondida) com o tamanho dela.
fn spawn(edge: Edge, g: crate::shell::labwc::DockGeometry, dock: crate::shell::labwc::DockGeometry) -> Option<Child> {
    let mut cmd = Command::new(own_exe());
    match edge {
        Edge::Dock => cmd.args(["--dock".to_string(), format!("--faixa={}x{}", g.width, g.height)]),
        Edge::Trigger => cmd.args([
            "--dock-gatilho".to_string(),
            format!("--faixa={}x{}", g.width, g.height),
            format!("--dock-faixa={}x{}", dock.width, dock.height),
        ]),
    };
    cmd.stdin(Stdio::null())
        .spawn()
        .map_err(|e| eprintln!("kalango-shell: falha ao abrir a dock: {e}"))
        .ok()
}

/// Cuida da janela da dock (só a área de trabalho da sessão real, com placa
/// de vídeo). O timer devolvido precisa ficar vivo.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if mode != Mode::Desktop || !crate::shell::session::is_native() || !hardware_gl() {
        return None;
    }
    let weak = ui.as_weak();
    // O que está aberto agora (dock ou faixa, a posição dela e a da dock) e o processo.
    let mut current: Option<(Edge, crate::shell::labwc::DockGeometry, crate::shell::labwc::DockGeometry)> = None;
    let mut child: Option<Child> = None;
    // Janelas antigas: a nova as fecha depois de aparecer (session::claim_instance).
    let mut retired: Vec<Child> = Vec::new();
    let mut failures = 0u32;
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(800), move || {
        let Some(ui) = weak.upgrade() else { return };
        let theme = ui.global::<Theme>();
        let shell = ui.global::<Shell>();
        let size = ui.window().size();
        let scale = ui.window().scale_factor().max(0.1);
        let screen = ((size.width as f32 / scale) as u32, (size.height as f32 / scale) as u32);

        // Dock escondida (aparece pela borda) ou janela ainda sem tamanho:
        // fica na área de trabalho.
        let spec = DockSpec {
            position: theme.get_dock_position(),
            bar: theme.get_dock_style() == 1,
            item: theme.get_dock_item(),
            thickness: theme.get_dock_thickness(),
            // Fixados + abertos que não estão fixados.
            count: ui.global::<crate::Apps>().get_dock().row_count() + shell.get_running_extra().row_count(),
            apps_button: theme.get_dock_apps_button() != 2,
            gap: theme.get_dock_gap(),
        };
        // No monitor principal (com vários monitores, a origem dele).
        let (ox, oy) = crate::shell::outputs::primary_origin();
        let wanted = (screen.0 > 0 && failures < 3).then(|| {
            let g = geometry(screen, spec, theme.get_bar_height() as u32);
            let dock = crate::shell::labwc::DockGeometry { x: g.x + ox, y: g.y + oy, ..g };
            if theme.get_dock_always_visible() {
                (Edge::Dock, dock, dock)
            } else {
                (Edge::Trigger, trigger_strip(screen, (ox, oy), spec.position, dock), dock)
            }
        });

        // A janela caiu: abre de novo (até 3 vezes; depois desiste e a dock
        // volta para a área de trabalho).
        if let Some(c) = child.as_mut() {
            if c.try_wait().ok().flatten().is_some() {
                child = None;
                current = None;
                failures += 1;
            }
        }

        retired.retain_mut(|c| c.try_wait().ok().flatten().is_none());
        if wanted != current {
            if let Some(mut c) = child.take() {
                if wanted.is_some() {
                    // Outra dock vai abrir: a antiga fica até a nova aparecer.
                    retired.push(c);
                } else {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
            current = wanted;
            match wanted {
                Some((edge, g, dock)) => {
                    // Posições e tamanhos no rc.xml antes de abrir (as regras
                    // valem ao mapear a janela). A da dock fica gravada também
                    // com ela escondida: é onde a faixa a abre.
                    let trigger = (edge == Edge::Trigger).then_some(g);
                    if crate::shell::labwc::set_dock_geometry(Some(dock))
                        && crate::shell::labwc::set_dock_trigger(trigger)
                    {
                        child = spawn(edge, g, dock);
                    }
                }
                None => {
                    crate::shell::labwc::set_dock_geometry(None);
                    crate::shell::labwc::set_dock_trigger(None);
                }
            }
        }
        shell.set_dock_external(child.is_some());
    });
    Some(timer)
}

/// Faixa da dock escondida (`--dock-gatilho`): o mouse encostou → abre a dock
/// (`--dock --ocultar`, do tamanho de `--dock-faixa=`) se ela não está aberta.
///
/// Além do aviso na hora em que o mouse entra, um timer confere a cada 300 ms:
/// com a dock solta da borda, o mouse pode ficar parado na faixa depois de a
/// dock sumir, e entrar de novo não aconteceria. O timer devolvido precisa
/// ficar vivo.
pub fn install_trigger(ui: &AppWindow) -> Option<Timer> {
    let (w, h) = crate::shell::session::dock_strip_from_args(std::env::args())?;
    let shell = ui.global::<Shell>();
    // Faixa: o mouse em cima dela (o mesmo `dock-hovered` da dock).
    let last = Rc::new(Cell::new(None::<Instant>));
    let reveal = {
        let last = last.clone();
        move || {
            // Uma vez por segundo no máximo (a dock leva um instante para abrir).
            if last.get().is_some_and(|t| t.elapsed() < Duration::from_millis(1000)) || crate::shell::session::dock_running() {
                return;
            }
            last.set(Some(Instant::now()));
            let _ = Command::new(own_exe())
                .args(["--dock".to_string(), format!("--faixa={w}x{h}"), "--ocultar".to_string()])
                .stdin(Stdio::null())
                .spawn();
        }
    };
    shell.on_reveal_dock(reveal.clone());
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(300), move || {
        if weak.upgrade().is_some_and(|ui| ui.global::<Shell>().get_dock_hovered()) {
            reveal();
        }
    });
    Some(timer)
}

/// Dock aberta pela faixa (`--dock --ocultar`): fecha sozinha 0,9 s depois de
/// o mouse sair dela, ou em 2,5 s se ele nem chegou a entrar. O timer
/// devolvido precisa ficar vivo.
pub fn install_autohide(ui: &AppWindow) -> Option<Timer> {
    let weak = ui.as_weak();
    let start = Instant::now();
    let mut seen = false;
    let mut last_hover = start;
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(ui) = weak.upgrade() else { return };
        if ui.global::<Shell>().get_dock_hovered() {
            seen = true;
            last_hover = Instant::now();
            return;
        }
        let gone = if seen { last_hover.elapsed() > Duration::from_millis(900) } else { start.elapsed() > Duration::from_millis(2500) };
        if gone {
            let _ = slint::quit_event_loop();
        }
    });
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn janela_do_tamanho_da_dock() {
        // 5 apps, botão de 36 px, dock (respiro 9): 5×44 + 17 + 36 + 18 = 291.
        let spec = DockSpec { position: 0, bar: false, item: 36.0, thickness: 54.0, count: 5, apps_button: false, gap: 0.0 };
        let g = geometry((1366, 768), spec, 35);
        assert_eq!((g.x, g.y, g.width, g.height), (538, 714, 291, 54));
        // Barra: a borda inteira.
        let g = geometry((1366, 768), DockSpec { bar: true, thickness: 48.0, ..spec }, 35);
        assert_eq!((g.x, g.y, g.width, g.height), (0, 720, 1366, 48));
        // Esquerda: abaixo da barra superior, centralizada na altura.
        let g = geometry((1366, 768), DockSpec { position: 1, ..spec }, 35);
        assert_eq!((g.x, g.width, g.height), (0, 54, 291));
        assert_eq!(g.y, 35 + ((733 - 291) as f32 / 2.0).round() as i32);
    }

    #[test]
    fn botao_de_programas_e_dock_solta() {
        let spec = DockSpec { position: 0, bar: false, item: 36.0, thickness: 54.0, count: 5, apps_button: true, gap: 8.0 };
        // Faixa da dock escondida: 2 px na borda, do comprimento da dock.
        let dock = geometry((1366, 768), spec, 35);
        let t = trigger_strip((1366, 768), (0, 0), 0, dock);
        assert_eq!((t.x, t.y, t.width, t.height), (dock.x, 766, dock.width, 2));
        let t = trigger_strip((1366, 768), (2560, 0), 2, dock);
        assert_eq!((t.x, t.width), (2560 + 1364, 2));
        // Mais um passo (36 + 8) no comprimento e 8 px acima da borda.
        let g = geometry((1366, 768), spec, 35);
        assert_eq!((g.width, g.height), (335, 54));
        assert_eq!((g.x, g.y), (516, 768 - 54 - 8));
        // Nas laterais, afastada da borda pela folga.
        assert_eq!(geometry((1366, 768), DockSpec { position: 1, ..spec }, 35).x, 8);
        assert_eq!(geometry((1366, 768), DockSpec { position: 2, ..spec }, 35).x, 1366 - 54 - 8);
    }
}
