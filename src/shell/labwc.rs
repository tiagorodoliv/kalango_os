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

/// Troca os atributos de uma ação na regra de janela `identifier`
/// (`<action name="MoveTo" x="0" y="0" />` → `attrs`). `None` se não houver.
pub fn replace_action(xml: &str, identifier: &str, action: &str, attrs: &str) -> Option<String> {
    let rule = xml.find(&format!("<windowRule identifier=\"{identifier}\""))?;
    let end = rule + xml[rule..].find("</windowRule>")?;
    let tag = format!("<action name=\"{action}\"");
    let at = rule + xml[rule..end].find(&tag)?;
    let close = at + xml[at..].find("/>")? + 2;
    Some(format!("{}{tag} {attrs} />{}", &xml[..at], &xml[close..]))
}

/// Troca a posição e o tamanho (`MoveTo` e `ResizeTo`) da regra da janela
/// da dock (`kalango-dock`). `None` se o `rc.xml` não tem a regra.
pub fn replace_dock_geometry(xml: &str, g: DockGeometry) -> Option<String> {
    let xml = replace_action(xml, "kalango-dock", "ResizeTo", &format!("width=\"{}\" height=\"{}\"", g.width, g.height))?;
    replace_action(&xml, "kalango-dock", "MoveTo", &format!("x=\"{}\" y=\"{}\"", g.x, g.y))
}

/// Põe as margens só no monitor principal (`<margin output="eDP-1" ...>`):
/// os outros monitores não têm barra nem dock.
pub fn set_margin_output(xml: &str, output: &str) -> Option<String> {
    let (start, end) = margin_tag(xml)?;
    let tag = &xml[start..end];
    let new_tag = match tag.find(" output=\"") {
        Some(p) => {
            let from = p + " output=\"".len();
            let to = from + tag[from..].find('"')?;
            format!("{}{output}{}", &tag[..from], &tag[to..])
        }
        None => format!("<margin output=\"{output}\"{}", &tag["<margin".len()..]),
    };
    Some(format!("{}{new_tag}{}", &xml[..start], &xml[end..]))
}

/// Regras das janelas de fundo dos monitores extras (`kalango-fundo-<saída>`):
/// papel de parede em tela cheia naquele monitor, atrás de tudo.
pub fn background_rules(outputs: &[(String, i32, i32)]) -> String {
    let mut text = String::new();
    for (name, x, y) in outputs {
        text += &format!(
            "    <windowRule identifier=\"kalango-fundo-{name}\" serverDecoration=\"no\" skipTaskbar=\"yes\" skipWindowSwitcher=\"yes\">\n"
        );
        // MoveToOutput (e não MoveTo): o ToggleFullscreen seguinte usa o
        // monitor em foco, e só assim a janela vai para o monitor certo.
        let _ = (x, y);
        text += &format!("      <action name=\"MoveToOutput\" output=\"{name}\" />\n");
        text += "      <action name=\"ToggleFullscreen\"/>\n";
        text += "      <action name=\"ToggleAlwaysOnBottom\"/>\n";
        text += "      <action name=\"ToggleOmnipresent\"/>\n";
        text += "    </windowRule>\n";
    }
    text
}

/// Posição e tamanho da janela da dock, em px lógicos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// O que o shell quer no `rc.xml` do usuário.
#[derive(Default, Clone)]
struct Wanted {
    margins: Option<Margins>,
    dock: Option<DockGeometry>,
    /// Monitor principal (nome e origem): margens e área de trabalho nele.
    primary: Option<(String, i32, i32)>,
    /// Monitores extras (nome e origem) com janela de fundo.
    backgrounds: Vec<(String, i32, i32)>,
}

static WANTED: std::sync::Mutex<Option<Wanted>> = std::sync::Mutex::new(None);

fn update(f: impl FnOnce(&mut Wanted)) -> bool {
    if let Ok(mut w) = WANTED.lock() {
        f(w.get_or_insert_with(Wanted::default));
    }
    sync()
}

/// Ajusta as margens da dock no labwc e pede para ele reler a configuração.
/// Não faz nada fora do labwc (`$LABWC_PID` ausente) ou se o usuário tem um
/// `rc.xml` próprio.
pub fn set_dock_margins(margins: Margins) {
    update(|w| w.margins = Some(margins));
}

/// Posição e tamanho da janela da dock (`src/shell/dock.rs`); `None` = a do
/// sistema. Devolve se o `rc.xml` ficou como pedido (a dock pode ser aberta).
pub fn set_dock_geometry(position: Option<DockGeometry>) -> bool {
    update(|w| w.dock = position)
}

