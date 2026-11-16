//! Botões das janelas (fechar, maximizar, minimizar) em estilos que combinam
//! com o tema (Configurações → Aparência).
//!
//! O labwc desenha os botões da barra de título a partir de imagens do tema
//! (`close-active.svg`, `close_hover-active.svg`...). A área de trabalho
//! grava os SVGs do estilo escolhido em
//! `~/.local/share/themes/Kalango-Janelas/openbox-3/` (com um `themerc` só
//! com o tamanho dos botões; as cores continuam no `themerc-override` do
//! sistema) e põe o nome do tema e a ordem dos botões no `rc.xml` do usuário
//! ([`crate::shell::labwc::set_window_theme`]). "Padrão" volta aos do labwc.
//!
//! Apps que desenham a própria barra de título (Chrome no modo GTK, Firefox,
//! apps GNOME) seguem a mesma escolha pelo GTK: o lado dos botões vai para o
//! GSettings (`org.gnome.desktop.wm.preferences button-layout`, que o GTK lê
//! no Wayland) e o visual para o `~/.config/gtk-3.0/gtk.css` ([`gtk_css`]).

use std::fs;
use std::path::PathBuf;

/// Estilos, na ordem da Aparência e como ficam no `aparencia.conf`.
pub const STYLES: [&str; 5] = ["padrao", "circulos", "arredondados", "destaque", "retangulares"];
/// Nome do tema do labwc com os botões.
pub const THEME_NAME: &str = "Kalango-Janelas";

/// Os botões que o labwc pede (o "max_toggled" é o de restaurar).
const BUTTONS: [&str; 4] = ["close", "iconify", "max", "max_toggled"];

/// Lados dos botões, como ficam no `aparencia.conf` (0 = esquerda).
pub const SIDES: [&str; 2] = ["esquerda", "direita"];

/// Lado de cada estilo quando a pessoa ainda não escolheu: os círculos (do
/// macOS) à esquerda, os outros à direita.
pub fn natural_side(style: i32) -> i32 {
    if style == 1 { 0 } else { 1 }
}

/// Ordem dos botões na barra de título do labwc (`<titlebar><layout>`).
pub fn layout(side: i32) -> &'static str {
    if side == 0 {
        // Como no macOS: à esquerda, fechar primeiro.
        "close,iconify,max:"
    } else {
        "icon:iconify,max,close"
    }
}

/// O mesmo para o GTK (`button-layout` do GSettings).
pub fn gtk_layout(side: i32) -> &'static str {
    if side == 0 { "close,minimize,maximize:" } else { "appmenu:minimize,maximize,close" }
}

/// Botões das barras de título desenhadas pelo GTK 3 (Chrome no modo GTK,
/// Firefox, apps GNOME), no mesmo estilo dos do labwc. Vazio no "Padrão".
pub fn gtk_css(style: i32, accent: &str) -> String {
    const SEL: &str = "headerbar button.titlebutton, .titlebar button.titlebutton";
    let each = |class: &str, extra: &str| -> String {
        SEL.split(", ").map(|s| format!("{s}{class}{extra}")).collect::<Vec<_>>().join(", ")
    };
    let reset = format!(
        "{SEL} {{\n  padding: 0; box-shadow: none; border: none; background-image: none;\n  \
         -gtk-icon-shadow: none; transition: background-color 150ms, color 150ms;\n}}\n"
    );
    match style {
        1 => format!(
            "{reset}{SEL} {{\n  min-width: 14px; min-height: 14px; margin: 0 3px; border-radius: 50%;\n  \
             color: transparent; -gtk-icon-transform: scale(0.75);\n}}\n\
             {} {{ background-color: #ff5f57; }}\n{} {{ background-color: #febc2e; }}\n\
             {} {{ background-color: #28c840; }}\n{} {{ background-color: #4b5563; }}\n\
             {} {{ color: rgba(59, 10, 10, 0.8); }}\n",
            each(".close", ""),
            each(".minimize", ""),
            each(".maximize", ""),
            each("", ":backdrop"),
            each("", ":hover"),
        ),
        2 => format!(
            "{reset}{SEL} {{\n  min-width: 24px; min-height: 24px; margin: 0 3px; border-radius: 50%;\n  \
             background-color: alpha(white, 0.11); color: #e6edf3;\n}}\n\
             {} {{ background-color: alpha(white, 0.22); }}\n{} {{ background-color: alpha(white, 0.06); color: #8b98a5; }}\n",
            each("", ":hover"),
            each("", ":backdrop"),
        ),
        3 => format!(
            "{reset}{SEL} {{\n  min-width: 24px; min-height: 24px; margin: 0 3px; border-radius: 50%;\n  \
             background-color: transparent; color: {accent};\n}}\n\
             {} {{ background-color: {accent}; color: #04121a; }}\n{} {{ background-color: #f07178; }}\n\
             {} {{ color: #8b98a5; }}\n",
            each("", ":hover"),
            each(".close", ":hover"),
            each("", ":backdrop"),
        ),
        4 => format!(
            "{reset}{SEL} {{\n  min-width: 40px; min-height: 28px; margin: 0; border-radius: 0;\n  \
             background-color: transparent; color: #e6edf3;\n}}\n\
             {} {{ background-color: alpha(white, 0.12); }}\n{} {{ background-color: #e81123; color: white; }}\n\
             {} {{ color: #8b98a5; }}\n",
            each("", ":hover"),
            each(".close", ":hover"),
            each("", ":backdrop"),
        ),
        _ => String::new(),
    }
}

