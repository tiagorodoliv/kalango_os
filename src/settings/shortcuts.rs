//! Atalhos de teclado (Configurações → Idioma e região → Atalhos de teclado).
//!
//! Os atalhos padrão do KalangOS (terminal, arquivos, navegador, bloquear...)
//! moram no `rc.xml` do labwc do sistema. A pessoa pode trocar a tecla de
//! cada um e criar atalhos próprios (nome, comando e tecla). A escolha fica em
//! `~/.config/kalango/atalhos.conf`:
//!
//! ```text
//! troca=terminal C-A-t           (id do atalho padrão e a tecla nova)
//! proprio=W-g<TAB>GIMP<TAB>gimp   (tecla, nome e comando)
//! ```
//!
//! As Configurações só gravam o arquivo; a área de trabalho percebe a mudança
//! (a cada 1 s) e o aplica no `~/.config/labwc/rc.xml` que ela gera
//! ([`apply`], `crate::shell::labwc`), e o labwc relê na hora.
//!
//! Teclas no formato do labwc: modificadores `W` (Super), `C` (Ctrl), `A`
//! (Alt) e `S` (Shift) separados por `-`, e o nome da tecla do XKB
//! (`W-Return`, `C-A-t`, `Print`).

use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::common::i18n::tr;
use crate::{AppWindow, Device, Mode, ShortcutItem};

/// Atalhos padrão que dá para trocar: id, nome (inglês, português) e a tecla
/// do `rc.xml` do sistema.
pub const DEFAULTS: [(&str, &str, &str, &str); 9] = [
    ("menu", "Open the app menu", "Abrir o menu de apps", "W-space"),
    ("terminal", "Terminal", "Terminal", "W-Return"),
    ("files", "Files", "Arquivos", "W-e"),
    ("browser", "Browser", "Navegador", "W-b"),
    ("lock", "Lock the screen", "Bloquear a tela", "W-l"),
    ("layouts", "Window layouts", "Layouts da janela", "W-z"),
    ("capture", "Screenshot tool", "Ferramenta de captura", "Print"),
    ("screenshot", "Screenshot of the whole screen", "Foto da tela inteira", "S-Print"),
    ("record", "Record the screen", "Gravar a tela", "C-S-A-r"),
];

/// Um atalho próprio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Custom {
    pub key: String,
    pub name: String,
    pub command: String,
}

/// Conteúdo do `atalhos.conf`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conf {
    /// (id do atalho padrão, tecla nova).
    pub changed: Vec<(String, String)>,
    pub custom: Vec<Custom>,
}

impl Conf {
    pub fn parse(text: &str) -> Conf {
        let mut c = Conf::default();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("troca=") {
                if let Some((id, key)) = rest.trim().split_once(' ') {
                    if DEFAULTS.iter().any(|d| d.0 == id) && valid_combo(key.trim()) {
                        c.changed.retain(|(i, _)| i != id);
                        c.changed.push((id.to_string(), key.trim().to_string()));
                    }
                }
            } else if let Some(rest) = line.strip_prefix("proprio=") {
                let mut parts = rest.splitn(3, '\t');
                let (Some(key), Some(name), Some(command)) = (parts.next(), parts.next(), parts.next()) else { continue };
                if valid_combo(key) && !command.trim().is_empty() {
                    c.custom.push(Custom { key: key.into(), name: name.into(), command: command.trim().into() });
                }
            }
        }
        c
    }

    pub fn to_text(&self) -> String {
        let mut out = String::from("# Atalhos de teclado (Configurações → Idioma e região)\n");
        for (id, key) in &self.changed {
            out.push_str(&format!("troca={id} {key}\n"));
        }
        for c in &self.custom {
            out.push_str(&format!("proprio={}\t{}\t{}\n", c.key, clean(&c.name), clean(&c.command)));
        }
        out
    }

    /// Tecla atual de um atalho padrão.
    pub fn key_of(&self, id: &str) -> Option<String> {
        if let Some((_, k)) = self.changed.iter().find(|(i, _)| i == id) {
            return Some(k.clone());
        }
        DEFAULTS.iter().find(|d| d.0 == id).map(|d| d.3.to_string())
    }
}

/// Tira quebras de linha e tabulações (separadores do arquivo).
fn clean(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string()
}

