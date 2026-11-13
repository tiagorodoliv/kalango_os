//! Ajustes do compositor (labwc) feitos pelo shell na sessão real.
//!
//! O labwc reserva a faixa da dock com `<margin bottom/left/right="...">` no
//! `rc.xml`, para apps maximizados não ficarem por cima dela. Quando a dock
//! muda de tamanho, de borda, de estilo ou passa a se esconder
//! (Configurações → Barra de tarefas), o shell gera `~/.config/labwc/rc.xml`:
//! uma cópia do arquivo do sistema (`/etc/xdg/labwc/rc.xml`) só com as
//! margens trocadas, e pede ao labwc para
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

/// Margens que o labwc reserva nas bordas (a de cima é sempre a da barra).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Margins {
    pub bottom: u32,
    pub left: u32,
    pub right: u32,
}

impl Margins {
    fn get(&self, attr: &str) -> u32 {
        match attr {
            "bottom" => self.bottom,
            "left" => self.left,
            _ => self.right,
        }
    }
}

/// Posição (início, fim) da tag `<margin ...>` no texto.
fn margin_tag(xml: &str) -> Option<(usize, usize)> {
    let start = xml.find("<margin")?;
    Some((start, start + xml[start..].find('>')?))
}

/// Troca (ou acrescenta) um atributo numérico da tag `<margin>`.
fn set_attr(xml: &str, attr: &str, value: u32) -> Option<String> {
    let (start, end) = margin_tag(xml)?;
    let key = format!(" {attr}=\"");
    match xml[start..end].find(&key) {
        Some(pos) => {
            let from = start + pos + key.len();
            let to = from + xml[from..].find('"')?;
            Some(format!("{}{value}{}", &xml[..from], &xml[to..]))
        }
        None => {
            let at = start + "<margin".len();
            Some(format!("{}{key}{value}\"{}", &xml[..at], &xml[at..]))
        }
    }
}

/// Troca as margens de baixo, da esquerda e da direita da tag `<margin>`
/// do `rc.xml`. `None` se o arquivo não tem a tag.
pub fn replace_margins(xml: &str, m: Margins) -> Option<String> {
    ["bottom", "left", "right"]
        .iter()
        .try_fold(xml.to_string(), |text, attr| set_attr(&text, attr, m.get(attr)))
}

/// Lê as margens da tag `<margin>` (atributo ausente = 0).
pub fn margins_of(xml: &str) -> Option<Margins> {
    let (start, end) = margin_tag(xml)?;
    let tag = &xml[start..end];
    let read = |attr: &str| {
        let key = format!(" {attr}=\"");
        tag.find(&key)
            .and_then(|p| tag[p + key.len()..].split('"').next())
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0)
    };
    Some(Margins { bottom: read("bottom"), left: read("left"), right: read("right") })
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

/// Ajusta as margens da dock no labwc e pede para ele reler a configuração.
/// Não faz nada fora do labwc (`$LABWC_PID` ausente) ou se o usuário tem um
/// `rc.xml` próprio.
pub fn set_dock_margins(margins: Margins) {
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

    let wanted = if margins_of(&system_xml) == Some(margins) {
        // Igual ao sistema: sem cópia (atualizações do rc.xml do sistema valem).
        None
    } else {
        match replace_margins(&system_xml, margins) {
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
    fn troca_as_margens_e_acrescenta_as_que_faltam() {
        let m = Margins { bottom: 0, left: 70, right: 0 };
        let xml = replace_margins(RC, m).unwrap();
        assert!(xml.contains("<margin right=\"0\" left=\"70\" top=\"35\" bottom=\"0\" />"));
        assert_eq!(margins_of(&xml), Some(m));
        assert_eq!(margins_of(RC), Some(Margins { bottom: 88, left: 0, right: 0 }));
        assert!(replace_margins("<labwc_config/>", m).is_none());
    }

    #[test]
    fn marca_depois_da_declaracao_xml() {
        let xml = with_mark(RC);
        assert!(xml.starts_with("<?xml version=\"1.0\"?>\n<!-- Gerado pelo kalango-shell"));
        assert!(xml.contains("<labwc_config>"));
        assert!(with_mark("<labwc_config/>").starts_with(MARK));
    }
}
