//! Apps abertos na dock (como a do Ubuntu) e o menu do botão direito dela.
//!
//! As janelas vêm do [`crate::shell::toplevels`]. Cada uma é ligada a um id da dock:
//! um app do catálogo ("files", "browser"...), um app do sistema pelo
//! `.desktop` (`desktop:<caminho>`: o VS Code é o
//! `com.microsoft.VSCode.desktop`, reconhecido pelo `StartupWMClass`) ou, sem
//! `.desktop`, `window:<app_id>`. A dock mostra o ponto nos apps abertos, o
//! traço no app em foco e, depois dos fixados, os apps abertos que não estão
//! fixados.
//!
//! Clique num app: sem janela, abre; com janela fora de foco, põe a mais
//! recente em foco; em foco, minimiza (uma janela) ou passa para a próxima
//! (várias). Botão direito: `kalango-shell --menu-dock=<id>` (menu em tela
//! cheia e transparente, porque a janela da dock é do tamanho dela) com as
//! janelas do app, nova janela, fixar/desafixar e fechar.

use std::cell::RefCell;
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};

use crate::launcher::desktop::{self, DesktopApp};
use crate::shell::session::Mode;
use crate::shell::toplevels::{Tracker, Window};
use crate::{AppEntry, AppWindow, Apps, DockMenu, DockMenuWindow, Shell, Theme};

/// Prefixo dos ids de janelas sem `.desktop` conhecido (definido no
/// `launcher::apps`, que também existe fora do Linux).
pub use crate::launcher::apps::WINDOW_PREFIX;

/// `app_id` das janelas de cada app do catálogo (sem diferenciar maiúsculas).
const CATALOG: [(&str, &[&str]); 10] = [
    ("files", &["thunar", "org.xfce.thunar"]),
    ("terminal", &["foot", "footclient"]),
    ("browser", &["firefox-esr", "firefox", "org.mozilla.firefox"]),
    ("code", &["geany", "org.geany.geany", "org.xfce.mousepad", "mousepad"]),
    ("store", &["org.gnome.software", "gnome-software"]),
    ("images", &["org.xfce.ristretto", "ristretto"]),
    ("music", &["org.xfce.parole", "parole"]),
    ("calendar", &["org.gnome.calendar", "gnome-calendar"]),
    ("settings", &["kalango-settings"]),
    ("monitor", &["org.gnome.systemmonitor", "gnome-system-monitor"]),
];

/// Apps do sistema, lidos uma vez (e de novo quando aparece uma janela que
/// não se encaixa em nenhum, no máximo a cada 15 s: app recém-instalado).
static DESKTOP_APPS: Mutex<Option<(Vec<DesktopApp>, Instant)>> = Mutex::new(None);

/// `.desktop` de uma janela: nome do arquivo, `StartupWMClass`, programa ou
/// a última parte do nome do arquivo (org.gnome.Calendar → calendar).
pub fn match_desktop<'a>(app_id: &str, apps: &'a [DesktopApp]) -> Option<&'a DesktopApp> {
    let a = app_id.to_lowercase();
    let stem = |d: &DesktopApp| {
        d.path.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
    };
    apps.iter()
        .find(|d| stem(d) == a)
        .or_else(|| apps.iter().find(|d| !d.wm_class.is_empty() && d.wm_class.to_lowercase() == a))
        .or_else(|| apps.iter().find(|d| d.program().to_lowercase() == a))
        .or_else(|| apps.iter().find(|d| stem(d).rsplit('.').next() == Some(a.as_str())))
}

/// Id da dock de uma janela, com a lista de apps do sistema já lida.
/// `None` para as janelas do próprio shell (menos as Configurações).
pub fn dock_id_with(app_id: &str, apps: &[DesktopApp]) -> Option<String> {
    let a = app_id.to_lowercase();
    if a.is_empty() {
        return None;
    }
    if let Some((id, _)) = CATALOG.iter().find(|(_, ids)| ids.contains(&a.as_str())) {
        return Some(id.to_string());
    }
    // Janelas do próprio shell (barra, fundo, menus) ficam fora da dock;
    // as que têm .desktop (a Calculadora) entram como qualquer app.
    if a.starts_with("kalango-") {
        return match_desktop(&a, apps).map(|d| d.id());
    }
    Some(match match_desktop(&a, apps) {
        Some(d) => d.id(),
        None => format!("{WINDOW_PREFIX}{app_id}"),
    })
}

/// Id da dock de uma janela (lê os `.desktop` na primeira vez).
pub fn dock_id(app_id: &str) -> Option<String> {
    let mut guard = DESKTOP_APPS.lock().ok()?;
    if guard.is_none() {
        *guard = Some((desktop::scan(&desktop::data_dirs()), Instant::now()));
    }
    let (apps, when) = guard.as_mut()?;
    let id = dock_id_with(app_id, apps)?;
    if id.starts_with(WINDOW_PREFIX) && when.elapsed() > Duration::from_secs(15) {
        *apps = desktop::scan(&desktop::data_dirs());
        *when = Instant::now();
        return dock_id_with(app_id, apps);
    }
    Some(id)
}