/// Tecla no formato do labwc, só com caracteres seguros (vai para o XML).
fn valid_combo(combo: &str) -> bool {
    let mut parts: Vec<&str> = combo.split('-').collect();
    let Some(key) = parts.pop() else { return false };
    !key.is_empty()
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && parts.iter().all(|m| matches!(*m, "W" | "C" | "A" | "S"))
}

/// Forma comparável de uma tecla: modificadores em ordem fixa e o nome da
/// tecla sem diferença de maiúsculas ("C-S-A-r" = "C-A-S-R").
pub fn canon(combo: &str) -> String {
    let mut parts: Vec<&str> = combo.split('-').collect();
    let key = parts.pop().unwrap_or_default().to_ascii_lowercase();
    let mods: String = ["W", "C", "A", "S"].iter().filter(|m| parts.contains(m)).map(|m| format!("{m}-")).collect();
    format!("{mods}{key}")
}

/// Monta a tecla a partir da tela: `mods` (1 Super, 2 Ctrl, 4 Alt, 8 Shift) e
/// o nome digitado ("t", "Enter", "f5", "espaço", "Print").
pub fn combo(mods: i32, key: &str) -> Result<String, &'static str> {
    let k = key.trim();
    let name = match k.to_lowercase().as_str() {
        "" => return Err("empty"),
        "enter" | "return" => "Return".to_string(),
        "space" | "espaço" | "espaco" => "space".to_string(),
        "tab" => "Tab".to_string(),
        "esc" | "escape" => "Escape".to_string(),
        "print" | "printscreen" | "prtsc" => "Print".to_string(),
        "del" | "delete" => "Delete".to_string(),
        "backspace" => "BackSpace".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "up" | "cima" => "Up".to_string(),
        "down" | "baixo" => "Down".to_string(),
        "left" | "esquerda" => "Left".to_string(),
        "right" | "direita" => "Right".to_string(),
        l if l.len() >= 2 && l.starts_with('f') && l[1..].parse::<u8>().is_ok_and(|n| (1..=24).contains(&n)) => {
            l.to_uppercase()
        }
        l if l.chars().count() == 1 && l.chars().all(|c| c.is_ascii_alphanumeric()) => l.to_string(),
        _ if k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => k.to_string(),
        _ => return Err("invalid"),
    };
    // Sem modificador, só teclas que não servem para digitar (F5, Print...).
    let plain_ok = name.len() > 1 && name != "space" && name != "Tab" && name != "BackSpace";
    if mods & 15 == 0 && !plain_ok {
        return Err("needs-modifier");
    }
    let mut out = String::new();
    for (bit, m) in [(1, "W"), (2, "C"), (4, "A"), (8, "S")] {
        if mods & bit != 0 {
            out.push_str(m);
            out.push('-');
        }
    }
    out.push_str(&name);
    Ok(out)
}

/// Para mostrar: "W-S-Return" → "Super + Shift + Enter".
pub fn pretty(combo: &str) -> String {
    let mut parts: Vec<&str> = combo.split('-').collect();
    let key = parts.pop().unwrap_or_default();
    let mut out: Vec<String> = ["W", "C", "A", "S"]
        .iter()
        .filter(|m| parts.contains(m))
        .map(|m| match *m {
            "W" => "Super".to_string(),
            "C" => "Ctrl".to_string(),
            "A" => "Alt".to_string(),
            _ => "Shift".to_string(),
        })
        .collect();
    out.push(match key {
        "Return" => "Enter".to_string(),
        "space" => tr("Space", "Espaço").to_string(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => k.to_string(),
    });
    out.join(" + ")
}

/// Escapa texto para atributo de XML.
fn xml_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Aplica a escolha no `rc.xml`: troca a tecla dos atalhos padrão e põe os
/// próprios antes de `</keyboard>`.
pub fn apply(xml: &str, conf: &Conf) -> String {
    let mut xml = xml.to_string();
    for (id, key) in &conf.changed {
        let Some(default) = DEFAULTS.iter().find(|d| d.0 == id).map(|d| d.3) else { continue };
        let old = format!("<keybind key=\"{default}\"");
        if let Some(at) = xml.find(&old) {
            xml.replace_range(at..at + old.len(), &format!("<keybind key=\"{}\"", xml_attr(key)));
        }
    }
    if !conf.custom.is_empty() {
        if let Some(at) = xml.rfind("</keyboard>") {
            let mut block = String::from("    <!-- Atalhos próprios (Configurações → Idioma e região). -->\n");
            for c in &conf.custom {
                block.push_str(&format!(
                    "    <keybind key=\"{}\"><action name=\"Execute\" command=\"{}\" /></keybind>\n",
                    xml_attr(&c.key),
                    xml_attr(&c.command)
                ));
            }
            let head = xml[..at].trim_end_matches(' ').to_string();
            xml = format!("{head}{block}  {}", &xml[at..]);
        }
    }
    xml
}

fn conf_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("kalango/atalhos.conf"))
}

