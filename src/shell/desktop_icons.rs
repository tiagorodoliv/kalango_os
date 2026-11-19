//! Ícones da área de trabalho: a Lixeira e o que está na pasta Desktop
//! (`XDG_DESKTOP_DIR`), como no Ubuntu e no Windows.
//!
//! A área de trabalho lê a pasta a cada 1,5 s (nomes e datas: só refaz quando
//! algo muda) e monta a grade: a Lixeira primeiro, depois as pastas e os
//! arquivos em ordem alfabética, de cima para baixo e da esquerda para a
//! direita, abaixo da barra e fora da dock. O lugar de cada ícone arrastado
//! fica em `~/.config/kalango/area-de-trabalho.conf` (`nome<TAB>coluna<TAB>linha`).
//!
//! Ícones: o do app (`.desktop`, como o atalho de um app do Chrome), a
//! miniatura das imagens e o do tipo de arquivo no tema de ícones. Abrir:
//! apps pelo comando do `.desktop`, pastas no Thunar, o resto no programa
//! padrão (`xdg-open`). Mover para a lixeira pelo `gio trash`; Propriedades
//! pela janela do Thunar (D-Bus `org.xfce.FileManager`).
//!
//! Ligado/desligado em Configurações → Aparência (`Theme.desktop-icons`).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use slint::{Color, ComponentHandle, Image, Model, ModelRc, Timer, TimerMode, VecModel};

use crate::common::i18n::tr;
use crate::launcher::desktop;
use crate::{AppWindow, DesktopIcon, DesktopIcons, Mode, Shell, Theme};

/// Lado (px) das miniaturas de imagens.
const THUMB: u32 = 96;

/// Um arquivo da pasta Desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    /// "app", "folder", "image" ou "file".
    pub kind: &'static str,
}

/// Tipo pelo nome do arquivo.
pub fn kind_of(path: &Path, is_dir: bool) -> &'static str {
    if is_dir {
        return "folder";
    }
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("desktop") => "app",
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp") => "image",
        _ => "file",
    }
}

/// Nome do ícone do tema para um tipo de arquivo (pela extensão).
pub fn mime_icon(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("pdf") => "application-pdf",
        Some("odt" | "doc" | "docx" | "rtf") => "x-office-document",
        Some("ods" | "xls" | "xlsx" | "csv") => "x-office-spreadsheet",
        Some("odp" | "ppt" | "pptx") => "x-office-presentation",
        Some("zip" | "tar" | "gz" | "xz" | "bz2" | "7z" | "rar" | "zst") => "package-x-generic",
        Some("mp3" | "ogg" | "flac" | "wav" | "m4a" | "opus") => "audio-x-generic",
        Some("mp4" | "mkv" | "webm" | "avi" | "mov") => "video-x-generic",
        Some("iso" | "img") => "media-optical",
        Some("deb") => "application-x-deb",
        Some("sh" | "py" | "rs" | "c" | "cpp" | "h" | "js" | "ts" | "json" | "html" | "css" | "xml" | "toml") => {
            "text-x-script"
        }
        _ => "text-x-generic",
    }
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

/// A pasta Desktop (`~/Desktop`, `~/Área de trabalho`...).
pub fn desktop_dir() -> PathBuf {
    PathBuf::from(crate::shell::session::xdg_dir(&home(), "XDG_DESKTOP_DIR", "Desktop"))
}

/// Os arquivos da pasta (sem os ocultos): pastas primeiro, depois por nome.
pub fn scan(dir: &Path) -> Vec<Entry> {
    let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Entry> = read
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| {
            let path = e.path();
            let is_dir = path.is_dir();
            let kind = kind_of(&path, is_dir);
            let mut name = e.file_name().to_string_lossy().into_owned();
            if kind == "app" {
                if let Some(app) = fs::read_to_string(&path).ok().and_then(|t| desktop::parse(&path, &t)) {
                    name = app.name;
                }
            }
            Entry { path, name, kind }
        })
        .collect();
    out.sort_by(|a, b| (a.kind != "folder", a.name.to_lowercase()).cmp(&(b.kind != "folder", b.name.to_lowercase())));
    out
}

/// "Assinatura" da pasta: nomes e datas (para só refazer quando mudar).
fn signature(dir: &Path) -> Vec<(String, Option<SystemTime>)> {
    let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
    let mut v: Vec<(String, Option<SystemTime>)> = read
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.metadata().and_then(|m| m.modified()).ok()))
        .collect();
    v.sort();
    v
}

fn positions_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("kalango/area-de-trabalho.conf"))
}