/// Janelas que entram na dock e no Alt+Tab, com o id da dock de cada uma.
pub fn with_ids(windows: Vec<Window>) -> Vec<(Window, String)> {
    windows.into_iter().filter_map(|w| dock_id(&w.app_id).map(|id| (w, id))).collect()
}

/// Ids dos apps abertos, sem repetir, na ordem em que abriram.
pub fn running_ids(windows: &[(Window, String)]) -> Vec<String> {
    let mut by_age: Vec<&(Window, String)> = windows.iter().collect();
    by_age.sort_by_key(|(w, _)| w.id);
    let mut ids: Vec<String> = Vec::new();
    for (_, id) in by_age {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids
}

/// O que fazer ao clicar num app da dock.
#[derive(Debug, PartialEq, Eq)]
pub enum Click {
    Open,
    Activate(u32),
    Minimize(u32),
}

/// Decide o clique: `windows` são as janelas do app (mais recente primeiro)
/// e `focused` a janela de app em foco por último.
pub fn click_action(windows: &[&Window], focused: Option<u32>) -> Click {
    if windows.is_empty() {
        return Click::Open;
    }
    match windows.iter().position(|w| Some(w.id) == focused && !w.minimized) {
        Some(_) if windows.len() == 1 => Click::Minimize(windows[0].id),
        // Várias janelas: passa para a próxima (a usada há mais tempo).
        Some(_) => Click::Activate(windows[windows.len() - 1].id),
        None => Click::Activate(windows[0].id),
    }
}

/// Estado na thread da UI.
#[derive(Default)]
struct State {
    tracker: Option<Tracker>,
    windows: Vec<(Window, String)>,
    /// Última janela de app em foco. A dock, o menu e o Alt+Tab também são
    /// janelas: clicar neles tira o foco do app, mas ele continua "o ativo".
    focused: Option<u32>,
    /// Lista da dock da última vez (para refazer os extras quando muda).
    dock_apps: String,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

/// Atualiza a dock com a lista nova de janelas.
fn apply(ui: &AppWindow, windows: Vec<(Window, String)>) {
    STATE.with_borrow_mut(|s| {
        if let Some((w, _)) = windows.iter().find(|(w, _)| w.active) {
            s.focused = Some(w.id);
        } else if let Some(f) = s.focused {
            // A janela fechou ou foi minimizada: nenhum app em foco.
            if !windows.iter().any(|(w, _)| w.id == f && !w.minimized) {
                s.focused = None;
            }
        }
        s.windows = windows;
    });
    refresh(ui);
}

/// Passa o estado para a dock: ids abertos, o em foco e os extras.
fn refresh(ui: &AppWindow) {
    let shell = ui.global::<Shell>();
    let dock_list = ui.global::<Theme>().get_dock_apps().to_string();
    let (ids, active) = STATE.with_borrow_mut(|s| {
        s.dock_apps = dock_list.clone();
        let active = s.focused.and_then(|f| s.windows.iter().find(|(w, _)| w.id == f)).map(|(_, id)| id.clone());
        (running_ids(&s.windows), active.unwrap_or_default())
    });
    shell.set_running_ids(ids.join(",").into());
    shell.set_active_app(active.into());
    let pinned = crate::launcher::apps::dock_ids(&dock_list);
    let extra: Vec<AppEntry> = ids
        .iter()
        .filter(|id| *id != "settings" && !pinned.contains(&id.as_str()))
        .filter_map(|id| crate::launcher::apps::entry_for(ui, id))
        .collect();
    let same = {
        let old = shell.get_running_extra();
        old.row_count() == extra.len() && old.iter().zip(&extra).all(|(a, b)| a.id == b.id)
    };
    if !same {
        shell.set_running_extra(ModelRc::from(Rc::new(VecModel::from(extra))));
    }
}

/// O tema de ícones mudou: refaz os apps abertos não fixados (com o ícone novo).
pub fn reload_icons(ui: &AppWindow) {
    ui.global::<Shell>().set_running_extra(ModelRc::default());
    refresh(ui);
}

/// Janelas de um app (mais recente primeiro).
fn windows_of(id: &str) -> Vec<Window> {
    STATE.with_borrow(|s| s.windows.iter().filter(|(_, i)| i == id).map(|(w, _)| w.clone()).collect())
}

/// Clique num app da dock.
fn click(id: &str) {
    let windows = windows_of(id);
    let refs: Vec<&Window> = windows.iter().collect();
    let focused = STATE.with_borrow(|s| s.focused);
    let tracker = STATE.with_borrow(|s| s.tracker.clone());
    match (click_action(&refs, focused), tracker) {
        (Click::Open, _) | (_, None) => {
            if !id.starts_with(WINDOW_PREFIX) {
                crate::shell::session::open(id);
            }
        }
        (Click::Activate(w), Some(t)) => t.activate(w),
        (Click::Minimize(w), Some(t)) => {
            t.minimize(w);
            STATE.with_borrow_mut(|s| s.focused = None);
        }
    }
}

/// Abre o menu do botão direito de um app.
fn open_menu(ui: &AppWindow, id: &str, offset: f32) {
    let total = ui.global::<Apps>().get_dock().row_count() + ui.global::<Shell>().get_running_extra().row_count();
    let _ = Command::new(crate::shell::session::own_exe())
        .args([format!("--menu-dock={id}"), format!("--pos={}", offset.round() as i32), format!("--total={total}")])
        .stdin(Stdio::null())
        .spawn();
}

/// Começa a acompanhar as janelas (área de trabalho e dock da sessão real).
fn start_tracker(ui: &AppWindow) -> Option<Tracker> {
    let weak = ui.as_weak();
    Tracker::start(move |windows| {
        let list = with_ids(windows);
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                apply(&ui, list);
            }
        });
    })
}

