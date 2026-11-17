//! Preferências de aparência salvas em disco e sincronizadas entre processos.
//!
//! Na sessão real, as Configurações rodam num processo separado da área de
//! trabalho e do menu de apps. Cada processo confere o global `Theme` a cada
//! fração de segundo: se mudou aqui, grava em `~/.config/kalango/aparencia.conf`;
//! se o arquivo mudou (outro processo gravou), aplica. Assim trocar a cor de
//! destaque nas Configurações muda a barra e a dock na hora.
//!
//! O mesmo arquivo guarda as opções da Barra de tarefas (o que a barra
//! superior mostra, tamanho e apps da dock). Na área de trabalho da sessão
//! real, o tamanho da dock também ajusta a margem que o labwc reserva para
//! ela ([`crate::shell::labwc`]).

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use slint::{ComponentHandle, Timer, TimerMode};

use crate::{AppWindow, Mode, Theme, ThemeMode};

/// Intervalo entre as conferências do tema e do arquivo.
const INTERVAL: Duration = Duration::from_millis(500);

/// Preferências das telas Aparência e Barra de tarefas.
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub accent_index: i32,
    pub mode: ThemeMode,
    pub transparency: f32,
    pub blur: bool,
    pub dock_always_visible: bool,
    pub reduce_motion: bool,
    /// Barra superior: dia, segundos, CPU e bateria.
    pub bar_show_date: bool,
    pub bar_show_seconds: bool,
    pub bar_show_cpu: bool,
    pub bar_show_battery: bool,
    /// Lado do botão da dock, em px (16–72).
    pub dock_size: i32,
    /// Ids dos apps da dock, separados por vírgula.
    pub dock_apps: String,
    /// 0 = embaixo, 1 = esquerda, 2 = direita.
    pub dock_position: i32,
    /// 0 = dock centralizada, 1 = barra de ponta a ponta.
    pub dock_style: i32,
    /// Transparência do fundo da dock, 0.0 (opaca) a 0.8.
    pub dock_transparency: f32,
    /// Ícones da dock: 0 = planos, 1 = 3D, 2 = ladrilho na cor do app.
    pub dock_icon_style: i32,
    /// Transparência dos ícones da dock, 0.0 (opacos) a 0.6.
    pub dock_icon_transparency: f32,
    /// Cantos da dock: 0 = retos, 1 = arredondados, 2 = pílula.
    pub dock_corners: i32,
    /// Dock solta da borda ([`DOCK_GAP`] px), como no Ubuntu; só no estilo dock.
    pub dock_detached: bool,
    /// Botão do menu de programas: 0 = no começo, 1 = no fim, 2 = escondido.
    pub dock_apps_button: i32,
    /// 0 = circuitos; 1 em diante, os modelos de [`crate::shell::wallpaper::MODELS`];
    /// [`crate::shell::wallpaper::CUSTOM`] = imagem própria (`wallpaper_file`).
    pub wallpaper: i32,
    /// Imagem própria (cópia reduzida em `~/.local/share/backgrounds`).
    pub wallpaper_file: String,
    /// Família de ícones: 0 = Flat Remix, 1 = Colloid, 2 = Fluent.
    pub icon_theme: i32,
    /// Botões das janelas ([`crate::settings::window_buttons::STYLES`]).
    pub window_buttons: i32,
    /// Lado dos botões: 0 = esquerda, 1 = direita
    /// ([`crate::settings::window_buttons::SIDES`]).
    pub window_buttons_side: i32,
}

/// Nomes da borda da dock no arquivo (índice = `dock_position`).
const DOCK_POSITIONS: [&str; 3] = ["embaixo", "esquerda", "direita"];
/// Nomes do estilo da dock no arquivo (índice = `dock_style`).
const DOCK_STYLES: [&str; 2] = ["dock", "barra"];
/// Ícones da dock no arquivo (índice = `dock_icon_style`).
const DOCK_ICONS: [&str; 3] = ["plano", "3d", "ladrilho"];
/// Cantos da dock no arquivo (índice = `dock_corners`).
const DOCK_CORNERS: [&str; 3] = ["retos", "arredondados", "pilula"];
/// Botão do menu de programas no arquivo (índice = `dock_apps_button`).
const DOCK_APPS_BUTTON: [&str; 3] = ["inicio", "fim", "oculto"];