/// Lugares guardados: nome do arquivo (ou "trash") → (coluna, linha).
pub fn parse_positions(text: &str) -> HashMap<String, (i32, i32)> {
    text.lines()
        .filter_map(|l| {
            let mut p = l.split('\t');
            let (name, c, r) = (p.next()?, p.next()?.parse().ok()?, p.next()?.parse().ok()?);
            Some((name.to_string(), (c, r)))
        })
        .collect()
}

fn load_positions() -> HashMap<String, (i32, i32)> {
    positions_path().and_then(|p| fs::read_to_string(p).ok()).map(|t| parse_positions(&t)).unwrap_or_default()
}

fn save_positions(map: &HashMap<String, (i32, i32)>) {
    let Some(path) = positions_path() else { return };
    let mut lines: Vec<String> = map.iter().map(|(n, (c, r))| format!("{n}\t{c}\t{r}")).collect();
    lines.sort();
    let _ = path.parent().map(fs::create_dir_all);
    let _ = fs::write(path, lines.join("\n") + "\n");
}

/// Põe os ícones na grade de `rows` linhas: os que têm lugar guardado (e
/// livre) ficam nele; os outros ocupam os lugares livres, coluna por coluna.
/// `keys` na ordem de exibição; devolve (coluna, linha) de cada um.
pub fn layout(keys: &[String], saved: &HashMap<String, (i32, i32)>, rows: i32, cols: i32) -> Vec<(i32, i32)> {
    let rows = rows.max(1);
    let cols = cols.max(1);
    let mut taken: Vec<(i32, i32)> = Vec::new();
    let mut out: Vec<Option<(i32, i32)>> = vec![None; keys.len()];
    for (i, k) in keys.iter().enumerate() {
        if let Some(&(c, r)) = saved.get(k) {
            if c >= 0 && r >= 0 && c < cols && r < rows && !taken.contains(&(c, r)) {
                out[i] = Some((c, r));
                taken.push((c, r));
            }
        }
    }
    let mut next = 0;
    for slot in out.iter_mut() {
        if slot.is_some() {
            continue;
        }
        while taken.contains(&(next / rows, next % rows)) {
            next += 1;
        }
        let cell = (next / rows, next % rows);
        taken.push(cell);
        *slot = Some(cell);
    }
    out.into_iter().map(|c| c.unwrap_or((0, 0))).collect()
}

/// Ícone do tema atual.
fn themed(name: &str) -> Option<Image> {
    desktop::find_themed_icon(name, &desktop::current_icon_theme(), &desktop::data_dirs())
        .and_then(|p| crate::launcher::apps::load_icon(&p))
}

/// Miniatura de uma imagem.
fn thumbnail(path: &Path) -> Option<Image> {
    let img = image::open(path).ok()?;
    let small = img.thumbnail(THUMB, THUMB).to_rgba8();
    Some(Image::from_rgba8(slint::SharedPixelBuffer::clone_from_slice(small.as_raw(), small.width(), small.height())))
}

/// Ícone de um arquivo (e a cor, se for o de traço do shell).
fn icon_for(e: &Entry) -> (Image, Color) {
    let none = Color::from_argb_u8(0, 0, 0, 0);
    let found = match e.kind {
        "app" => fs::read_to_string(&e.path)
            .ok()
            .and_then(|t| desktop::parse(&e.path, &t))
            .and_then(|app| crate::launcher::apps::desktop_icon(&app))
            .and_then(|p| crate::launcher::apps::load_icon(&p))
            .or_else(|| themed("application-x-executable")),
        "folder" => themed("folder"),
        "image" => thumbnail(&e.path).or_else(|| themed("image-x-generic")),
        _ => themed(mime_icon(&e.path)).or_else(|| themed("text-x-generic")),
    };
    // Sem ícone no tema: imagem vazia, e a tela usa o ícone de traço do shell.
    (found.unwrap_or_default(), none)
}

/// Estado da área de trabalho (thread da interface).
struct State {
    dir: PathBuf,
    sig: Vec<(String, Option<SystemTime>)>,
    entries: Vec<Entry>,
    /// Ícones já carregados (caminho e data → imagem).
    cache: HashMap<(PathBuf, Option<SystemTime>), (Image, Color)>,
    trash_full: Option<bool>,
    grid: (i32, i32),
    theme: String,
}

thread_local! {
    static STATE: std::cell::RefCell<Option<State>> = const { std::cell::RefCell::new(None) };
}

