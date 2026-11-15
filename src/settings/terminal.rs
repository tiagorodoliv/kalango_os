//! Temas do terminal (foot): Configurações → Aparência → Terminal.
//!
//! Os temas ficam em `/usr/share/kalango/terminal/*.ini` (nome em inglês e
//! português nos comentários `# name:`/`# nome:` e a seção `[colors]`). Escolher
//! um tema grava `~/.config/foot/foot.ini` com uma marca: as configurações
//! gerais do arquivo do sistema (fonte, espaçamento) mais as cores do tema. O
//! foot lê um só arquivo (o do usuário esconde o do sistema); com o tema
//! padrão, a cópia é apagada. Um `foot.ini` feito à mão (sem a marca) nunca é
//! sobrescrito. Vale para os terminais abertos depois.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::{Color, ComponentHandle, ModelRc, SharedString, VecModel};

use crate::common::i18n::tr;
use crate::{AppWindow, Mode, Shell, TerminalTheme};

/// Pasta dos temas instalados.
pub const THEMES_DIR: &str = "/usr/share/kalango/terminal";
/// Tema padrão (as cores do `/etc/xdg/foot/foot.ini`).
pub const DEFAULT_THEME: &str = "kalango";
/// Marca do `foot.ini` gerado (a linha seguinte guarda o tema).
pub const MARK: &str = "# Gerado pelo kalango-shell (Configurações → Aparência → Terminal).";

/// Um tema lido do disco.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Theme {
    /// Nome do arquivo sem o número de ordem nem `.ini` ("dracula").
    pub id: String,
    pub name_en: String,
    pub name_pt: String,
    /// Seção `[colors]` inteira (do cabeçalho ao fim).
    pub colors: String,
}

/// Id do tema a partir do nome do arquivo ("20-dracula.ini" → "dracula").
pub fn theme_id(file_name: &str) -> String {
    let stem = file_name.trim_end_matches(".ini");
    match stem.split_once('-') {
        Some((n, rest)) if n.chars().all(|c| c.is_ascii_digit()) => rest.to_string(),
        _ => stem.to_string(),
    }
}

/// Lê um tema (`None` sem a seção `[colors]`).
pub fn parse_theme(file_name: &str, text: &str) -> Option<Theme> {
    let start = text.find("[colors]")?;
    let comment = |key: &str| {
        text.lines()
            .find_map(|l| l.strip_prefix('#').map(str::trim).and_then(|l| l.strip_prefix(key)).map(|v| v.trim().to_string()))
    };
    let id = theme_id(file_name);
    let name_en = comment("name:").unwrap_or_else(|| id.clone());
    let name_pt = comment("nome:").unwrap_or_else(|| name_en.clone());
    Some(Theme { id, name_en, name_pt, colors: text[start..].trim_end().to_string() + "\n" })
}

/// Cor `chave=rrggbb` da seção de cores.
pub fn color_of(colors: &str, key: &str) -> Option<Color> {
    let hex = colors.lines().find_map(|l| l.trim().strip_prefix(key)?.trim().strip_prefix('=').map(str::trim))?;
    let v = u32::from_str_radix(hex.get(..6)?, 16).ok()?;
    Some(Color::from_rgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// Temas de uma pasta, na ordem dos arquivos.
pub fn load_from(dir: &Path) -> Vec<Theme> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ini")).collect())
        .unwrap_or_default();
    files.sort();
    files
        .iter()
        .filter_map(|p| parse_theme(&p.file_name()?.to_string_lossy(), &fs::read_to_string(p).ok()?))
        .collect()
}

/// Parte geral do `foot.ini` do sistema (tudo antes de `[colors]`).
pub fn base_settings(system_ini: &str) -> String {
    let end = system_ini.find("[colors]").unwrap_or(system_ini.len());
    system_ini[..end].trim_end().to_string() + "\n"
}

/// Texto do `foot.ini` do usuário com o tema.
pub fn user_ini(system_ini: &str, theme: &Theme) -> String {
    format!("{MARK}\n# tema: {}\n{}\n{}", theme.id, base_settings(system_ini), theme.colors)
}

/// Tema gravado num `foot.ini` gerado (`None` = feito à mão ou sem tema).
pub fn theme_in(user_ini: &str) -> Option<String> {
    if !user_ini.starts_with(MARK) {
        return None;
    }
    user_ini.lines().find_map(|l| l.strip_prefix("# tema:").map(|t| t.trim().to_string()))
}

fn user_ini_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("foot/foot.ini"))
}

/// Tema atual do usuário ("kalango" sem cópia; "" = arquivo feito à mão).
fn current_theme() -> String {
    match user_ini_path().and_then(|p| fs::read_to_string(p).ok()) {
        None => DEFAULT_THEME.into(),
        Some(text) => theme_in(&text).unwrap_or_default(),
    }
}

