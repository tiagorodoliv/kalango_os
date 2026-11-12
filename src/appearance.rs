//! Preferências de aparência salvas em disco e sincronizadas entre processos.
//!
//! Na sessão real, as Configurações rodam num processo separado da área de
//! trabalho e do menu de apps. Cada processo confere o global `Theme` a cada
//! fração de segundo: se mudou aqui, grava em `~/.config/kalango/aparencia.conf`;
//! se o arquivo mudou (outro processo gravou), aplica. Assim trocar a cor de
//! destaque nas Configurações muda a barra e a dock na hora.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use slint::{ComponentHandle, Timer, TimerMode};

use crate::{AppWindow, Theme, ThemeMode};

/// Intervalo entre as conferências do tema e do arquivo.
const INTERVAL: Duration = Duration::from_millis(500);

/// Preferências da tela Aparência.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub accent_index: i32,
    pub mode: ThemeMode,
    pub transparency: f32,
    pub blur: bool,
    pub dock_always_visible: bool,
    pub reduce_motion: bool,
}

impl Prefs {
    /// Lê as preferências atuais do global `Theme`.
    fn from_ui(ui: &AppWindow) -> Prefs {
        let t = ui.global::<Theme>();
        Prefs {
            accent_index: t.get_accent_index(),
            mode: t.get_mode(),
            transparency: t.get_transparency(),
            blur: t.get_blur(),
            dock_always_visible: t.get_dock_always_visible(),
            reduce_motion: t.get_reduce_motion(),
        }
    }

    /// Aplica as preferências ao global `Theme`.
    fn apply(&self, ui: &AppWindow) {
        let t = ui.global::<Theme>();
        t.set_accent_index(self.accent_index);
        t.set_mode(self.mode);
        t.set_transparency(self.transparency);
        t.set_blur(self.blur);
        t.set_dock_always_visible(self.dock_always_visible);
        t.set_reduce_motion(self.reduce_motion);
    }

    /// Texto do arquivo (`chave=valor`, uma por linha).
    pub fn to_text(&self) -> String {
        let mode = match self.mode {
            ThemeMode::Oled => "oled",
            ThemeMode::Light => "claro",
            _ => "vidro",
        };
        format!(
            "# Aparência do Kalango OS (gravado pelas Configurações).\n\
             destaque={}\ntema={}\ntransparencia={}\ndesfoque={}\n\
             dock-sempre-visivel={}\nreduzir-animacoes={}\n",
            self.accent_index,
            mode,
            self.transparency,
            self.blur,
            self.dock_always_visible,
            self.reduce_motion
        )
    }

    /// Lê o texto do arquivo por cima de `base`; chaves ausentes ou inválidas
    /// mantêm o valor de `base`.
    pub fn parse(text: &str, base: &Prefs) -> Prefs {
        let mut p = base.clone();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "destaque" => {
                    if let Ok(v) = value.parse::<i32>() {
                        p.accent_index = v.clamp(0, 5);
                    }
                }
                "tema" => {
                    p.mode = match value {
                        "oled" => ThemeMode::Oled,
                        "claro" => ThemeMode::Light,
                        "vidro" => ThemeMode::Glass,
                        _ => p.mode,
                    }
                }
                "transparencia" => {
                    if let Ok(v) = value.parse::<f32>() {
                        p.transparency = v.clamp(0.0, 1.0);
                    }
                }
                "desfoque" => p.blur = value.parse().unwrap_or(p.blur),
                "dock-sempre-visivel" => {
                    p.dock_always_visible = value.parse().unwrap_or(p.dock_always_visible)
                }
                "reduzir-animacoes" => p.reduce_motion = value.parse().unwrap_or(p.reduce_motion),
                _ => {}
            }
        }
        p
    }
}

/// Caminho do arquivo: `$XDG_CONFIG_HOME/kalango/aparencia.conf`
/// (ou `~/.config/...`). `None` sem pasta pessoal.
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("kalango/aparencia.conf"))
}

fn modified(path: &PathBuf) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Carrega o arquivo (se existir) e mantém tema e arquivo em sincronia.
///
/// Só roda na sessão real: no desenvolvimento e nas capturas o tema começa
/// sempre no padrão. O timer devolvido precisa ficar vivo.
pub fn install(ui: &AppWindow) -> Option<Timer> {
    if !crate::session::is_native() {
        return None;
    }
    let path = config_path()?;
    if let Ok(text) = fs::read_to_string(&path) {
        Prefs::parse(&text, &Prefs::from_ui(ui)).apply(ui);
    }
    let mut last = Prefs::from_ui(ui);
    let mut last_mtime = modified(&path);

    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, INTERVAL, move || {
        let Some(ui) = weak.upgrade() else { return };
        let now = Prefs::from_ui(&ui);
        if now != last {
            // Mudou aqui (tela Aparência): grava para os outros processos.
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if let Err(err) = fs::write(&path, now.to_text()) {
                eprintln!("kalango-shell: falha ao salvar a aparência: {err}");
            }
            last_mtime = modified(&path);
            last = now;
        } else if modified(&path) != last_mtime {
            // Outro processo gravou: aplica aqui.
            last_mtime = modified(&path);
            if let Ok(text) = fs::read_to_string(&path) {
                let p = Prefs::parse(&text, &now);
                p.apply(&ui);
                last = p;
            }
        }
    });
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padrao() -> Prefs {
        Prefs {
            accent_index: 0,
            mode: ThemeMode::Glass,
            transparency: 0.6,
            blur: true,
            dock_always_visible: true,
            reduce_motion: false,
        }
    }

    #[test]
    fn grava_e_le_de_volta() {
        let p = Prefs {
            accent_index: 3,
            mode: ThemeMode::Light,
            transparency: 0.25,
            blur: false,
            dock_always_visible: false,
            reduce_motion: true,
        };
        assert_eq!(Prefs::parse(&p.to_text(), &padrao()), p);
    }

    #[test]
    fn ignora_valores_invalidos() {
        let p = Prefs::parse("destaque=99\ntema=xyz\nlixo\ndesfoque=talvez\n", &padrao());
        assert_eq!(p.accent_index, 5);
        assert_eq!(p.mode, ThemeMode::Glass);
        assert!(p.blur);
    }
}