/// Chave guardada de cada ícone (nome do arquivo ou "trash").
fn key_of(id: &str) -> String {
    if id == "trash" {
        return "trash".into();
    }
    Path::new(id).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Refaz a lista na tela.
fn refresh(ui: &AppWindow, force: bool) {
    let theme = ui.global::<Theme>();
    let size = ui.window().size();
    let scale = ui.window().scale_factor().max(0.1);
    let (w, h) = (size.width as f32 / scale, size.height as f32 / scale);
    if w < 10.0 {
        return;
    }
    // Fora da barra e da dock.
    let bar = theme.get_bar_height();
    let dock = if theme.get_dock_always_visible() { theme.get_dock_thickness() + theme.get_dock_gap() } else { 0.0 };
    let left = if theme.get_dock_position() == 1 { dock } else { 0.0 };
    let right = if theme.get_dock_position() == 2 { dock } else { 0.0 };
    let bottom = if theme.get_dock_position() == 0 { dock } else { 0.0 };
    let icons = ui.global::<DesktopIcons>();
    let (cw, ch) = (icons.get_cell_w(), icons.get_cell_h());
    // Na ISO live o "Instalar KalangOS" fica no canto: a grade começa abaixo.
    let live = if ui.global::<Shell>().get_live() { 140.0 } else { 0.0 };
    let (ox, oy) = (16.0 + left, bar + 12.0 + live);
    let rows = ((h - oy - bottom - 8.0) / ch).floor() as i32;
    let cols = ((w - ox - right - 8.0) / cw).floor() as i32;
    icons.set_origin_x(ox);
    icons.set_origin_y(oy);

    let full = crate::shell::trash::is_full();
    let theme_name = desktop::current_icon_theme();
    let changed = STATE.with_borrow_mut(|s| {
        let s = s.get_or_insert_with(|| State {
            dir: desktop_dir(),
            sig: Vec::new(),
            entries: Vec::new(),
            cache: HashMap::new(),
            trash_full: None,
            grid: (0, 0),
            theme: String::new(),
        });
        let sig = signature(&s.dir);
        let mut changed = force || sig != s.sig || s.trash_full != Some(full) || s.grid != (rows, cols);
        if s.theme != theme_name {
            s.cache.clear();
            s.theme = theme_name.clone();
            changed = true;
        }
        if sig != s.sig {
            s.entries = scan(&s.dir);
            s.sig = sig;
        }
        s.trash_full = Some(full);
        s.grid = (rows, cols);
        changed
    });
    if !changed {
        return;
    }

    let items = STATE.with_borrow_mut(|s| {
        let s = s.as_mut().expect("estado");
        let mut keys = vec!["trash".to_string()];
        keys.extend(s.entries.iter().map(|e| key_of(&e.path.to_string_lossy())));
        let cells = layout(&keys, &load_positions(), rows, cols);
        let trash_icon = themed(if full { "user-trash-full" } else { "user-trash" });
        let mut items = vec![DesktopIcon {
            id: "trash".into(),
            name: tr("Trash", "Lixeira").into(),
            icon: trash_icon.unwrap_or_default(),
            tint: Color::from_argb_u8(0, 0, 0, 0),
            kind: "trash".into(),
            col: cells[0].0,
            row: cells[0].1,
        }];
        for (i, e) in s.entries.iter().enumerate() {
            let stamp = fs::metadata(&e.path).and_then(|m| m.modified()).ok();
            let (icon, tint) =
                s.cache.entry((e.path.clone(), stamp)).or_insert_with(|| icon_for(e)).clone();
            items.push(DesktopIcon {
                id: e.path.to_string_lossy().into_owned().into(),
                name: e.name.clone().into(),
                icon,
                tint,
                kind: e.kind.into(),
                col: cells[i + 1].0,
                row: cells[i + 1].1,
            });
        }
        items
    });
    icons.set_items(ModelRc::from(Rc::new(VecModel::from(items))));
}

/// Abre um ícone.
fn open(id: &str) {
    if id == "trash" {
        crate::shell::session::open("trash");
        return;
    }
    let path = Path::new(id);
    match kind_of(path, path.is_dir()) {
        "app" => {
            crate::shell::session::open(&format!("{}{id}", desktop::ID_PREFIX));
        }
        "folder" => {
            let _ = Command::new("thunar").arg(id).stdin(Stdio::null()).spawn();
        }
        _ => {
            crate::shell::session::open_file(id);
        }
    }
}

/// Novo nome pelo zenity (sem "/" nem nome vazio); `None` = cancelou.
fn ask_name(current: &str) -> Option<String> {
    let out = Command::new("zenity")
        .args([
            "--entry".to_string(),
            format!("--title={}", tr("Rename", "Renomear")),
            format!("--text={}", tr("New name:", "Novo nome:")),
            format!("--entry-text={current}"),
        ])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !name.is_empty() && !name.contains('/') && name != "." && name != "..").then_some(name)
}