/// O mesmo para os apps GTK 4/libadwaita (Monitor do Sistema, Loja, Agenda):
/// lá o desenho do botão fica na imagem dentro dele
/// (`windowcontrols > button > image`). Vai para `~/.config/gtk-4.0/gtk.css`.
pub fn gtk4_css(style: i32, accent: &str) -> String {
    const B: &str = "windowcontrols > button";
    const I: &str = "windowcontrols > button > image";
    match style {
        1 => format!(
            "{I} {{ min-width: 14px; min-height: 14px; padding: 0; margin: 0 1px; \
             color: transparent; box-shadow: none; -gtk-icon-size: 10px; }}\n\
             {B}.close > image {{ background-color: #ff5f57; }}\n\
             {B}.minimize > image {{ background-color: #febc2e; }}\n\
             {B}.maximize > image {{ background-color: #28c840; }}\n\
             {B}:hover > image {{ color: rgba(59, 10, 10, 0.8); }}\n\
             {B}:backdrop > image {{ background-color: #4b5563; }}\n\
             {B}, {B}:hover, {B}:active {{ background: none; box-shadow: none; }}\n"
        ),
        2 => format!(
            "{I} {{ background-color: alpha(white, 0.11); color: #e6edf3; }}\n\
             {B}:hover > image {{ background-color: alpha(white, 0.22); }}\n"
        ),
        3 => format!(
            "{I} {{ background-color: transparent; color: {accent}; }}\n\
             {B}:hover > image {{ background-color: {accent}; color: #04121a; }}\n\
             {B}.close:hover > image {{ background-color: #f07178; }}\n\
             {B}:backdrop > image {{ color: #8b98a5; }}\n"
        ),
        4 => format!(
            "{I} {{ background-color: transparent; border-radius: 0; color: #e6edf3; }}\n\
             {B} {{ border-radius: 0; min-width: 40px; }}\n\
             {B}:hover {{ background-color: alpha(white, 0.12); }}\n\
             {B}.close:hover {{ background-color: #e81123; }}\n\
             {B}.close:hover > image {{ color: white; }}\n"
        ),
        _ => String::new(),
    }
}

/// Põe o lado dos botões no GSettings (o GTK do Chrome, Firefox e apps GNOME
/// lê na hora). Só grava se mudou.
fn sync_gtk_layout(side: i32) {
    const KEY: [&str; 2] = ["org.gnome.desktop.wm.preferences", "button-layout"];
    let wanted = gtk_layout(side);
    let current = std::process::Command::new("gsettings")
        .arg("get")
        .args(KEY)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().trim_matches('\'').to_string())
        .unwrap_or_default();
    if current != wanted {
        let _ = std::process::Command::new("gsettings").arg("set").args(KEY).arg(wanted).status();
    }
}