/// Monitores (`src/shell/outputs.rs`): o principal e os extras com fundo. Devolve
/// se o `rc.xml` ficou como pedido.
pub fn set_outputs(primary: Option<(String, i32, i32)>, backgrounds: Vec<(String, i32, i32)>) -> bool {
    update(|w| {
        w.primary = primary;
        w.backgrounds = backgrounds;
    })
}

/// Monta o `rc.xml` do usuário a partir do do sistema (`None` = algo faltou).
fn build(system_xml: &str, w: &Wanted) -> Option<String> {
    let mut xml = system_xml.to_string();
    if let Some(m) = w.margins {
        if margins_of(&xml) != Some(m) {
            xml = replace_margins(&xml, m)?;
        }
    }
    if let Some((name, x, y)) = &w.primary {
        // Só com mais de um monitor: com um, a margem vale para ele de qualquer jeito.
        if !w.backgrounds.is_empty() {
            xml = set_margin_output(&xml, name)?;
        }
        let _ = (x, y);
        xml = replace_action(&xml, "kalango-shell", "MoveToOutput", &format!("output=\"{name}\""))?;
        // O menu da dock, o menu de apps e o painel rápido também (ficam junto
        // da barra e da dock). Sem isso abriam no monitor em foco.
        for rule in ["kalango-dock-menu", "kalango-launcher", "kalango-quick-settings"] {
            if let Some(x) = replace_action(&xml, rule, "MoveToOutput", &format!("output=\"{name}\"")) {
                xml = x;
            }
        }
    }
    if let Some(g) = w.dock {
        xml = replace_dock_geometry(&xml, g)?;
    }
    if !w.backgrounds.is_empty() {
        let at = xml.rfind("</windowRules>")?;
        xml.insert_str(at, &background_rules(&w.backgrounds));
    }
    Some(xml)
}

/// Gera (ou apaga) a cópia do `rc.xml` do usuário com o que o shell quer e
/// pede ao labwc para reler. Devolve se a cópia ficou como pedida.
fn sync() -> bool {
    if std::env::var_os("LABWC_PID").is_none() {
        return false;
    }
    let (Some(system), Some(user)) = (system_rc(), user_rc()) else {
        return false;
    };
    let Ok(system_xml) = fs::read_to_string(&system) else {
        return false;
    };
    let current = fs::read_to_string(&user).ok();
    if current.as_deref().is_some_and(|text| !text.contains(MARK)) {
        eprintln!("kalango-shell: {} foi feito à mão; a margem da dock não muda", user.display());
        return false;
    }
    let wanted = WANTED.lock().ok().and_then(|w| w.clone()).unwrap_or_default();
    let Some(xml) = build(&system_xml, &wanted) else {
        eprintln!("kalango-shell: o rc.xml do sistema não tem o que o shell precisa trocar");
        return false;
    };
    // Igual ao sistema: sem cópia (atualizações do rc.xml do sistema valem).
    let wanted = (xml != system_xml).then(|| with_mark(&xml));
    if wanted == current {
        return true;
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
        return false;
    }
    let _ = Command::new("labwc")
        .arg("--reconfigure")
        .stdin(Stdio::null())
        .status();
    true
}

/// Marca do `menu.xml` gerado pelo shell.
pub const MENU_MARK: &str = "<!-- Gerado pelo kalango-shell (menu de layouts das janelas). -->";

/// `menu.xml` com o menu de layouts (`kalango-layouts`): botão direito no
/// maximizar ou Super+Z. As regiões estão no `rc.xml` do sistema.
pub fn layouts_menu() -> String {
    use crate::common::i18n::tr;
    let item = |label: &str, action: &str| format!("    <item label=\"{label}\">{action}</item>\n");
    let region = |r: &str| format!("<action name=\"SnapToRegion\" region=\"{r}\" />");
    let mut xml = format!(
        "<?xml version=\"1.0\"?>\n{MENU_MARK}\n<openbox_menu>\n  <menu id=\"kalango-layouts\" label=\"{}\">\n",
        tr("Layouts", "Layouts")
    );
    xml += &item(tr("Left half", "Metade esquerda"), &region("esquerda"));
    xml += &item(tr("Right half", "Metade direita"), &region("direita"));
    xml += "    <separator />\n";
    xml += &item(tr("Top left", "Canto superior esquerdo"), &region("sup-esquerda"));
    xml += &item(tr("Top right", "Canto superior direito"), &region("sup-direita"));
    xml += &item(tr("Bottom left", "Canto inferior esquerdo"), &region("inf-esquerda"));
    xml += &item(tr("Bottom right", "Canto inferior direito"), &region("inf-direita"));
    xml += "    <separator />\n";
    xml += &item(tr("Maximize", "Maximizar"), "<action name=\"Maximize\" />");
    xml += &item(tr("Restore", "Restaurar"), "<action name=\"UnSnap\" /><action name=\"UnMaximize\" />");
    xml += "  </menu>\n</openbox_menu>\n";
    xml
}