/// Liga os apps abertos à dock (área de trabalho e dock própria) e monta o
/// menu da dock (`--menu-dock=`). O timer devolvido precisa ficar vivo.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if !crate::shell::session::is_native() {
        return None;
    }
    let shell = ui.global::<Shell>();
    let weak = ui.as_weak();
    shell.on_save_prefs(move || {
        if let Some(ui) = weak.upgrade() {
            crate::settings::appearance::save_now(&ui);
        }
    });
    match mode {
        Mode::Desktop | Mode::Dock => {}
        Mode::DockMenu => {
            install_menu(ui);
            return None;
        }
        _ => return None,
    }
    let tracker = start_tracker(ui);
    STATE.with_borrow_mut(|s| s.tracker = tracker);
    shell.on_dock_click(|id| click(&id));
    let weak = ui.as_weak();
    shell.on_dock_menu(move |id, offset| {
        if let Some(ui) = weak.upgrade() {
            open_menu(&ui, &id, offset);
        }
    });
    // Mudou a lista de apps fixados (Configurações, menu): refaz os extras.
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(700), move || {
        let Some(ui) = weak.upgrade() else { return };
        let now = ui.global::<Theme>().get_dock_apps();
        if STATE.with_borrow(|s| s.dock_apps != now.as_str()) {
            refresh(&ui);
        }
    });
    Some(timer)
}

/// Argumentos do menu da dock: id, posição do item e total de apps.
pub fn menu_args<I: IntoIterator<Item = String>>(args: I) -> Option<(String, f32, i32)> {
    let mut id = None;
    let (mut pos, mut total) = (0.0, 1);
    for a in args {
        if let Some(v) = a.strip_prefix("--menu-dock=") {
            id = Some(v.to_string());
        } else if let Some(v) = a.strip_prefix("--pos=") {
            pos = v.parse().unwrap_or(0.0);
        } else if let Some(v) = a.strip_prefix("--total=") {
            total = v.parse().unwrap_or(1);
        }
    }
    Some((id?, pos, total))
}