/// Opções que chegaram numa atualização e mudam o visual, com o valor que
/// mantém a interface como era antes delas.
///
/// Uma atualização não muda a aparência de quem já usa o KalangOS: se o
/// `aparencia.conf` existe e não tem a chave, vale o valor daqui (o
/// comportamento antigo), e a pessoa liga a novidade nas Configurações se
/// quiser. Só quem instala do zero (sem arquivo) recebe os padrões novos.
/// Toda opção nova que muda o que aparece na tela entra nesta lista.
const LEGACY: &[(&str, &str)] = &[
    // 0.1.1-2: botão do menu de programas na dock (padrão novo: no fim).
    ("dock-botao-apps", "oculto"),
];

/// Folga entre a dock solta e a borda da tela, em px lógicos (o mesmo de
/// `Theme.dock-gap` em ui/theme.slint).
pub const DOCK_GAP: u32 = 8;

/// As 6 cores de destaque (mesma ordem de `Theme.accents` em ui/theme.slint).
pub const ACCENTS: [&str; 6] = ["#3dd6f5", "#a78bfa", "#7ea6f7", "#5fd38d", "#e8b04b", "#f28b95"];

/// Cores dos temas de ícones do Kalango (mesma ordem de [`ACCENTS`]).
const ICON_COLORS: [&str; 6] = ["Ciano", "Roxo", "Azul", "Verde", "Ambar", "Rosa"];
/// Famílias de ícones (Configurações → Aparência), como ficam no arquivo.
const ICON_FAMILIES: [&str; 3] = ["flatremix", "colloid", "fluent"];

/// Nome do tema de ícones para uma família (0 = Flat Remix, 1 = Colloid,
/// 2 = Fluent) e uma cor de destaque. Instalados por `distro/icones/`.
pub fn icon_theme_name(family: i32, accent: i32) -> String {
    let color = ICON_COLORS[accent.clamp(0, 5) as usize];
    match family {
        1 => format!("Kalango-Colloid-{color}"),
        2 => format!("Kalango-Fluent-{color}"),
        _ => format!("Kalango-{color}"),
    }
}

/// Marca do `~/.config/gtk-3.0/gtk.css` gerado pelo shell.
const GTK_MARK: &str = "/* Gerado pelo kalango-shell (Configurações → Aparência). */";

