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

use std::fs;
use std::path::PathBuf;

/// Estilos, na ordem da Aparência e como ficam no `aparencia.conf`.
pub const STYLES: [&str; 5] = ["padrao", "circulos", "arredondados", "destaque", "retangulares"];
/// Nome do tema do labwc com os botões.
pub const THEME_NAME: &str = "Kalango-Janelas";

/// Os botões que o labwc pede (o "max_toggled" é o de restaurar).
const BUTTONS: [&str; 4] = ["close", "iconify", "max", "max_toggled"];

/// Ordem dos botões na barra de título (`<titlebar><layout>`).
pub fn layout(style: usize) -> &'static str {
    match style {
        // Como no macOS: à esquerda, fechar primeiro.
        1 => "close,iconify,max:",
        _ => "icon:iconify,max,close",
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

/// Aplica o estilo (área de trabalho): grava o tema e ajusta o `rc.xml`.
pub fn apply(style: i32, accent: &str) {
    let style = style.clamp(0, STYLES.len() as i32 - 1) as usize;
    if style == 0 {
        crate::shell::labwc::set_window_theme(None);
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
    crate::shell::labwc::set_window_theme(Some((THEME_NAME.into(), layout(style).into())));
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
        assert_eq!(layout(1), "close,iconify,max:");
        assert_eq!(layout(2), "icon:iconify,max,close");
    }
}
