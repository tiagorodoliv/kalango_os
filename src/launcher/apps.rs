//! Busca do launcher e a lista "Todos os apps".
//!
//! A busca ignora maiúsculas e acentos ("configuracoes" acha "Configurações",
//! "MUSICA" acha "Música"), o que o Slint não faz sozinho — por isso fica no Rust.
//! Ela procura primeiro nos fixados e depois nos apps instalados no sistema
//! (`Apps.all`, lidos dos `.desktop` por [`crate::launcher::desktop`]), sem repetir os
//! que já são fixados.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

thread_local! {
    /// Ids de `Apps.all` que repetem um app fixado (ex.: o "Thunar", que já é
    /// "Arquivos"): ficam fora dos resultados da busca. Só a thread da UI usa.
    static DUPLICATES: RefCell<HashSet<SharedString>> = RefCell::default();
    /// Itens já montados de apps do sistema (`desktop:<caminho>`) e de
    /// janelas sem app conhecido (`window:<app_id>`), com o ícone carregado.
    static ENTRIES: RefCell<HashMap<String, AppEntry>> = RefCell::default();
}

use slint::{Color, ComponentHandle, Image, Model, ModelRc, SharedString, VecModel};

use crate::launcher::desktop::{self, DesktopApp};
use crate::{AppEntry, AppWindow, Apps, IconPreview, Shell};

/// Normaliza para comparação: minúsculas e sem acentos do português.
pub fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// Diz se um app combina com a busca: o nome ou o texto extra (id, nome
/// genérico, palavras-chave) contêm o que foi digitado.
pub fn matches(name: &str, extra: &str, query: &str) -> bool {
    let q = normalize(query.trim());
    q.is_empty() || normalize(name).contains(&q) || normalize(extra).contains(&q)
}