/// Texto do `gtk.css` do usuário: a cor de destaque do tema GTK "Kalango"
/// (`/usr/share/themes/Kalango`, cor nomeada `@kalango_accent`) e os botões
/// das barras de título desenhadas pelo GTK no estilo escolhido.
pub fn gtk_css(accent_index: i32, window_buttons: i32) -> String {
    let color = ACCENTS[accent_index.clamp(0, 5) as usize];
    let buttons = crate::settings::window_buttons::gtk_css(window_buttons, color);
    format!("{GTK_MARK}
@define-color kalango_accent {color};
{buttons}")
}

/// Troca o tema de ícones (família e pastas na cor de destaque). Só mexe se
/// o tema atual for um dos do Kalango: um tema escolhido pelo usuário fica.
/// Os apps GTK abertos trocam os ícones na hora.
fn sync_icon_theme(family: i32, accent_index: i32) -> String {
    const KEY: [&str; 2] = ["org.gnome.desktop.interface", "icon-theme"];
    let wanted = icon_theme_name(family, accent_index);
    let Ok(out) = std::process::Command::new("gsettings").arg("get").args(KEY).output() else {
        return wanted;
    };
    let current = String::from_utf8_lossy(&out.stdout).trim().trim_matches('\'').to_string();
    if current == wanted || !(current.is_empty() || current.starts_with("Kalango-")) {
        return current;
    }
    let _ = std::process::Command::new("gsettings")
        .arg("set")
        .args(KEY)
        .arg(&wanted)
        .status();
    wanted
}

/// Tema de ícones que a área de trabalho põe para essas escolhas (o mesmo
/// critério de `sync_icon_theme`, sem gravar nada): para a dock em janela
/// própria trocar os ícones junto, sem esperar o GSettings.
fn expected_icon_theme(family: i32, accent_index: i32) -> String {
    let wanted = icon_theme_name(family, accent_index);
    let current = crate::launcher::desktop::read_icon_theme();
    if current.is_empty() || current.starts_with("Kalango-") || current == "hicolor" { wanted } else { current }
}

/// Grava a cor de destaque no `gtk.css` do usuário (apps GTK abertos depois
/// já usam a cor nova). Um `gtk.css` feito à mão (sem a marca) fica intocado.
fn sync_gtk_css(accent_index: i32, window_buttons: i32) {
    let Some(base) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    else {
        return;
    };
    let accent = ACCENTS[accent_index.clamp(0, 5) as usize];
    let gtk4 = format!("{GTK_MARK}\n{}", crate::settings::window_buttons::gtk4_css(window_buttons, accent));
    for (dir, wanted) in [("gtk-3.0", gtk_css(accent_index, window_buttons)), ("gtk-4.0", gtk4)] {
        let path = base.join(dir).join("gtk.css");
        let current = fs::read_to_string(&path).ok();
        // Um gtk.css feito à mão (sem a marca) fica intocado.
        if current.as_deref().is_some_and(|t| !t.starts_with(GTK_MARK)) || current.as_deref() == Some(wanted.as_str()) {
            continue;
        }
        let _ = fs::create_dir_all(base.join(dir));
        if let Err(err) = fs::write(&path, wanted) {
            eprintln!("kalango-shell: falha ao gravar {}: {err}", path.display());
        }
    }
}

/// Ajustes fora do shell que seguem as preferências (só a área de trabalho
/// faz, um processo por sessão): margem da dock no labwc, destaque do GTK e
/// cor das pastas (tema de ícones).
fn sync_system(ui: &AppWindow, now: &Prefs, before: Option<&Prefs>) {
    if before.is_none_or(|b| b.dock_margins() != now.dock_margins()) {
        crate::shell::labwc::set_dock_margins(now.dock_margins());
    }
    if before.is_none() {
        crate::shell::labwc::sync_layouts_menu();
    }
    // Destaque e botões das janelas (o estilo "destaque" segue a cor de destaque).
    let buttons_changed = before.is_none_or(|b| {
        b.window_buttons != now.window_buttons
            || b.window_buttons_side != now.window_buttons_side
            || b.accent_index != now.accent_index
    });
    if buttons_changed {
        sync_gtk_css(now.accent_index, now.window_buttons);
        let accent = ACCENTS[now.accent_index.clamp(0, ACCENTS.len() as i32 - 1) as usize];
        crate::settings::window_buttons::apply(now.window_buttons, now.window_buttons_side, accent);
    }
    if before.is_none_or(|b| b.accent_index != now.accent_index || b.icon_theme != now.icon_theme) {
        let theme = sync_icon_theme(now.icon_theme, now.accent_index);
        // Dock e menu desta janela com os ícones do tema novo.
        if before.is_some() {
            crate::launcher::apps::reload_icons(ui, &theme);
        }
    }
}

/// Tamanho da dock (lado do botão, px): limites (o padrão, 52, fica no Theme).
const DOCK_SIZE_MIN: i32 = 16;
const DOCK_SIZE_MAX: i32 = 72;

/// Tamanho da dock lido do arquivo: px (`52`) ou os nomes antigos
/// (`pequena`, `media`, `grande`), sempre de 4 em 4 entre 36 e 72.
pub fn parse_dock_size(value: &str) -> Option<i32> {
    let px = match value {
        "pequena" => 44,
        "media" => 52,
        "grande" => 60,
        _ => value.parse().ok()?,
    };
    Some(clamp_dock_size(px))
}

/// Põe o tamanho da dock entre os limites, de 4 em 4.
pub fn clamp_dock_size(px: i32) -> i32 {
    let px = px.clamp(DOCK_SIZE_MIN, DOCK_SIZE_MAX);
    DOCK_SIZE_MIN + (px - DOCK_SIZE_MIN + 2) / 4 * 4
}

impl Prefs {
    /// Lê as preferências atuais do global `Theme`.
    fn from_ui(ui: &AppWindow) -> Prefs {
        let t = ui.global::<Theme>();
        Prefs {
            accent_index: t.get_accent_index(),
            mode: t.get_mode(),
            transparency: t.get_transparency(),
            dock_transparency: t.get_dock_transparency(),
            dock_icon_style: t.get_dock_icon_style(),
            dock_icon_transparency: t.get_dock_icon_transparency(),
            dock_corners: t.get_dock_corners(),
            dock_detached: t.get_dock_detached(),
            dock_apps_button: t.get_dock_apps_button(),
            blur: t.get_blur(),
            dock_always_visible: t.get_dock_always_visible(),
            reduce_motion: t.get_reduce_motion(),
            bar_show_date: t.get_bar_show_date(),
            bar_show_seconds: t.get_bar_show_seconds(),
            bar_show_cpu: t.get_bar_show_cpu(),
            bar_show_battery: t.get_bar_show_battery(),
            dock_size: t.get_dock_size(),
            dock_apps: t.get_dock_apps().into(),
            dock_position: t.get_dock_position(),
            dock_style: t.get_dock_style(),
            wallpaper: t.get_wallpaper(),
            wallpaper_file: t.get_wallpaper_file().into(),
            icon_theme: t.get_icon_theme(),
            window_buttons: t.get_window_buttons(),
            window_buttons_side: t.get_window_buttons_side(),
        }
    }

    /// Aplica as preferências ao global `Theme`.
    fn apply(&self, ui: &AppWindow) {
        let t = ui.global::<Theme>();
        t.set_accent_index(self.accent_index);
        t.set_mode(self.mode);
        t.set_transparency(self.transparency);
        t.set_dock_transparency(self.dock_transparency);
        t.set_dock_icon_style(self.dock_icon_style);
        t.set_dock_icon_transparency(self.dock_icon_transparency);
        t.set_dock_corners(self.dock_corners);
        t.set_dock_detached(self.dock_detached);
        t.set_dock_apps_button(self.dock_apps_button);
        t.set_blur(self.blur);
        t.set_dock_always_visible(self.dock_always_visible);
        t.set_reduce_motion(self.reduce_motion);
        t.set_bar_show_date(self.bar_show_date);
        t.set_bar_show_seconds(self.bar_show_seconds);
        t.set_bar_show_cpu(self.bar_show_cpu);
        t.set_bar_show_battery(self.bar_show_battery);
        t.set_dock_size(self.dock_size);
        t.set_dock_apps(self.dock_apps.clone().into());
        t.set_dock_position(self.dock_position);
        t.set_dock_style(self.dock_style);
        t.set_wallpaper(self.wallpaper);
        t.set_wallpaper_file(self.wallpaper_file.clone().into());
        t.set_icon_theme(self.icon_theme);
        t.set_window_buttons(self.window_buttons);
        t.set_window_buttons_side(self.window_buttons_side);
    }

    /// Margens que o labwc reserva para a dock na borda dela: a espessura da
    /// dock (botão + respiro de 9 px de cada lado; 6 px na barra), mais a
    /// folga quando ela está solta da borda. Com a dock escondida, nenhuma
    /// (os apps maximizados ocupam a tela toda).
    pub fn dock_margins(&self) -> crate::shell::labwc::Margins {
        let mut m = crate::shell::labwc::Margins::default();
        if !self.dock_always_visible {
            return m;
        }
        let item = clamp_dock_size(self.dock_size) as u32;
        // Encostada na borda, a margem é só a espessura; solta, mais a folga
        // (a mesma do lado de fora, para as janelas não colarem nela).
        let size = if self.dock_style == 1 { item + 12 } else { item + 18 + self.dock_gap() };
        match self.dock_position {
            1 => m.left = size,
            2 => m.right = size,
            _ => m.bottom = size,
        }
        m
    }

    /// Folga da dock até a borda (0 se encostada ou no estilo barra).
    pub fn dock_gap(&self) -> u32 {
        if self.dock_detached && self.dock_style == 0 { DOCK_GAP } else { 0 }
    }

    /// Texto do arquivo (`chave=valor`, uma por linha).
    pub fn to_text(&self) -> String {
        let mode = match self.mode {
            ThemeMode::Oled => "oled",
            ThemeMode::Light => "claro",
            _ => "vidro",
        };
        format!(
            "# Aparência do KalangOS (gravado pelas Configurações).\n\
             destaque={}\ntema={}\ntransparencia={}\ndesfoque={}\n\
             dock-sempre-visivel={}\nreduzir-animacoes={}\n\
             # Barra de tarefas\n\
             barra-data={}\nbarra-segundos={}\nbarra-cpu={}\nbarra-bateria={}\n\
             dock-tamanho={}\ndock-apps={}\ndock-posicao={}\ndock-estilo={}\ndock-transparencia={}\n\
             dock-icones={}\ndock-icones-transparencia={}\ndock-cantos={}\n\
             dock-solta={}\ndock-botao-apps={}\n\
             papel-de-parede={}\npapel-de-parede-arquivo={}\nicones={}\nbotoes-janela={}\nbotoes-janela-lado={}\n",
            self.accent_index,
            mode,
            self.transparency,
            self.blur,
            self.dock_always_visible,
            self.reduce_motion,
            self.bar_show_date,
            self.bar_show_seconds,
            self.bar_show_cpu,
            self.bar_show_battery,
            clamp_dock_size(self.dock_size),
            self.dock_apps,
            DOCK_POSITIONS[self.dock_position.clamp(0, 2) as usize],
            DOCK_STYLES[self.dock_style.clamp(0, 1) as usize],
            self.dock_transparency,
            DOCK_ICONS[self.dock_icon_style.clamp(0, 2) as usize],
            self.dock_icon_transparency,
            DOCK_CORNERS[self.dock_corners.clamp(0, 2) as usize],
            self.dock_detached,
            DOCK_APPS_BUTTON[self.dock_apps_button.clamp(0, 2) as usize],
            crate::shell::wallpaper::name(self.wallpaper),
            self.wallpaper_file,
            ICON_FAMILIES[self.icon_theme.clamp(0, 2) as usize],
            crate::settings::window_buttons::STYLES
                [self.window_buttons.clamp(0, crate::settings::window_buttons::STYLES.len() as i32 - 1) as usize],
            crate::settings::window_buttons::SIDES[self.window_buttons_side.clamp(0, 1) as usize],
        )
    }

    /// Lê o texto do arquivo por cima de `base`; chaves ausentes ou inválidas
    /// mantêm o valor de `base`.
    ///
    /// Num arquivo que já tem alguma opção, as chaves de [`LEGACY`] que faltam
    /// valem o valor antigo (a atualização não muda o visual sozinha).
    pub fn parse(text: &str, base: &Prefs) -> Prefs {
        let mut p = base.clone();
        let keys: Vec<&str> = text.lines().filter_map(|l| l.split_once('=')).map(|(k, _)| k.trim()).collect();
        let legacy: Vec<String> = if keys.is_empty() {
            Vec::new()
        } else {
            LEGACY.iter().filter(|(k, _)| !keys.contains(k)).map(|(k, v)| format!("{k}={v}")).collect()
        };
        for line in legacy.iter().map(String::as_str).chain(text.lines()) {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "destaque" => {
                    if let Ok(v) = value.parse::<i32>() {
                        p.accent_index = v.clamp(0, 5);
                    }
                }
                "tema" => {
                    p.mode = match value {
                        "oled" => ThemeMode::Oled,
                        "claro" => ThemeMode::Light,
                        "vidro" => ThemeMode::Glass,
                        _ => p.mode,
                    }
                }
                "dock-transparencia" => {
                    if let Ok(v) = value.parse::<f32>() {
                        p.dock_transparency = v.clamp(0.0, 0.8);
                    }
                }
                "dock-icones-transparencia" => {
                    if let Ok(v) = value.parse::<f32>() {
                        p.dock_icon_transparency = v.clamp(0.0, 0.6);
                    }
                }
                "dock-icones" => {
                    if let Some(i) = DOCK_ICONS.iter().position(|s| *s == value) {
                        p.dock_icon_style = i as i32;
                    }
                }
                "dock-solta" => p.dock_detached = value.parse().unwrap_or(p.dock_detached),
                "dock-botao-apps" => {
                    if let Some(i) = DOCK_APPS_BUTTON.iter().position(|s| *s == value) {
                        p.dock_apps_button = i as i32;
                    }
                }
                "dock-cantos" => {
                    if let Some(i) = DOCK_CORNERS.iter().position(|s| *s == value) {
                        p.dock_corners = i as i32;
                    }
                }
                "transparencia" => {
                    if let Ok(v) = value.parse::<f32>() {
                        p.transparency = v.clamp(0.0, 1.0);
                    }
                }
                "desfoque" => p.blur = value.parse().unwrap_or(p.blur),
                "dock-sempre-visivel" => {
                    p.dock_always_visible = value.parse().unwrap_or(p.dock_always_visible)
                }
                "reduzir-animacoes" => p.reduce_motion = value.parse().unwrap_or(p.reduce_motion),
                "barra-data" => p.bar_show_date = value.parse().unwrap_or(p.bar_show_date),
                "barra-segundos" => {
                    p.bar_show_seconds = value.parse().unwrap_or(p.bar_show_seconds)
                }
                "barra-cpu" => p.bar_show_cpu = value.parse().unwrap_or(p.bar_show_cpu),
                "barra-bateria" => {
                    p.bar_show_battery = value.parse().unwrap_or(p.bar_show_battery)
                }
                "dock-tamanho" => {
                    if let Some(px) = parse_dock_size(value) {
                        p.dock_size = px;
                    }
                }
                "dock-posicao" => {
                    if let Some(i) = DOCK_POSITIONS.iter().position(|s| *s == value) {
                        p.dock_position = i as i32;
                    }
                }
                "dock-estilo" => {
                    if let Some(i) = DOCK_STYLES.iter().position(|s| *s == value) {
                        p.dock_style = i as i32;
                    }
                }
                "icones" => {
                    if let Some(i) = ICON_FAMILIES.iter().position(|f| *f == value) {
                        p.icon_theme = i as i32;
                    }
                }
                "papel-de-parede" => {
                    if let Some(i) = crate::shell::wallpaper::index(value) {
                        p.wallpaper = i;
                    }
                }
                "papel-de-parede-arquivo" => p.wallpaper_file = value.to_string(),
                "botoes-janela" => {
                    if let Some(i) = crate::settings::window_buttons::STYLES.iter().position(|s| *s == value) {
                        p.window_buttons = i as i32;
                        // Arquivo de antes do lado: o lado natural do estilo.
                        if !text.contains("botoes-janela-lado=") {
                            p.window_buttons_side = crate::settings::window_buttons::natural_side(p.window_buttons);
                        }
                    }
                }
                "botoes-janela-lado" => {
                    if let Some(i) = crate::settings::window_buttons::SIDES.iter().position(|s| *s == value) {
                        p.window_buttons_side = i as i32;
                    }
                }
                "dock-apps" => {
                    let ids = crate::launcher::apps::dock_ids(value);
                    if !ids.is_empty() {
                        p.dock_apps = ids.join(",");
                    }
                }
                _ => {}
            }
        }
        p
    }
}

