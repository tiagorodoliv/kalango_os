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
//! Precisa de transparência (cantos arredondados, a faixa ao lado da dock),
//! que no Wayland só o renderer OpenGL tem: só com uma placa de vídeo de
//! verdade (`hardware_gl`). Sem ela (VM, máquina sem aceleração), e com a
//! dock escondida (aparece ao chegar com o mouse na borda), a dock continua
//! desenhada na área de trabalho, como antes.

use std::process::{Child, Command, Stdio};
use std::time::Duration;

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
}

/// Janela da dock na tela: posição e tamanho exatos da dock (na barra, a
/// borda inteira), encostada na borda. `screen` em px lógicos.
pub fn geometry(screen: (u32, u32), spec: DockSpec, bar_height: u32) -> crate::shell::labwc::DockGeometry {
    let (w, h) = (screen.0 as f32, screen.1 as f32);
    let side = h - bar_height as f32;
    let pad = (spec.thickness - spec.item) / 2.0;
    // Comprimento: apps + separador (17 px) + Configurações + respiro.
    let run = spec.count as f32 * (spec.item + 8.0) + 17.0 + spec.item;
    let edge = if spec.position == 0 { w } else { side };
    let along = if spec.bar { edge } else { (run + 2.0 * pad).min(edge) };
    let t = spec.thickness.round() as u32;
    let along_px = along.round() as u32;
    match spec.position {
        1 | 2 => crate::shell::labwc::DockGeometry {
            x: if spec.position == 1 { 0 } else { (w - spec.thickness).round() as i32 },
            y: bar_height as i32 + ((side - along) / 2.0).round() as i32,
            width: t,
            height: along_px,
        },
        _ => crate::shell::labwc::DockGeometry {
            x: ((w - along) / 2.0).round() as i32,
            y: (h - spec.thickness).round() as i32,
            width: along_px,
            height: t,
        },
    }
}

/// Abre a janela da dock com o tamanho dela.
fn spawn(width: u32, height: u32) -> Option<Child> {
    let exe = std::env::current_exe().ok().filter(|p| p.exists()).unwrap_or_else(|| "kalango-shell".into());
    Command::new(exe)
        .args(["--dock", &format!("--faixa={width}x{height}")])
        .stdin(Stdio::null())
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
    // Posição e tamanho da janela aberta agora e o processo dela.
    let mut current: Option<crate::shell::labwc::DockGeometry> = None;
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
        };
        // No monitor principal (com vários monitores, a origem dele).
        let (ox, oy) = crate::shell::outputs::primary_origin();
        let wanted = (theme.get_dock_always_visible() && screen.0 > 0 && failures < 3).then(|| {
            let g = geometry(screen, spec, theme.get_bar_height() as u32);
            crate::shell::labwc::DockGeometry { x: g.x + ox, y: g.y + oy, ..g }
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
                Some(g) => {
                    // Posição e tamanho no rc.xml antes de abrir (a regra vale
                    // ao mapear a janela).
                    if crate::shell::labwc::set_dock_geometry(Some(g)) {
                        child = spawn(g.width, g.height);
                    }
                }
                None => {
                    crate::shell::labwc::set_dock_geometry(None);
                }
            }
        }
        shell.set_dock_external(child.is_some());
    });
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn janela_do_tamanho_da_dock() {
        // 5 apps, botão de 36 px, dock (respiro 9): 5×44 + 17 + 36 + 18 = 291.
        let spec = DockSpec { position: 0, bar: false, item: 36.0, thickness: 54.0, count: 5 };
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
}