pub fn load() -> Conf {
    conf_path().and_then(|p| fs::read_to_string(p).ok()).map(|t| Conf::parse(&t)).unwrap_or_default()
}

fn save(conf: &Conf) -> bool {
    let Some(path) = conf_path() else { return false };
    path.parent().map_or(Ok(()), fs::create_dir_all).and_then(|_| fs::write(&path, conf.to_text())).is_ok()
}

/// Todas as teclas em uso (do `rc.xml` do sistema, com as trocas e os
/// próprios), na forma de [`canon`]; menos a do atalho `except`.
fn used_keys(conf: &Conf, except: Option<&str>) -> Vec<String> {
    let system = fs::read_to_string("/etc/xdg/labwc/rc.xml").unwrap_or_default();
    let mut keys: Vec<String> = system
        .split("<keybind key=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next())
        .map(canon)
        .collect();
    for (id, _, _, default) in DEFAULTS {
        keys.retain(|k| *k != canon(default));
        if Some(id) != except {
            if let Some(k) = conf.key_of(id) {
                keys.push(canon(&k));
            }
        }
    }
    keys.extend(conf.custom.iter().filter(|c| Some(c.key.as_str()) != except).map(|c| canon(&c.key)));
    keys
}

/// Mensagem do erro de [`combo`].
fn combo_error(e: &str) -> &'static str {
    match e {
        "empty" => tr("Type the key (a letter, Enter, F5, Print...).", "Digite a tecla (uma letra, Enter, F5, Print...)."),
        "needs-modifier" => tr(
            "Choose Super, Ctrl or Alt too (or use a key like F5 or Print).",
            "Escolha também Super, Ctrl ou Alt (ou use uma tecla como F5 ou Print).",
        ),
        _ => tr("Unknown key name.", "Nome de tecla desconhecido."),
    }
}

/// Mostra a lista nas Configurações.
fn show(d: &Device, conf: &Conf) {
    let mut items: Vec<ShortcutItem> = DEFAULTS
        .iter()
        .map(|(id, en, pt, _)| {
            let key = conf.key_of(id).unwrap_or_default();
            ShortcutItem {
                id: (*id).into(),
                name: tr(en, pt).into(),
                keys: pretty(&key).into(),
                command: SharedString::new(),
                custom: false,
                changed: conf.changed.iter().any(|(i, _)| i == id),
            }
        })
        .collect();
    items.extend(conf.custom.iter().map(|c| ShortcutItem {
        id: c.key.clone().into(),
        name: c.name.clone().into(),
        keys: pretty(&c.key).into(),
        command: c.command.clone().into(),
        custom: true,
        changed: false,
    }));
    d.set_shortcuts(ModelRc::from(Rc::new(VecModel::from(items))));
}

