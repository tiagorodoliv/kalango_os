//! Ação "Definir como papel de parede" no botão direito das imagens do Thunar.
//!
//! O "Set as wallpaper" do próprio Thunar (plugin `thunar-wallpaper-plugin`)
//! só sabe falar com o Xfce, o GNOME e poucos outros: no labwc ele só
//! reclama no log. A ISO esconde esse plugin (hook `0150`) e a área de
//! trabalho põe no `~/.config/Thunar/uca.xml` (as "ações personalizadas" do
//! Thunar) uma ação que chama `kalango-shell --papel-de-parede`. Sem o
//! arquivo, parte do `/etc/xdg/Thunar/uca.xml` (o padrão do Debian); as ações
//! que a pessoa já tem ficam como estão.
//!
//! "Abrir terminal aqui" (a ação padrão do Debian) chama
//! `exo-open --launch TerminalEmulator`, que no Xfce 4.20 precisa do pacote
//! `xfce4-helpers` e, sem ele, falha calada. O shell troca esse comando pelo
//! foot ([`TERMINAL_COMMAND`]); ações que a pessoa mudou ficam como estão.

use std::fs;
use std::path::PathBuf;

use crate::common::i18n::tr;

/// Identificador da ação no `uca.xml` (para não repetir).
const UNIQUE_ID: &str = "kalango-papel-de-parede";

/// Comando do "Abrir terminal aqui" no padrão do Debian (não funciona aqui).
const EXO_TERMINAL: &str = "exo-open --working-directory %f --launch TerminalEmulator";

/// O mesmo com o terminal do KalangOS.
pub const TERMINAL_COMMAND: &str = "foot -D %f";

/// `uca.xml` com o "Abrir terminal aqui" no foot (e o nome no idioma), ou
/// `None` se não há a ação do exo-open.
pub fn with_foot_terminal(xml: &str) -> Option<String> {
    let old = format!("<command>{EXO_TERMINAL}</command>");
    if !xml.contains(&old) {
        return None;
    }
    let xml = xml.replace(&old, &format!("<command>{TERMINAL_COMMAND}</command>"));
    Some(xml.replace(
        "<name>Open Terminal Here</name>",
        &format!("<name>{}</name>", tr("Open Terminal Here", "Abrir terminal aqui")),
    ))
}

/// XML da ação, no idioma da sessão.
fn action_xml() -> String {
    format!(
        "<action>\n\t<icon>preferences-desktop-wallpaper</icon>\n\t<name>{}</name>\n\t<submenu></submenu>\n\
         \t<unique-id>{UNIQUE_ID}</unique-id>\n\t<command>kalango-shell --papel-de-parede %f</command>\n\
         \t<description>{}</description>\n\t<range>*</range>\n\
         \t<patterns>*.jpg;*.jpeg;*.png;*.JPG;*.JPEG;*.PNG</patterns>\n\t<image-files/>\n</action>\n",
        tr("Set as wallpaper", "Definir como papel de parede"),
        tr("Use this image as the desktop background", "Usar esta imagem como fundo da área de trabalho"),
    )
}

/// `uca.xml` com a ação acrescentada antes de `</actions>`, ou `None` se ela
/// já está lá (ou o arquivo não tem a lista de ações).
pub fn with_action(xml: &str, action: &str) -> Option<String> {
    if xml.contains(UNIQUE_ID) {
        return None;
    }
    let at = xml.rfind("</actions>")?;
    Some(format!("{}{}{}", &xml[..at], action, &xml[at..]))
}

fn user_file() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("Thunar/uca.xml"))
}

/// Garante a ação no `uca.xml` da pessoa e o terminal no foot (área de
/// trabalho, ao entrar).
pub fn ensure_wallpaper_action() {
    let Some(path) = user_file() else { return };
    let xml = fs::read_to_string(&path)
        .or_else(|_| fs::read_to_string("/etc/xdg/Thunar/uca.xml"))
        .unwrap_or_else(|_| "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n</actions>\n".into());
    let fixed = with_foot_terminal(&xml);
    let base = fixed.as_deref().unwrap_or(&xml);
    let Some(new) = with_action(base, &action_xml()).or(fixed) else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Err(err) = fs::write(&path, new) {
        eprintln!("kalango-shell: Thunar uca.xml: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acrescenta_a_acao_uma_vez() {
        let xml = "<?xml version=\"1.0\"?>\n<actions>\n<action><name>Abrir terminal</name></action>\n</actions>\n";
        let a = action_xml();
        let novo = with_action(xml, &a).unwrap();
        assert!(novo.contains("Abrir terminal") && novo.contains(UNIQUE_ID));
        assert!(novo.trim_end().ends_with("</actions>"));
        assert_eq!(with_action(&novo, &a), None);
        assert_eq!(with_action("lixo", &a), None);
    }

    #[test]
    fn terminal_aqui_abre_no_foot() {
        let xml = "<actions>\n<action>\n\t<name>Open Terminal Here</name>\n\t\
                   <command>exo-open --working-directory %f --launch TerminalEmulator</command>\n</action>\n</actions>\n";
        let novo = with_foot_terminal(xml).unwrap();
        assert!(novo.contains("<command>foot -D %f</command>") && !novo.contains("exo-open"));
        // Já corrigido (ou mudado pela pessoa): não mexe.
        assert_eq!(with_foot_terminal(&novo), None);
    }
}