/// Largura, altura e espaço entre os botões.
fn size(style: usize) -> (u32, u32, u32) {
    match style {
        1 => (20, 26, 2),
        4 => (40, 26, 0),
        _ => (28, 26, 2),
    }
}

/// O desenho do símbolo (traço) no centro `(cx, cy)`.
fn glyph(button: &str, cx: f32, cy: f32, s: f32, color: &str, width: f32) -> String {
    let h = s / 2.0;
    let line = |x1: f32, y1: f32, x2: f32, y2: f32| {
        format!("<line x1='{x1}' y1='{y1}' x2='{x2}' y2='{y2}' stroke='{color}' stroke-width='{width}' stroke-linecap='round'/>")
    };
    let rect = |x: f32, y: f32, w: f32, hh: f32| {
        format!("<rect x='{x}' y='{y}' width='{w}' height='{hh}' rx='1' fill='none' stroke='{color}' stroke-width='{width}'/>")
    };
    match button {
        "close" => line(cx - h, cy - h, cx + h, cy + h) + &line(cx + h, cy - h, cx - h, cy + h),
        "iconify" => line(cx - h, cy + h * 0.3, cx + h, cy + h * 0.3),
        "max" => rect(cx - h, cy - h, s, s),
        _ => {
            // Restaurar: dois quadrados.
            let q = s * 0.78;
            rect(cx - h, cy - h + s - q, q, q) + &format!(
                "<path d='M{} {} V{} H{} V{}' fill='none' stroke='{color}' stroke-width='{width}' stroke-linejoin='round'/>",
                cx - h + s - q,
                cy - h + s - q,
                cy - h,
                cx + h,
                cy - h + q
            )
        }
    }
}

/// SVG de um botão no estado pedido.
pub fn svg(style: usize, button: &str, active: bool, hover: bool, accent: &str) -> String {
    let (w, h, _) = size(style);
    let (wf, hf) = (w as f32, h as f32);
    let (cx, cy) = (wf / 2.0, hf / 2.0);
    let text = if active { "#e6edf3" } else { "#8b98a5" };
    let body = match style {
        // Círculos coloridos (macOS); o símbolo só aparece com o mouse em cima.
        1 => {
            let color = match (active || hover, button) {
                (false, _) => "#4b5563",
                (true, "close") => "#ff5f57",
                (true, "iconify") => "#febc2e",
                (true, _) => "#28c840",
            };
            let mut b = format!("<circle cx='{cx}' cy='{cy}' r='6' fill='{color}'/>");
            if hover {
                b += &glyph(button, cx, cy, 5.0, "#3b0a0a", 1.3);
            }
            b
        }
        // Arredondados (GNOME): círculo discreto atrás do símbolo.
        2 => {
            let bg = match (hover, active) {
                (true, _) => "#ffffff38",
                (false, true) => "#ffffff1c",
                (false, false) => "#ffffff0e",
            };
            format!("<circle cx='{cx}' cy='{cy}' r='10' fill='{bg}'/>") + &glyph(button, cx, cy, 7.0, text, 1.5)
        }
        // Destaque: símbolo na cor de destaque; com o mouse, círculo cheio.
        3 => {
            if hover {
                let bg = if button == "close" { "#f07178" } else { accent };
                format!("<circle cx='{cx}' cy='{cy}' r='10' fill='{bg}'/>") + &glyph(button, cx, cy, 7.0, "#04121a", 1.6)
            } else {
                glyph(button, cx, cy, 8.0, if active { accent } else { "#8b98a5" }, 1.6)
            }
        }
        // Retangulares (Windows): fundo inteiro com o mouse; fechar fica vermelho.
        4 => {
            let mut b = String::new();
            if hover {
                let bg = if button == "close" { "#e81123" } else { "#ffffff1f" };
                b += &format!("<rect width='{wf}' height='{hf}' fill='{bg}'/>");
            }
            let fg = if hover && button == "close" { "#ffffff" } else { text };
            b + &glyph(button, cx, cy, 9.0, fg, 1.0)
        }
        // Padrão: traço simples (não usado: o padrão é o do próprio labwc).
        _ => glyph(button, cx, cy, 8.0, text, 1.5),
    };
    format!("<svg xmlns='http://www.w3.org/2000/svg' width='{w}' height='{h}' viewBox='0 0 {w} {h}'>{body}</svg>\n")
}

