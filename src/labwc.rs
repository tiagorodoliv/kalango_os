//! Ajustes do compositor (labwc) feitos pelo shell na sessão real.
//!
//! O labwc reserva a faixa da dock com `<margin bottom="...">` no `rc.xml`,
//! para apps maximizados não ficarem por cima dela. Quando a dock muda de
//! tamanho ou passa a se esconder (Configurações → Barra de tarefas), o shell
//! gera `~/.config/labwc/rc.xml`: uma cópia do arquivo do sistema
//! (`/etc/xdg/labwc/rc.xml`) só com a margem trocada, e pede ao labwc para
//! reler a configuração (`labwc --reconfigure`).
//!
//! A cópia leva uma marca na primeira linha. Um `rc.xml` do usuário sem a
//! marca (feito à mão) nunca é sobrescrito. Com a margem igual à do sistema,
//! a cópia é apagada e o labwc volta a usar o arquivo do sistema.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Marca da cópia gerada pelo shell (primeira linha depois do `<?xml ?>`).
pub const MARK: &str = "<!-- Gerado pelo kalango-shell (Configurações → Barra de tarefas). -->";

/// Troca o atributo `bottom` da tag `<margin>` do `rc.xml`. `None` se o
/// arquivo não tem a tag com esse atributo.
pub fn replace_margin(xml: &str, bottom: u32) -> Option<String> {
    let tag_start = xml.find("<margin")?;
    let tag_end = tag_start + xml[tag_start..].find('>')?;
    let attr = tag_start + xml[tag_start..tag_end].find("bottom=\"")? + "bottom=\"".len();
    let value_end = attr + xml[attr..].find('"')?;
    Some(format!("{}{bottom}{}", &xml[..attr], &xml[value_end..]))
}

/// Lê o valor de `bottom` da tag `<margin>`.
pub fn margin_of(xml: &str) -> Option<u32> {
    let tag_start = xml.find("<margin")?;
    let tag_end = tag_start + xml[tag_start..].find('>')?;
    let attr = tag_start + xml[tag_start..tag_end].find("bottom=\"")? + "bottom=\"".len();
    let value_end = attr + xml[attr..].find('"')?;
    xml[attr..value_end].trim().parse().ok()
}

/// Põe a marca da cópia gerada logo depois da declaração `<?xml ?>`.
pub fn with_mark(xml: &str) -> String {
    match xml.find("?>") {
        Some(end) if xml.trim_start().starts_with("<?xml") => {
            format!("{}\n{MARK}{}", &xml[..end + 2], &xml[end + 2..])
        }
        _ => format!("{MARK}\n{xml}"),
    }
}

/// `rc.xml` do sistema: o primeiro de `$XDG_CONFIG_DIRS` (padrão `/etc/xdg`).
fn system_rc() -> Option<PathBuf> {
    let dirs = std::env::var("XDG_CONFIG_DIRS")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/etc/xdg".into());
    dirs.split(':')
        .map(|d| PathBuf::from(d).join("labwc/rc.xml"))
        .find(|p| p.is_file())
}

/// `rc.xml` do usuário (`$XDG_CONFIG_HOME/labwc/rc.xml`).
fn user_rc() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("labwc/rc.xml"))
}

/// Ajusta a margem da dock no labwc e pede para ele reler a configuração.
/// Não faz nada fora do labwc (`$LABWC_PID` ausente) ou se o usuário tem um
/// `rc.xml` próprio.
pub fn set_dock_margin(bottom: u32) {
    if std::env::var_os("LABWC_PID").is_none() {
        return;
    }
    let (Some(system), Some(user)) = (system_rc(), user_rc()) else {
        return;
    };
    let Ok(system_xml) = fs::read_to_string(&system) else {
        return;
    };
    let current = fs::read_to_string(&user).ok();
    if current.as_deref().is_some_and(|text| !text.contains(MARK)) {
        eprintln!("kalango-shell: {} foi feito à mão; a margem da dock não muda", user.display());
        return;
    }

    let wanted = if margin_of(&system_xml) == Some(bottom) {
        // Igual ao sistema: sem cópia (atualizações do rc.xml do sistema valem).
        None
    } else {
        match replace_margin(&system_xml, bottom) {
            Some(xml) => Some(with_mark(&xml)),
            None => return,
        }
    };
    if wanted == current {
        return;
    }
    let result = match &wanted {
        Some(xml) => user
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|_| fs::write(&user, xml)),
        None => fs::remove_file(&user),
    };
    if let Err(err) = result {
        eprintln!("kalango-shell: falha ao gravar {}: {err}", user.display());
        return;
    }
    let _ = Command::new("labwc")
        .arg("--reconfigure")
        .stdin(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    const RC: &str = "<?xml version=\"1.0\"?>\n<labwc_config>\n  <margin top=\"35\" bottom=\"88\" />\n</labwc_config>\n";

    #[test]
    fn troca_so_a_margem_de_baixo() {
        let xml = replace_margin(RC, 0).unwrap();
        assert!(xml.contains("<margin top=\"35\" bottom=\"0\" />"));
        assert_eq!(margin_of(&xml), Some(0));
        assert_eq!(margin_of(RC), Some(88));
        assert!(replace_margin("<labwc_config/>", 10).is_none());
    }

    #[test]
    fn marca_depois_da_declaracao_xml() {
        let xml = with_mark(RC);
        assert!(xml.starts_with("<?xml version=\"1.0\"?>\n<!-- Gerado pelo kalango-shell"));
        assert!(xml.contains("<labwc_config>"));
        assert!(with_mark("<labwc_config/>").starts_with(MARK));
    }
}