/// Grava `~/.config/labwc/menu.xml` no idioma da sessão (se mudou) e pede ao
/// labwc para reler. Um `menu.xml` feito à mão (sem a marca) fica intocado.
pub fn sync_layouts_menu() {
    if std::env::var_os("LABWC_PID").is_none() {
        return;
    }
    let Some(dir) = user_rc().and_then(|p| p.parent().map(PathBuf::from)) else {
        return;
    };
    let path = dir.join("menu.xml");
    let current = fs::read_to_string(&path).ok();
    if current.as_deref().is_some_and(|t| !t.contains(MENU_MARK)) {
        return;
    }
    let wanted = layouts_menu();
    if current.as_deref() == Some(wanted.as_str()) {
        return;
    }
    if fs::create_dir_all(&dir).and_then(|_| fs::write(&path, wanted)).is_ok() {
        let _ = Command::new("labwc").arg("--reconfigure").stdin(Stdio::null()).status();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posicao_da_janela_da_dock() {
        let xml = "<a/>\n<windowRule identifier=\"kalango-shell\"><action name=\"MoveTo\" x=\"1\" y=\"1\" /></windowRule>\n<windowRule identifier=\"kalango-dock\" fixedPosition=\"yes\">\n  <action name=\"ToggleAlwaysOnTop\"/>\n  <action name=\"ResizeTo\" width=\"1\" height=\"1\" />\n  <action name=\"MoveTo\" x=\"0\" y=\"0\" />\n</windowRule>";
        let g = DockGeometry { x: 450, y: 714, width: 466, height: 54 };
        let out = replace_dock_geometry(xml, g).unwrap();
        assert!(out.contains("<action name=\"ResizeTo\" width=\"466\" height=\"54\" />"));
        assert!(out.contains("<action name=\"MoveTo\" x=\"450\" y=\"714\" />\n</windowRule>"));
        // A outra regra fica igual.
        assert!(out.contains("kalango-shell\"><action name=\"MoveTo\" x=\"1\" y=\"1\" />"));
        assert!(replace_dock_geometry("<windowRule identifier=\"x\"></windowRule>", g).is_none());
    }

    #[test]
    fn monitores_no_rc_xml() {
        let xml = "<margin top=\"35\" bottom=\"70\" />\n<windowRules>\n<windowRule identifier=\"kalango-shell\">\n<action name=\"MoveToOutput\" output=\"\" />\n<action name=\"ToggleFullscreen\"/>\n</windowRule>\n<windowRule identifier=\"kalango-dock\"><action name=\"ResizeTo\" width=\"1\" height=\"1\" /><action name=\"MoveTo\" x=\"0\" y=\"0\" /></windowRule>\n</windowRules>";
        let w = Wanted {
            primary: Some(("HDMI-A-1".into(), 1366, 0)),
            backgrounds: vec![("eDP-1".into(), 0, 0)],
            ..Default::default()
        };
        let out = build(xml, &w).unwrap();
        assert!(out.contains("<margin output=\"HDMI-A-1\" top=\"35\""));
        assert!(out.contains("kalango-shell\">\n<action name=\"MoveToOutput\" output=\"HDMI-A-1\" />"));
        assert!(out.contains("<action name=\"MoveToOutput\" output=\"eDP-1\" />"));
        assert!(out.contains("<windowRule identifier=\"kalango-fundo-eDP-1\""));
        assert!(out.trim_end().ends_with("</windowRule>\n</windowRules>"));
        // Um monitor só: sem output na margem.
        let one = build(xml, &Wanted { primary: Some(("eDP-1".into(), 0, 0)), ..Default::default() }).unwrap();
        assert!(!one.contains("<margin output="));
    }

    #[test]
    fn menu_de_layouts_valido() {
        let xml = layouts_menu();
        assert!(xml.contains(MENU_MARK));
        assert!(xml.contains("<menu id=\"kalango-layouts\""));
        assert_eq!(xml.matches("SnapToRegion").count(), 6);
        assert!(xml.trim_end().ends_with("</openbox_menu>"));
    }

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