/// Troca chaves do `aparencia.conf` sem mexer no resto (quem escolhe o papel
/// de parede fora das Configurações, como o "Abrir com" do Thunar). Os
/// processos abertos aplicam sozinhos ao ver o arquivo mudar.
pub fn set_keys(keys: &[(&str, &str)]) -> std::io::Result<()> {
    let path = config_path().ok_or_else(|| std::io::Error::other("sem pasta pessoal"))?;
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !keys.iter().any(|(k, _)| l.split_once('=').is_some_and(|(lk, _)| lk.trim() == *k)))
        .map(str::to_string)
        .collect();
    if lines.is_empty() {
        lines.push("# Aparência do KalangOS (gravado pelas Configurações).".into());
    }
    lines.extend(keys.iter().map(|(k, v)| format!("{k}={v}")));
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&path, lines.join("\n") + "\n")
}

/// Caminho do arquivo: `$XDG_CONFIG_HOME/kalango/aparencia.conf`
/// (ou `~/.config/...`). `None` sem pasta pessoal.
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("kalango/aparencia.conf"))
}

/// Grava a aparência da tela agora (o menu da dock e o de apps fixam um
/// app e fecham antes da conferência periódica).
pub fn save_now(ui: &AppWindow) {
    let Some(path) = config_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Err(err) = fs::write(&path, Prefs::from_ui(ui).to_text()) {
        eprintln!("kalango-shell: falha ao salvar a aparência: {err}");
    }
}