/// Ids da lista da dock (`Theme.dock-apps`, separados por vírgula), sem
/// vazios nem repetidos, na ordem da lista.
pub fn dock_ids(list: &str) -> Vec<&str> {
    let mut ids: Vec<&str> = Vec::new();
    for id in list.split(',').map(str::trim) {
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// Liga ou desliga um app na lista da dock: acrescenta no fim ou tira.
/// O último app não sai (a dock nunca fica vazia).
pub fn dock_toggle(list: &str, id: &str) -> String {
    let mut ids = dock_ids(list);
    if let Some(pos) = ids.iter().position(|i| *i == id) {
        if ids.len() > 1 {
            ids.remove(pos);
        }
    } else {
        ids.push(id);
    }
    ids.join(",")
}

/// Programas dos apps fixados: os `.desktop` desses programas não se repetem
/// nos resultados da busca (ex.: "Thunar" já aparece como "Arquivos").
fn pinned_programs(ids: impl Iterator<Item = String>) -> HashSet<String> {
    ids.filter_map(|id| crate::shell::session::command_for(&id))
        .flat_map(|argv| {
            let first = argv.first().cloned();
            let last = argv.last().cloned();
            first.into_iter().chain(last)
        })
        .collect()
}

/// Nomes de ícone a procurar no tema para cada app do catálogo (o primeiro
/// que o tema tiver vale). Na sessão real, a dock e o menu usam esses ícones
/// coloridos no lugar dos ícones de traço.
pub fn themed_icon_names(id: &str) -> &'static [&'static str] {
    match id {
        "files" => &["org.xfce.thunar", "thunar", "system-file-manager"],
        "terminal" => &["foot", "utilities-terminal"],
        "browser" => &["firefox-esr", "firefox"],
        "code" => &["geany", "accessories-text-editor"],
        "store" => &["org.gnome.Software", "gnome-software", "system-software-install"],
        "settings" => &["preferences-system", "org.gnome.Settings", "preferences-desktop"],
        "images" => &["org.xfce.ristretto", "ristretto", "image-viewer"],
        "music" => &["org.xfce.parole", "parole", "multimedia-audio-player"],
        "documents" => &["folder-documents"],
        "calendar" => &["org.gnome.Calendar", "office-calendar"],
        "monitor" => &["org.gnome.SystemMonitor", "utilities-system-monitor", "htop"],
        "updates" => &["system-software-update", "software-update-available", "update-manager"],
        _ => &[],
    }
}

/// O tema de ícones mudou: ícones novos nos apps do catálogo, nos apps do
/// sistema fixados e nos abertos (a dock e o menu desta janela).
pub fn reload_icons(ui: &AppWindow, theme: &str) {
    desktop::set_current_icon_theme(theme);
    ENTRIES.with_borrow_mut(|m| m.clear());
    use_themed_icons(ui, theme);
    #[cfg(target_os = "linux")]
    crate::shell::taskbar::reload_icons(ui);
}

/// Troca os ícones dos apps fixados pelos do tema de ícones `theme` (com
/// `tint` transparente, que a dock e o menu desenham sem colorir).
pub fn use_themed_icons(ui: &AppWindow, theme: &str) {
    let dirs = desktop::data_dirs();
    let apps = ui.global::<Apps>();
    let entries: Vec<AppEntry> = apps
        .get_pinned()
        .iter()
        .map(|mut app| {
            let image = themed_icon_names(&app.id)
                .iter()
                .find_map(|name| desktop::find_themed_icon(name, theme, &dirs))
                .and_then(|path| Image::load_from_path(&path).ok());
            if let Some(image) = image {
                app.icon = image;
                app.tint = Color::from_argb_u8(0, 0, 0, 0);
            }
            app
        })
        .collect();
    apps.set_pinned(ModelRc::from(Rc::new(VecModel::from(entries))));
}

/// Amostras de cada família de ícones para a escolha na Aparência (pasta,
/// Arquivos, Terminal, Navegador e Configurações), na variante azul.
pub fn icon_previews() -> Vec<IconPreview> {
    let dirs = desktop::data_dirs();
    (0..3)
        .map(|family| {
            let theme = crate::settings::appearance::icon_theme_name(family, 2);
            let icons: Vec<Image> = std::iter::once(&["folder"][..])
                .chain(["files", "terminal", "browser", "settings"].iter().map(|id| themed_icon_names(id)))
                .map(|names| {
                    names
                        .iter()
                        .find_map(|n| desktop::find_themed_icon(n, &theme, &dirs))
                        .and_then(|p| Image::load_from_path(&p).ok())
                        .unwrap_or_default()
                })
                .collect();
            IconPreview { icons: ModelRc::from(Rc::new(VecModel::from(icons))) }
        })
        .collect()
}

/// Cores das iniciais dos apps sem ícone (as cores de destaque do tema).
const LETTER_TINTS: [u32; 6] = [0x3dd6f5, 0xa78bfa, 0x7ea6f7, 0x5fd38d, 0xe8b04b, 0xf28b95];

/// Inicial maiúscula do nome (para o app sem ícone).
pub fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

/// Converte um app do sistema para o item do menu. Com ícone, `tint` fica
/// transparente (o ícone já é colorido); sem ícone, o menu mostra a inicial
/// numa cor fixa por app.
fn to_entry(app: &DesktopApp, icon: Option<&PathBuf>) -> AppEntry {
    let image = icon.and_then(|p| Image::load_from_path(p).ok());
    let tint = if image.is_some() {
        Color::from_argb_u8(0, 0, 0, 0)
    } else {
        let sum: u32 = app.name.bytes().map(u32::from).sum();
        Color::from_argb_encoded(0xff00_0000 | LETTER_TINTS[sum as usize % LETTER_TINTS.len()])
    };
    AppEntry {
        id: app.id().into(),
        name: app.name.clone().into(),
        icon: image.clone().unwrap_or_default(),
        tint,
        letter: if image.is_some() { SharedString::new() } else { initial(&app.name).into() },
        keywords: app.keywords.clone().into(),
    }
}

/// Ícone de um app do sistema: do tema de ícones atual ou do hicolor/pixmaps.
pub fn desktop_icon(app: &DesktopApp) -> Option<PathBuf> {
    let dirs = desktop::data_dirs();
    if app.icon.starts_with('/') {
        return desktop::find_icon(&app.icon, &dirs);
    }
    desktop::find_themed_icon(&app.icon, &desktop::current_icon_theme(), &dirs)
        .or_else(|| desktop::find_icon(&app.icon, &dirs))
}

/// Item de um app do sistema (`desktop:<caminho>`) ou de uma janela sem app
/// conhecido (`window:<app_id>`), guardado depois da primeira vez.
pub fn extra_entry(id: &str) -> Option<AppEntry> {
    if let Some(e) = ENTRIES.with_borrow(|m| m.get(id).cloned()) {
        return Some(e);
    }
    let entry = if let Some(app_id) = id.strip_prefix(crate::shell::taskbar::WINDOW_PREFIX) {
        // Sem `.desktop`: ícone com o nome do app_id, se o tema tiver, ou a inicial.
        let fake = DesktopApp {
            path: PathBuf::new(),
            name: app_id.rsplit('.').next().unwrap_or(app_id).to_string(),
            keywords: String::new(),
            icon: app_id.to_string(),
            argv: Vec::new(),
            terminal: false,
            wm_class: String::new(),
        };
        let mut e = to_entry(&fake, desktop_icon(&fake).as_ref());
        e.id = id.into();
        e
    } else {
        let app = desktop::from_id(id)?;
        to_entry(&app, desktop_icon(&app).as_ref())
    };
    ENTRIES.with_borrow_mut(|m| m.insert(id.to_string(), entry.clone()));
    Some(entry)
}

/// Item de qualquer id da dock: do catálogo, app do sistema ou janela.
pub fn entry_for(ui: &AppWindow, id: &str) -> Option<AppEntry> {
    ui.global::<Apps>().get_pinned().iter().find(|a| a.id == id).or_else(|| extra_entry(id))
}

/// Liga o callback `Shell.filter-apps`; na sessão real (`themed`) troca os
/// ícones dos fixados pelos do tema e, no menu (`load_installed`), preenche
/// `Apps.all` com os apps instalados.
pub fn install(ui: &AppWindow, load_installed: bool, themed: bool) {
    if themed {
        use_themed_icons(ui, &desktop::current_icon_theme());
        ui.global::<Shell>()
            .set_icon_previews(ModelRc::from(Rc::new(VecModel::from(icon_previews()))));
    }
    let shell = ui.global::<Shell>();
    shell.on_dock_has(|list, id| dock_ids(&list).contains(&id.as_str()));
    shell.on_dock_toggle(|list, id| dock_toggle(&list, &id).into());
    let weak = ui.as_weak();
    shell.on_dock_entries(move |list| {
        let Some(ui) = weak.upgrade() else {
            return ModelRc::default();
        };
        let pinned = ui.global::<Apps>().get_pinned();
        // Configurações fica fora: a dock a mostra sempre, depois do separador.
        let entries: Vec<AppEntry> = dock_ids(&list)
            .into_iter()
            .filter(|id| *id != "settings")
            // Do catálogo ou qualquer app do sistema fixado (`desktop:...`).
            .filter_map(|id| pinned.iter().find(|a| a.id == id).or_else(|| extra_entry(id)))
            .collect();
        ModelRc::from(Rc::new(VecModel::from(entries)))
    });

    let weak = ui.as_weak();
    ui.global::<Shell>().on_filter_apps(move |query| {
        let Some(ui) = weak.upgrade() else {
            return ModelRc::default();
        };
        let apps = ui.global::<Apps>();
        let pinned = apps.get_pinned();
        let pinned_ids: HashSet<SharedString> = pinned.iter().map(|a| a.id).collect();
        let found: Vec<AppEntry> = DUPLICATES.with_borrow(|dup| {
            pinned
                .iter()
                .filter(|app| matches(&app.name, &app.id, &query))
                .chain(apps.get_all().iter().filter(|app| {
                    !pinned_ids.contains(&app.id)
                        && !dup.contains(&app.id)
                        && matches(&app.name, &app.keywords, &query)
                }))
                .collect()
        });
        ModelRc::from(Rc::new(VecModel::from(found)))
    });

    if !load_installed {
        return;
    }
    // Ler e conferir dezenas de arquivos fica numa thread para o menu abrir
    // na hora; as imagens (que não são `Send`) são criadas no laço da UI.
    let programs = pinned_programs(ui.global::<Apps>().get_pinned().iter().map(|a| a.id.into()));
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let dirs = desktop::data_dirs();
        let found: Vec<(DesktopApp, Option<PathBuf>)> = desktop::scan(&dirs)
            .into_iter()
            .map(|app| {
                let icon = desktop::find_icon(&app.icon, &dirs);
                (app, icon)
            })
            .collect();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let entries: Vec<AppEntry> = DUPLICATES.with_borrow_mut(|dup| {
                found
                    .iter()
                    .map(|(app, icon)| {
                        if programs.contains(app.program()) {
                            dup.insert(app.id().into());
                        }
                        to_entry(app, icon.as_ref())
                    })
                    .collect()
            });
            ui.global::<Apps>()
                .set_all(ModelRc::from(Rc::new(VecModel::from(entries))));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignora_acentos_e_maiusculas() {
        assert!(matches("Configurações", "settings", "configuracoes"));
        assert!(matches("Música", "music", "MUSICA"));
        assert!(matches("Atualizações", "updates", "  atual "));
    }

    #[test]
    fn acha_pelo_id_e_rejeita_o_resto() {
        assert!(matches("Navegador", "browser", "brow"));
        assert!(!matches("Loja", "store", "terminal"));
        assert!(!matches("GIMP", " Editor de Imagens gimp", "edição"));
        assert!(matches("GIMP", " Editor de Imagens gimp", "editor"));
    }

    #[test]
    fn lista_da_dock() {
        assert_eq!(dock_ids(" files,terminal,,files ,store"), ["files", "terminal", "store"]);
        assert_eq!(dock_toggle("files,terminal", "music"), "files,terminal,music");
        assert_eq!(dock_toggle("files,terminal,music", "terminal"), "files,music");
        // O último app fica.
        assert_eq!(dock_toggle("files", "files"), "files");
    }

    #[test]
    fn programas_dos_fixados() {
        let p = pinned_programs(["files", "monitor", "browser"].iter().map(|s| s.to_string()));
        assert!(p.contains("thunar"));
        assert!(p.contains("htop"));
        assert!(p.contains("firefox-esr"));
        assert!(!p.contains("gimp"));
    }

    #[test]
    fn inicial_maiuscula() {
        assert_eq!(initial("épico"), "É");
        assert_eq!(initial(""), "");
    }
}