/// Menu do botão direito (`--menu-dock=<id> --pos=<px> --total=<n>`).
fn install_menu(ui: &AppWindow) {
    let Some((id, pos, total)) = menu_args(std::env::args()) else { return };
    let menu = ui.global::<DockMenu>();
    if let Some(entry) = crate::launcher::apps::entry_for(ui, &id) {
        menu.set_app(entry);
    }
    menu.set_offset(pos);
    menu.set_total(total);
    let pinned = crate::launcher::apps::dock_ids(&ui.global::<Theme>().get_dock_apps()).contains(&id.as_str());
    menu.set_pinned(pinned);
    menu.set_can_pin(id != "settings" && !id.starts_with(WINDOW_PREFIX));
    let can_open = id == "settings"
        || (id.starts_with(desktop::ID_PREFIX) && Path::new(&id[desktop::ID_PREFIX.len()..]).is_file())
        || crate::shell::session::command_for(&id).is_some();
    menu.set_can_open(can_open);

    let tracker = Tracker::start(|_| {});
    let windows: Vec<Window> = tracker
        .as_ref()
        .map(|t| with_ids(t.windows()).into_iter().filter(|(_, i)| *i == id).map(|(w, _)| w).collect())
        .unwrap_or_default();
    let items: Vec<DockMenuWindow> = windows
        .iter()
        .map(|w| DockMenuWindow { id: w.id as i32, title: if w.title.is_empty() { w.app_id.clone() } else { w.title.clone() }.into() })
        .collect();
    menu.set_windows(ModelRc::from(Rc::new(VecModel::from(items))));

    let quit = || {
        let _ = slint::quit_event_loop();
    };
    let t = tracker.clone();
    menu.on_focus_window(move |w| {
        if let Some(t) = &t {
            t.activate(w as u32);
        }
        quit();
    });
    let open_id = id.clone();
    menu.on_new_window(move || {
        crate::shell::session::open(&open_id);
        quit();
    });
    let weak = ui.as_weak();
    let pin_id = id.clone();
    menu.on_toggle_pin(move || {
        if let Some(ui) = weak.upgrade() {
            let theme = ui.global::<Theme>();
            theme.set_dock_apps(crate::launcher::apps::dock_toggle(&theme.get_dock_apps(), &pin_id).into());
            crate::settings::appearance::save_now(&ui);
        }
        quit();
    });
    let t = tracker;
    menu.on_quit_app(move || {
        if let Some(t) = &t {
            for w in &windows {
                t.close(w.id);
            }
        }
        quit();
    });
    menu.on_close(quit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn app(file: &str, program: &str, wm_class: &str) -> DesktopApp {
        DesktopApp {
            path: PathBuf::from(format!("/usr/share/applications/{file}.desktop")),
            name: file.into(),
            keywords: String::new(),
            icon: String::new(),
            argv: vec![program.into()],
            terminal: false,
            wm_class: wm_class.into(),
        }
    }

    #[test]
    fn reconhece_a_janela_do_app() {
        let apps = vec![
            app("com.microsoft.VSCode", "/usr/share/code/code", "com.microsoft.VSCode"),
            app("gimp", "gimp-2.10", "gimp"),
            app("org.gnome.Calculator", "gnome-calculator", ""),
        ];
        let vscode = "desktop:/usr/share/applications/com.microsoft.VSCode.desktop";
        // VS Code no X11 (classe "Code"), no Wayland e pelo StartupWMClass.
        assert_eq!(dock_id_with("Code", &apps).as_deref(), Some(vscode));
        assert_eq!(dock_id_with("com.microsoft.VSCode", &apps).as_deref(), Some(vscode));
        assert_eq!(dock_id_with("Gimp-2.10", &apps).as_deref(), Some("desktop:/usr/share/applications/gimp.desktop"));
        assert_eq!(
            dock_id_with("org.gnome.Calculator", &apps).as_deref(),
            Some("desktop:/usr/share/applications/org.gnome.Calculator.desktop")
        );
        // Catálogo, janelas do shell e app desconhecido.
        assert_eq!(dock_id_with("firefox-esr", &apps).as_deref(), Some("browser"));
        assert_eq!(dock_id_with("kalango-settings", &apps).as_deref(), Some("settings"));
        assert_eq!(dock_id_with("kalango-dock", &apps), None);
        let calc = desktop::parse(
            std::path::Path::new("/usr/share/applications/kalango-calculadora.desktop"),
            "[Desktop Entry]\nType=Application\nName=Calculator\nExec=kalango-shell --calculadora\nStartupWMClass=kalango-calculadora\n",
        )
        .unwrap();
        assert_eq!(dock_id_with("kalango-calculadora", std::slice::from_ref(&calc)), Some(calc.id()));
        assert_eq!(dock_id_with("xyz", &apps).as_deref(), Some("window:xyz"));
    }

    fn win(id: u32, minimized: bool) -> Window {
        Window { id, app_id: "foot".into(), title: String::new(), active: false, minimized, maximized: false }
    }

    #[test]
    fn clique_na_dock() {
        let (a, b) = (win(1, false), win(2, false));
        assert_eq!(click_action(&[], None), Click::Open);
        assert_eq!(click_action(&[&a], None), Click::Activate(1));
        assert_eq!(click_action(&[&a], Some(1)), Click::Minimize(1));
        assert_eq!(click_action(&[&b, &a], Some(2)), Click::Activate(1));
        assert_eq!(click_action(&[&b, &a], Some(9)), Click::Activate(2));
        let m = win(1, true);
        assert_eq!(click_action(&[&m], Some(1)), Click::Activate(1));
    }

    #[test]
    fn apps_abertos_sem_repetir() {
        let w = |id, app: &str| (Window { id, ..Default::default() }, app.to_string());
        let list = vec![w(5, "code"), w(2, "files"), w(7, "files")];
        assert_eq!(running_ids(&list), ["files", "code"]);
    }

    #[test]
    fn argumentos_do_menu() {
        let args = ["x", "--menu-dock=files", "--pos=120", "--total=6"].map(String::from);
        assert_eq!(menu_args(args), Some(("files".into(), 120.0, 6)));
        assert_eq!(menu_args(["x".to_string()]), None);
    }
}