fn modified(path: &PathBuf) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Aplica a aparência salva em disco, uma vez (janelas de vida curta, como
/// a do Alt+Tab, que não acompanham as mudanças).
pub fn load(ui: &AppWindow) {
    if let Some(text) = config_path().and_then(|p| fs::read_to_string(p).ok()) {
        Prefs::parse(&text, &Prefs::from_ui(ui)).apply(ui);
    }
}

/// Carrega o arquivo (se existir) e mantém tema e arquivo em sincronia.
///
/// Só roda na sessão real: no desenvolvimento e nas capturas o tema começa
/// sempre no padrão. Na área de trabalho, também mantém a margem da dock no
/// labwc. O timer devolvido precisa ficar vivo.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if !crate::shell::session::is_native() {
        return None;
    }
    let path = config_path()?;
    load(ui);
    let mut last = Prefs::from_ui(ui);
    // Só a área de trabalho (um processo por sessão) mexe no labwc.
    let desktop = mode == Mode::Desktop;
    let shows_dock = matches!(mode, Mode::Dock | Mode::Launcher);
    if desktop {
        sync_system(ui, &last, None);
    }
    let mut last_mtime = modified(&path);

    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, INTERVAL, move || {
        let Some(ui) = weak.upgrade() else { return };
        let now = Prefs::from_ui(&ui);
        if now != last {
            // Mudou aqui (tela Aparência): grava para os outros processos.
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if let Err(err) = fs::write(&path, now.to_text()) {
                eprintln!("kalango-shell: falha ao salvar a aparência: {err}");
            }
            last_mtime = modified(&path);
            if desktop {
                sync_system(&ui, &now, Some(&last));
            }
            last = now;
        } else if modified(&path) != last_mtime {
            // Outro processo gravou: aplica aqui.
            last_mtime = modified(&path);
            if let Ok(text) = fs::read_to_string(&path) {
                let p = Prefs::parse(&text, &now);
                p.apply(&ui);
                if desktop {
                    sync_system(&ui, &p, Some(&last));
                } else if shows_dock && (p.icon_theme != last.icon_theme || p.accent_index != last.accent_index) {
                    // A dock em janela própria e o menu de apps também trocam
                    // os ícones (antes só a área de trabalho trocava).
                    let theme = expected_icon_theme(p.icon_theme, p.accent_index);
                    crate::launcher::apps::reload_icons(&ui, &theme);
                }
                last = p;
            }
        }
    });
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padrao() -> Prefs {
        Prefs {
            accent_index: 0,
            mode: ThemeMode::Glass,
            transparency: 0.6,
            dock_transparency: 0.0,
            dock_icon_style: 0,
            dock_icon_transparency: 0.0,
            dock_corners: 1,
            dock_detached: false,
            dock_apps_button: 1,
            blur: true,
            dock_always_visible: true,
            reduce_motion: false,
            bar_show_date: true,
            bar_show_seconds: false,
            bar_show_cpu: true,
            bar_show_battery: true,
            dock_size: 52,
            dock_apps: "files,terminal,browser,code,store".into(),
            dock_position: 0,
            dock_style: 0,
            wallpaper: 1,
            wallpaper_file: String::new(),
            icon_theme: 0,
            window_buttons: 0,
            window_buttons_side: 1,
        }
    }

    #[test]
    fn grava_e_le_de_volta() {
        let p = Prefs {
            accent_index: 3,
            mode: ThemeMode::Light,
            transparency: 0.25,
            dock_transparency: 0.4,
            dock_icon_style: 1,
            dock_icon_transparency: 0.2,
            dock_corners: 2,
            dock_detached: true,
            dock_apps_button: 0,
            blur: false,
            dock_always_visible: false,
            reduce_motion: true,
            bar_show_date: false,
            bar_show_seconds: true,
            bar_show_cpu: false,
            bar_show_battery: false,
            dock_size: 60,
            dock_apps: "files,music,calendar".into(),
            dock_position: 1,
            dock_style: 1,
            wallpaper: 0,
            wallpaper_file: String::new(),
            icon_theme: 2,
            window_buttons: 0,
            window_buttons_side: 0,
        };
        assert_eq!(Prefs::parse(&p.to_text(), &padrao()), p);
    }

    #[test]
    fn ignora_valores_invalidos() {
        let p = Prefs::parse("destaque=99\ntema=xyz\nlixo\ndesfoque=talvez\n", &padrao());
        assert_eq!(p.accent_index, 5);
        assert_eq!(p.mode, ThemeMode::Glass);
        assert!(p.blur);
    }

    #[test]
    fn arquivo_antigo_mantem_o_padrao_da_barra() {
        // aparencia.conf de antes da Barra de tarefas: só as chaves da Aparência.
        let p = Prefs::parse("destaque=2\ndock-tamanho=enorme\ndock-apps= , \n", &padrao());
        assert_eq!(p.accent_index, 2);
        assert_eq!(p.dock_size, 52);
        assert_eq!(p.dock_apps, "files,terminal,browser,code,store");
    }

    #[test]
    fn nome_do_tema_de_icones() {
        assert_eq!(icon_theme_name(0, 0), "Kalango-Ciano");
        assert_eq!(icon_theme_name(1, 1), "Kalango-Colloid-Roxo");
        assert_eq!(icon_theme_name(2, 4), "Kalango-Fluent-Ambar");
        assert_eq!(icon_theme_name(9, 99), "Kalango-Rosa");
    }

    #[test]
    fn destaque_do_gtk() {
        assert!(gtk_css(1, 0).ends_with("@define-color kalango_accent #a78bfa;\n"));
        assert!(gtk_css(99, 0).contains("#f28b95"));
        // Com os círculos, os botões das barras do GTK vêm junto.
        assert!(gtk_css(0, 1).contains("button.titlebutton.close"));
        // Arquivo antigo (sem o lado): círculos à esquerda, os outros à direita.
        assert_eq!(Prefs::parse("botoes-janela=circulos\n", &padrao()).window_buttons_side, 0);
        assert_eq!(Prefs::parse("botoes-janela=destaque\n", &padrao()).window_buttons_side, 1);
        assert_eq!(Prefs::parse("botoes-janela=circulos\nbotoes-janela-lado=direita\n", &padrao()).window_buttons_side, 1);
    }

    #[test]
    fn tamanho_da_dock_em_px_e_nomes_antigos() {
        assert_eq!(parse_dock_size("pequena"), Some(44));
        assert_eq!(parse_dock_size("grande"), Some(60));
        assert_eq!(parse_dock_size("64"), Some(64));
        assert_eq!(parse_dock_size("65"), Some(64));
        assert_eq!(parse_dock_size("10"), Some(16));
        assert_eq!(parse_dock_size("16"), Some(16));
        assert_eq!(parse_dock_size("999"), Some(72));
        assert_eq!(parse_dock_size("enorme"), None);
    }

    #[test]
    fn margens_da_dock() {
        use crate::shell::labwc::Margins;
        let mut p = padrao();
        assert_eq!(p.dock_margins(), Margins { bottom: 70, left: 0, right: 0 });
        p.dock_size = 44;
        assert_eq!(p.dock_margins().bottom, 62);
        p.dock_size = 60;
        p.dock_position = 1;
        assert_eq!(p.dock_margins(), Margins { bottom: 0, left: 78, right: 0 });
        p.dock_style = 1;
        p.dock_position = 2;
        assert_eq!(p.dock_margins(), Margins { bottom: 0, left: 0, right: 72 });
        p.dock_always_visible = false;
        assert_eq!(p.dock_margins(), Margins::default());
    }

    #[test]
    fn dock_solta_da_borda() {
        use crate::shell::labwc::Margins;
        let mut p = padrao();
        p.dock_detached = true;
        // 52 + 18 de respiro + 8 de folga.
        assert_eq!(p.dock_margins(), Margins { bottom: 78, left: 0, right: 0 });
        // No estilo barra a folga não vale: continua encostada.
        p.dock_style = 1;
        assert_eq!(p.dock_gap(), 0);
        assert_eq!(p.dock_margins().bottom, 64);
    }

    #[test]
    fn botao_do_menu_de_programas() {
        assert_eq!(padrao().dock_apps_button, 1);
        let p = Prefs::parse("dock-botao-apps=oculto\ndock-solta=true\n", &padrao());
        assert_eq!((p.dock_apps_button, p.dock_detached), (2, true));
        // Valor desconhecido mantém o padrão (no fim).
        assert_eq!(Prefs::parse("dock-botao-apps=meio\n", &padrao()).dock_apps_button, 1);
    }

    #[test]
    fn atualizacao_nao_muda_o_visual() {
        // Arquivo de antes do botão: continua sem o botão.
        assert_eq!(Prefs::parse("destaque=2\ndock-tamanho=52\n", &padrao()).dock_apps_button, 2);
        // Quem escolheu, fica com a escolha.
        assert_eq!(Prefs::parse("destaque=2\ndock-botao-apps=inicio\n", &padrao()).dock_apps_button, 0);
        // Sem arquivo (instalação nova): padrão novo.
        assert_eq!(Prefs::parse("", &padrao()).dock_apps_button, 1);
    }
}