/// Liga a seção às Configurações e, na área de trabalho, aplica o arquivo no
/// labwc quando ele muda (o timer devolvido precisa ficar vivo).
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if !crate::shell::session::is_native() {
        return None;
    }
    match mode {
        Mode::Desktop => {
            let mut seen: Option<SystemTime> = None;
            let mut first = true;
            let timer = Timer::default();
            timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
                let now = conf_path().and_then(|p| fs::metadata(p).ok()).and_then(|m| m.modified().ok());
                if first || now != seen {
                    first = false;
                    seen = now;
                    crate::shell::labwc::set_shortcuts(load());
                }
            });
            Some(timer)
        }
        Mode::Settings => {
            let d = ui.global::<Device>();
            show(&d, &load());
            let weak = ui.as_weak();
            let finish = move |conf: Conf, msg: &str| {
                if let Some(ui) = weak.upgrade() {
                    let d = ui.global::<Device>();
                    if msg.is_empty() && !save(&conf) {
                        d.set_shortcut_message(tr("Could not save the shortcuts.", "Não deu para gravar os atalhos.").into());
                        return;
                    }
                    d.set_shortcut_message(msg.into());
                    if msg.is_empty() {
                        d.set_shortcut_saved(d.get_shortcut_saved() + 1);
                    }
                    show(&d, &load());
                }
            };
            let f = finish.clone();
            d.on_set_shortcut(move |id, mods, key| {
                let mut conf = load();
                match combo(mods, &key) {
                    Err(e) => f(conf, combo_error(e)),
                    Ok(k) if used_keys(&conf, Some(&id)).contains(&canon(&k)) => {
                        f(conf, tr("This key is already used by another shortcut.", "Essa tecla já é usada por outro atalho."))
                    }
                    Ok(k) => {
                        conf.changed.retain(|(i, _)| *i != id.as_str());
                        let default = DEFAULTS.iter().find(|d| d.0 == id.as_str()).map(|d| d.3);
                        if default != Some(k.as_str()) {
                            conf.changed.push((id.to_string(), k));
                        }
                        f(conf, "");
                    }
                }
            });
            let f = finish.clone();
            d.on_reset_shortcut(move |id| {
                let mut conf = load();
                conf.changed.retain(|(i, _)| *i != id.as_str());
                f(conf, "");
            });
            let f = finish.clone();
            d.on_add_shortcut(move |name, command, mods, key| {
                let mut conf = load();
                if command.trim().is_empty() {
                    return f(conf, tr("Type the command to run (e.g. gimp).", "Digite o comando a rodar (ex.: gimp)."));
                }
                match combo(mods, &key) {
                    Err(e) => f(conf, combo_error(e)),
                    Ok(k) if used_keys(&conf, None).contains(&canon(&k)) => {
                        f(conf, tr("This key is already used by another shortcut.", "Essa tecla já é usada por outro atalho."))
                    }
                    Ok(k) => {
                        let name = if name.trim().is_empty() { command.trim().to_string() } else { name.trim().to_string() };
                        conf.custom.push(Custom { key: k, name, command: command.trim().into() });
                        f(conf, "");
                    }
                }
            });
            let f = finish;
            d.on_remove_shortcut(move |id| {
                let mut conf = load();
                conf.custom.retain(|c| c.key != id.as_str());
                f(conf, "");
            });
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monta_e_mostra_teclas() {
        assert_eq!(combo(1, "Enter").unwrap(), "W-Return");
        assert_eq!(combo(2 | 4, "T").unwrap(), "C-A-t");
        assert_eq!(combo(0, "f5").unwrap(), "F5");
        assert_eq!(combo(0, "Print").unwrap(), "Print");
        assert!(combo(0, "a").is_err());
        assert!(combo(1, "a b").is_err());
        assert!(combo(1, "\"><x").is_err());
        assert_eq!(pretty("W-S-Return"), "Super + Shift + Enter");
        assert_eq!(pretty("C-A-t"), "Ctrl + Alt + T");
        assert_eq!(canon("C-S-A-r"), canon("C-A-S-R"));
    }

    #[test]
    fn arquivo_e_rc_xml() {
        let conf = Conf {
            changed: vec![("terminal".into(), "C-A-t".into())],
            custom: vec![Custom { key: "W-g".into(), name: "GIMP".into(), command: "gimp --new \"a&b\"".into() }],
        };
        assert_eq!(Conf::parse(&conf.to_text()), conf);
        // Lixo e ids desconhecidos ficam de fora.
        assert_eq!(Conf::parse("troca=xyz W-q\ntroca=terminal W-\"x\nproprio=W-k\tsem comando\t\n"), Conf::default());
        let xml = "<keyboard>\n    <keybind key=\"W-Return\">\n      <action name=\"Execute\" command=\"foot\" />\n    </keybind>\n  </keyboard>\n";
        let out = apply(xml, &conf);
        assert!(out.contains("<keybind key=\"C-A-t\">") && !out.contains("W-Return"));
        assert!(out.contains("command=\"gimp --new &quot;a&amp;b&quot;\""));
        assert!(out.trim_end().ends_with("</keyboard>"));
        assert_eq!(apply(xml, &Conf::default()), xml);
    }
}
