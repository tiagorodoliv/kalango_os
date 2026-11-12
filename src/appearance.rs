//! Preferências de aparência salvas em disco e sincronizadas entre processos.
//!
//! Na sessão real, as Configurações rodam num processo separado da área de
//! trabalho e do menu de apps. Cada processo confere o global `Theme` a cada
//! fração de segundo: se mudou aqui, grava em `~/.config/kalango/aparencia.conf`;
//! se o arquivo mudou (outro processo gravou), aplica. Assim trocar a cor de
//! destaque nas Configurações muda a barra e a dock na hora.
//!
//! O mesmo arquivo guarda as opções da Barra de tarefas (o que a barra
//! superior mostra, tamanho e apps da dock). Na área de trabalho da sessão
//! real, o tamanho da dock também ajusta a margem que o labwc reserva para
//! ela ([`crate::labwc`]).

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use slint::{ComponentHandle, Timer, TimerMode};

use crate::{AppWindow, Mode, Theme, ThemeMode};

/// Intervalo entre as conferências do tema e do arquivo.
const INTERVAL: Duration = Duration::from_millis(500);

/// Preferências das telas Aparência e Barra de tarefas.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub accent_index: i32,
    pub mode: ThemeMode,
    pub transparency: f32,
    pub blur: bool,
    pub dock_always_visible: bool,
    pub reduce_motion: bool,
    /// Barra superior: dia, segundos, CPU e bateria.
    pub bar_show_date: bool,
    pub bar_show_seconds: bool,
    pub bar_show_cpu: bool,
    pub bar_show_battery: bool,
    /// 0 = pequena, 1 = média, 2 = grande.
    pub dock_size: i32,
    /// Ids dos apps da dock, separados por vírgula.
    pub dock_apps: String,
}

/// As 6 cores de destaque (mesma ordem de `Theme.accents` em ui/theme.slint).
pub const ACCENTS: [&str; 6] = ["#3dd6f5", "#a78bfa", "#7ea6f7", "#5fd38d", "#e8b04b", "#f28b95"];

/// Marca do `~/.config/gtk-3.0/gtk.css` gerado pelo shell.
const GTK_MARK: &str = "/* Gerado pelo kalango-shell (Configurações → Aparência). */";