/// `themerc` do tema dos botões: só o tamanho (as cores vêm do override).
pub fn themerc(style: usize) -> String {
    let (w, h, spacing) = size(style);
    format!(
        "# Gerado pelo kalango-shell: botões das janelas (estilo {}).\n\
         window.button.width: {w}\nwindow.button.height: {h}\nwindow.button.spacing: {spacing}\n",
        STYLES[style.min(STYLES.len() - 1)]
    )
}

/// Todos os arquivos do tema: `(nome, conteúdo)`.
pub fn files(style: usize, accent: &str) -> Vec<(String, String)> {
    let mut out = vec![("themerc".to_string(), themerc(style))];
    for b in BUTTONS {
        for (active, suffix) in [(true, "active"), (false, "inactive")] {
            out.push((format!("{b}-{suffix}.svg"), svg(style, b, active, false, accent)));
            out.push((format!("{b}_hover-{suffix}.svg"), svg(style, b, active, true, accent)));
        }
    }
    out
}

fn theme_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("themes").join(THEME_NAME).join("openbox-3"))
}

/// Aplica o estilo e o lado (área de trabalho): grava o tema, ajusta o
/// `rc.xml` e o lado dos botões do GTK.
pub fn apply(style: i32, side: i32, accent: &str) {
    sync_gtk_layout(side);
    let style = style.clamp(0, STYLES.len() as i32 - 1) as usize;
    if style == 0 {
        // Botões do próprio labwc; o lado só vai para o rc.xml se não for o padrão.
        let layout = (side != 1).then(|| (String::new(), layout(side).to_string()));
        crate::shell::labwc::set_window_theme(layout);
        return;
    }
    let Some(dir) = theme_dir() else { return };
    if let Err(err) = fs::create_dir_all(&dir) {
        eprintln!("kalango-shell: botões das janelas: {err}");
        return;
    }
    for (name, text) in files(style, accent) {
        if let Err(err) = fs::write(dir.join(&name), text) {
            eprintln!("kalango-shell: botões das janelas ({name}): {err}");
            return;
        }
    }
    crate::shell::labwc::set_window_theme(Some((THEME_NAME.into(), layout(side).into())));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arquivos_do_tema() {
        let f = files(3, "#3dd6f5");
        // themerc + 4 botões × (ativo, inativo) × (normal, mouse em cima).
        assert_eq!(f.len(), 1 + 16);
        assert!(f.iter().any(|(n, _)| n == "close_hover-active.svg"));
        assert!(f.iter().any(|(n, _)| n == "max_toggled-inactive.svg"));
        let close = &f.iter().find(|(n, _)| n == "close-active.svg").unwrap().1;
        assert!(close.starts_with("<svg") && close.contains("#3dd6f5"));
        assert!(themerc(4).contains("window.button.width: 40"));
        assert_eq!(layout(0), "close,iconify,max:");
        assert_eq!(layout(1), "icon:iconify,max,close");
        assert_eq!(gtk_layout(0), "close,minimize,maximize:");
        assert_eq!((natural_side(1), natural_side(3)), (0, 1));
    }

    #[test]
    fn botoes_no_gtk() {
        assert!(gtk_css(0, "#3dd6f5").is_empty() && gtk4_css(0, "#3dd6f5").is_empty());
        assert!(gtk4_css(1, "#3dd6f5").contains("windowcontrols > button.close > image { background-color: #ff5f57; }"));
        for st in 1..5 {
            let c = gtk4_css(st, "#3dd6f5");
            assert_eq!(c.matches('{').count(), c.matches('}').count(), "gtk4 estilo {st}");
        }
        let c = gtk_css(1, "#3dd6f5");
        assert!(c.contains("headerbar button.titlebutton.close, .titlebar button.titlebutton.close { background-color: #ff5f57; }"));
        assert!(gtk_css(3, "#a78bfa").contains("color: #a78bfa;"));
        // Chaves equilibradas (CSS válido o bastante para o GTK não descartar).
        for st in 1..5 {
            let c = gtk_css(st, "#3dd6f5");
            assert_eq!(c.matches('{').count(), c.matches('}').count(), "estilo {st}");
        }
    }
}
