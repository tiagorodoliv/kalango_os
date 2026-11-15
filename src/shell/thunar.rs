//! Ação "Definir como papel de parede" no botão direito das imagens do Thunar.
//!
//! O "Set as wallpaper" do próprio Thunar (plugin `thunar-wallpaper-plugin`)
//! só sabe falar com o Xfce, o GNOME e poucos outros: no labwc ele só
//! reclama no log. A ISO esconde esse plugin (hook `0150`) e a área de
//! trabalho põe no `~/.config/Thunar/uca.xml` (as "ações personalizadas" do
//! Thunar) uma ação que chama `kalango-shell --papel-de-parede`. Sem o
//! arquivo, parte do `/etc/xdg/Thunar/uca.xml` (o padrão do Debian); as ações
//! que a pessoa já tem ficam como estão.

use std::fs;
use std::path::PathBuf;

use crate::common::i18n::tr;

/// Identificador da ação no `uca.xml` (para não repetir).
const UNIQUE_ID: &str = "kalango-papel-de-parede";

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

/// Garante a ação no `uca.xml` da pessoa (área de trabalho, ao entrar).
pub fn ensure_wallpaper_action() {
    let Some(path) = user_file() else { return };
    let xml = fs::read_to_string(&path)
        .or_else(|_| fs::read_to_string("/etc/xdg/Thunar/uca.xml"))
        .unwrap_or_else(|_| "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<actions>\n</actions>\n".into());
    let Some(new) = with_action(&xml, &action_xml()) else { return };
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
}