/// Ação do menu de um ícone.
fn action(id: &str, what: &str) {
    match what {
        "open" => open(id),
        "empty" => crate::shell::trash::empty(),
        "trash" => {
            let _ = Command::new("gio").args(["trash", id]).stdin(Stdio::null()).spawn();
        }
        "properties" => {
            let uri = format!("file://{id}");
            let _ = Command::new("gdbus")
                .args([
                    "call",
                    "--session",
                    "--dest",
                    "org.xfce.FileManager",
                    "--object-path",
                    "/org/xfce/FileManager",
                    "--method",
                    "org.xfce.FileManager.DisplayFileProperties",
                    &uri,
                    "",
                    "",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .spawn();
        }
        "rename" => {
            let path = PathBuf::from(id);
            let current = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            // Numa thread: o zenity espera a pessoa digitar.
            std::thread::spawn(move || {
                let Some(name) = ask_name(&current) else { return };
                let target = path.with_file_name(&name);
                if target.exists() {
                    return;
                }
                if fs::rename(&path, &target).is_ok() {
                    // O lugar guardado segue o arquivo.
                    let mut map = load_positions();
                    if let Some(cell) = map.remove(&current) {
                        map.insert(name, cell);
                        save_positions(&map);
                    }
                }
            });
        }
        _ => {}
    }
}

/// Liga os ícones à área de trabalho (sessão real). O timer devolvido precisa
/// ficar vivo.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if mode != Mode::Desktop || !crate::shell::session::is_native() {
        return None;
    }
    let icons = ui.global::<DesktopIcons>();
    icons.on_open(|id| open(&id));
    icons.on_action(|id, what| action(&id, &what));
    let weak = ui.as_weak();
    icons.on_move_to(move |id, col, row| {
        let Some(ui) = weak.upgrade() else { return };
        let mut map = load_positions();
        let key = key_of(&id);
        // Lugar ocupado por outro: troca os dois de lugar.
        let current = ui
            .global::<DesktopIcons>()
            .get_items()
            .iter()
            .find(|i| i.id == id)
            .map(|i| (i.col, i.row));
        let other = ui
            .global::<DesktopIcons>()
            .get_items()
            .iter()
            .find(|i| i.id != id && (i.col, i.row) == (col, row))
            .map(|i| key_of(&i.id));
        if let (Some(other), Some(cell)) = (other, current) {
            map.insert(other, cell);
        }
        map.insert(key, (col, row));
        save_positions(&map);
        refresh(&ui, true);
    });
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(1500), move || {
        let Some(ui) = weak.upgrade() else { return };
        if !ui.global::<Theme>().get_desktop_icons() || ui.global::<Shell>().get_app_fullscreen() {
            return;
        }
        refresh(&ui, false);
    });
    // Primeira vez logo que a janela tiver tamanho.
    let weak = ui.as_weak();
    slint::Timer::single_shot(Duration::from_millis(300), move || {
        if let Some(ui) = weak.upgrade() {
            refresh(&ui, true);
        }
    });
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grade_com_lugares_guardados() {
        let keys: Vec<String> = ["trash", "a", "b", "c"].iter().map(|s| s.to_string()).collect();
        // Sem nada guardado: de cima para baixo na 1ª coluna.
        assert_eq!(layout(&keys, &HashMap::new(), 3, 5), [(0, 0), (0, 1), (0, 2), (1, 0)]);
        // "b" guardado no meio da tela; os outros seguem preenchendo.
        let saved = parse_positions("b\t4\t2\nlixo\nc\t0\t0\n");
        assert_eq!(layout(&keys, &saved, 3, 5), [(0, 1), (0, 2), (4, 2), (0, 0)]);
        // Lugar fora da tela (monitor menor): volta para a grade.
        let fora = parse_positions("a\t20\t20\n");
        assert_eq!(layout(&keys, &fora, 3, 5)[1], (0, 1));
    }

    #[test]
    fn tipos_e_icones() {
        assert_eq!(kind_of(Path::new("/x/WhatsApp.desktop"), false), "app");
        assert_eq!(kind_of(Path::new("/x/foto.JPG"), false), "image");
        assert_eq!(kind_of(Path::new("/x/Projetos"), true), "folder");
        assert_eq!(mime_icon(Path::new("a.pdf")), "application-pdf");
        assert_eq!(mime_icon(Path::new("planilha.xlsx")), "x-office-spreadsheet");
        assert_eq!(mime_icon(Path::new("semextensao")), "text-x-generic");
    }

    #[test]
    fn le_a_pasta() {
        let dir = std::env::temp_dir().join(format!("kalango-desktop-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Pasta")).unwrap();
        fs::write(dir.join("b.txt"), "x").unwrap();
        fs::write(dir.join(".oculto"), "x").unwrap();
        fs::write(dir.join("app.desktop"), "[Desktop Entry]\nType=Application\nName=WhatsApp\nExec=chrome --app\n").unwrap();
        let names: Vec<String> = scan(&dir).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Pasta", "b.txt", "WhatsApp"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