/// Grava o tema escolhido (`Err` = mensagem).
fn apply(themes: &[Theme], id: &str) -> Result<(), String> {
    let path = user_ini_path().ok_or("HOME")?;
    if let Ok(text) = fs::read_to_string(&path) {
        if theme_in(&text).is_none() {
            return Err(tr(
                "Your ~/.config/foot/foot.ini was edited by hand; it was kept.",
                "O seu ~/.config/foot/foot.ini foi feito à mão e foi mantido.",
            )
            .into());
        }
    }
    if id == DEFAULT_THEME {
        let _ = fs::remove_file(&path);
        return Ok(());
    }
    let theme = themes.iter().find(|t| t.id == id).ok_or("tema")?;
    let system = fs::read_to_string("/etc/xdg/foot/foot.ini").unwrap_or_default();
    fs::create_dir_all(path.parent().ok_or("foot")?).map_err(|e| e.to_string())?;
    fs::write(&path, user_ini(&system, theme)).map_err(|e| e.to_string())
}

/// Lista para a UI (cores da prévia).
pub fn to_ui(themes: &[Theme]) -> Vec<TerminalTheme> {
    let black = Color::from_rgb_u8(0, 0, 0);
    themes
        .iter()
        .map(|t| {
            let c = |k: &str| color_of(&t.colors, k).unwrap_or(black);
            TerminalTheme {
                id: t.id.clone().into(),
                name: tr(&t.name_en, &t.name_pt).to_string().into(),
                bg: c("background"),
                fg: c("foreground"),
                red: c("regular1"),
                green: c("regular2"),
                yellow: c("regular3"),
                blue: c("regular4"),
                magenta: c("regular5"),
                cyan: c("regular6"),
            }
        })
        .collect()
}

/// Preenche a lista de temas (Configurações).
pub fn install(ui: &AppWindow, mode: Mode) {
    if mode != Mode::Settings {
        return;
    }
    let themes = load_from(Path::new(THEMES_DIR));
    if themes.is_empty() {
        return;
    }
    let shell = ui.global::<Shell>();
    shell.set_terminal_themes(ModelRc::from(Rc::new(VecModel::from(to_ui(&themes)))));
    shell.set_terminal_theme(current_theme().into());
    let weak = ui.as_weak();
    shell.on_set_terminal_theme(move |id| {
        let Some(ui) = weak.upgrade() else { return };
        let shell = ui.global::<Shell>();
        match apply(&themes, &id) {
            Ok(()) => {
                shell.set_terminal_theme(id);
                shell.set_terminal_message(
                    tr("New terminal windows will use this theme.", "As novas janelas do terminal vão usar este tema.").into(),
                );
            }
            Err(e) => shell.set_terminal_message(SharedString::from(e)),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRACULA: &str = "# Tema\n# name: Dracula\n# nome: Drácula\n[colors]\nbackground=282a36\nforeground=f8f8f2\nregular1=ff5555\n";
    const SYSTEM: &str = "# Terminal do KalangOS\nfont=JetBrains Mono:size=11\npad=10x8\n\n[colors]\nbackground=07090d\n";

    #[test]
    fn le_tema_e_cores() {
        let t = parse_theme("20-dracula.ini", DRACULA).unwrap();
        assert_eq!(t.id, "dracula");
        assert_eq!((t.name_en.as_str(), t.name_pt.as_str()), ("Dracula", "Drácula"));
        assert!(t.colors.starts_with("[colors]\n"));
        assert_eq!(color_of(&t.colors, "background"), Some(Color::from_rgb_u8(0x28, 0x2a, 0x36)));
        assert_eq!(color_of(&t.colors, "regular1"), Some(Color::from_rgb_u8(0xff, 0x55, 0x55)));
        assert_eq!(color_of(&t.colors, "regular9"), None);
        assert!(parse_theme("x.ini", "sem cores").is_none());
    }

    #[test]
    fn foot_ini_do_usuario() {
        let t = parse_theme("20-dracula.ini", DRACULA).unwrap();
        let ini = user_ini(SYSTEM, &t);
        assert!(ini.starts_with(MARK));
        assert!(ini.contains("font=JetBrains Mono:size=11\npad=10x8\n"));
        assert!(!ini.contains("07090d"));
        assert!(ini.contains("background=282a36"));
        assert_eq!(theme_in(&ini), Some("dracula".into()));
        assert_eq!(theme_in("[colors]\nbackground=000000\n"), None);
    }

    #[test]
    fn temas_da_distro() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("distro/config/includes.chroot/usr/share/kalango/terminal");
        let themes = load_from(&dir);
        assert_eq!(themes.len(), 10);
        assert_eq!(themes[0].id, DEFAULT_THEME);
        assert!(themes.iter().all(|t| color_of(&t.colors, "background").is_some() && color_of(&t.colors, "regular6").is_some()));
    }
}