/// Texto do `gtk.css` do usuário que troca a cor de destaque do tema GTK
/// "Kalango" (`/usr/share/themes/Kalango`, cor nomeada `@kalango_accent`).
pub fn gtk_css(accent_index: i32) -> String {
    let color = ACCENTS[accent_index.clamp(0, 5) as usize];
    format!("{GTK_MARK}
@define-color kalango_accent {color};
")
}

/// Grava a cor de destaque no `gtk.css` do usuário (apps GTK abertos depois
/// já usam a cor nova). Um `gtk.css` feito à mão (sem a marca) fica intocado.
fn sync_gtk_accent(accent_index: i32) {
    let Some(base) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    else {
        return;
    };
    let path = base.join("gtk-3.0/gtk.css");
    let current = fs::read_to_string(&path).ok();
    if current.as_deref().is_some_and(|t| !t.starts_with(GTK_MARK)) {
        return;
    }
    let wanted = gtk_css(accent_index);
    if current.as_deref() == Some(wanted.as_str()) {
        return;
    }
    let _ = fs::create_dir_all(base.join("gtk-3.0"));
    if let Err(err) = fs::write(&path, wanted) {
        eprintln!("kalango-shell: falha ao gravar {}: {err}", path.display());
    }
}

/// Ajustes fora do shell que seguem as preferências (só a área de trabalho
/// faz, um processo por sessão): margem da dock no labwc e destaque do GTK.
fn sync_system(now: &Prefs, before: Option<&Prefs>) {
    if before.is_none_or(|b| b.dock_margin() != now.dock_margin()) {
        crate::labwc::set_dock_margin(now.dock_margin());
    }
    if before.is_none_or(|b| b.accent_index != now.accent_index) {
        sync_gtk_accent(now.accent_index);
    }
}

/// Nomes do tamanho da dock no arquivo (índice = `dock_size`).
const DOCK_SIZES: [&str; 3] = ["pequena", "media", "grande"];

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
            bar_show_date: t.get_bar_show_date(),
            bar_show_seconds: t.get_bar_show_seconds(),
            bar_show_cpu: t.get_bar_show_cpu(),
            bar_show_battery: t.get_bar_show_battery(),
            dock_size: t.get_dock_size(),
            dock_apps: t.get_dock_apps().into(),
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
        t.set_bar_show_date(self.bar_show_date);
        t.set_bar_show_seconds(self.bar_show_seconds);
        t.set_bar_show_cpu(self.bar_show_cpu);
        t.set_bar_show_battery(self.bar_show_battery);
        t.set_dock_size(self.dock_size);
        t.set_dock_apps(self.dock_apps.clone().into());
    }

    /// Margem inferior que o labwc reserva para a dock: o botão, a borda da
    /// dock (2 × 9 px) e a distância até o rodapé (18 px). Com a dock
    /// escondida, nenhuma (os apps maximizados ocupam até o rodapé).
    pub fn dock_margin(&self) -> u32 {
        if !self.dock_always_visible {
            return 0;
        }
        let item = match self.dock_size {
            0 => 44,
            2 => 60,
            _ => 52,
        };
        item + 18 + 18
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
             dock-sempre-visivel={}\nreduzir-animacoes={}\n\
             # Barra de tarefas\n\
             barra-data={}\nbarra-segundos={}\nbarra-cpu={}\nbarra-bateria={}\n\
             dock-tamanho={}\ndock-apps={}\n",
            self.accent_index,
            mode,
            self.transparency,
            self.blur,
            self.dock_always_visible,
            self.reduce_motion,
            self.bar_show_date,
            self.bar_show_seconds,
            self.bar_show_cpu,
            self.bar_show_battery,
            DOCK_SIZES[self.dock_size.clamp(0, 2) as usize],
            self.dock_apps
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
                "barra-data" => p.bar_show_date = value.parse().unwrap_or(p.bar_show_date),
                "barra-segundos" => {
                    p.bar_show_seconds = value.parse().unwrap_or(p.bar_show_seconds)
                }
                "barra-cpu" => p.bar_show_cpu = value.parse().unwrap_or(p.bar_show_cpu),
                "barra-bateria" => {
                    p.bar_show_battery = value.parse().unwrap_or(p.bar_show_battery)
                }
                "dock-tamanho" => {
                    if let Some(i) = DOCK_SIZES.iter().position(|s| *s == value) {
                        p.dock_size = i as i32;
                    }
                }
                "dock-apps" => {
                    let ids = crate::apps::dock_ids(value);
                    if !ids.is_empty() {
                        p.dock_apps = ids.join(",");
                    }
                }
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
/// sempre no padrão. Na área de trabalho, também mantém a margem da dock no
/// labwc. O timer devolvido precisa ficar vivo.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if !crate::session::is_native() {
        return None;
    }
    let path = config_path()?;
    if let Ok(text) = fs::read_to_string(&path) {
        Prefs::parse(&text, &Prefs::from_ui(ui)).apply(ui);
    }
    let mut last = Prefs::from_ui(ui);
    // Só a área de trabalho (um processo por sessão) mexe no labwc.
    let desktop = mode == Mode::Desktop;
    if desktop {
        sync_system(&last, None);
    }
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
            if desktop {
                sync_system(&now, Some(&last));
            }
            last = now;
        } else if modified(&path) != last_mtime {
            // Outro processo gravou: aplica aqui.
            last_mtime = modified(&path);
            if let Ok(text) = fs::read_to_string(&path) {
                let p = Prefs::parse(&text, &now);
                p.apply(&ui);
                if desktop {
                    sync_system(&p, Some(&last));
                }
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
            bar_show_date: true,
            bar_show_seconds: false,
            bar_show_cpu: true,
            bar_show_battery: true,
            dock_size: 1,
            dock_apps: "files,terminal,browser,code,store".into(),
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
            bar_show_date: false,
            bar_show_seconds: true,
            bar_show_cpu: false,
            bar_show_battery: false,
            dock_size: 2,
            dock_apps: "files,music,calendar".into(),
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

    #[test]
    fn arquivo_antigo_mantem_o_padrao_da_barra() {
        // aparencia.conf de antes da Barra de tarefas: só as chaves da Aparência.
        let p = Prefs::parse("destaque=2\ndock-tamanho=enorme\ndock-apps= , \n", &padrao());
        assert_eq!(p.accent_index, 2);
        assert_eq!(p.dock_size, 1);
        assert_eq!(p.dock_apps, "files,terminal,browser,code,store");
    }

    #[test]
    fn destaque_do_gtk() {
        assert!(gtk_css(1).ends_with("@define-color kalango_accent #a78bfa;\n"));
        assert!(gtk_css(99).contains("#f28b95"));
    }

    #[test]
    fn margem_da_dock() {
        let mut p = padrao();
        assert_eq!(p.dock_margin(), 88);
        p.dock_size = 0;
        assert_eq!(p.dock_margin(), 80);
        p.dock_size = 2;
        assert_eq!(p.dock_margin(), 96);
        p.dock_always_visible = false;
        assert_eq!(p.dock_margin(), 0);
    }
}
