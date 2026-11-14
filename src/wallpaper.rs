//! Papéis de parede da área de trabalho.
//!
//! Os modelos (gerados por `assets/wallpapers/gerar.py`) ficam em
//! `/usr/share/backgrounds/kalango` na ISO, ou em `assets/wallpapers/16x9` no
//! desenvolvimento, em três resoluções e uma miniatura cada. A área de
//! trabalho carrega o escolhido (`Theme.wallpaper`) na menor resolução que
//! cubra a tela; as Configurações mostram as miniaturas para escolher.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Image, ModelRc, Timer, TimerMode, VecModel};

use crate::{AppWindow, Mode, Shell, Theme};

/// Modelos na ordem de `Theme.wallpaper-names` (o índice 0 é "Circuitos",
/// o desenho do próprio shell, que não tem arquivo).
pub const MODELS: [&str; 7] = ["PretoAzul", "Preto", "Azul", "Verde", "Roxo", "Laranja", "Branco"];
/// Resoluções disponíveis de cada modelo (largura × altura), da menor para a maior.
const SIZES: [(u32, u32); 3] = [(1920, 1080), (2560, 1440), (3840, 2160)];

/// Nome no `aparencia.conf` ("circuitos" ou o modelo).
pub fn name(index: i32) -> &'static str {
    match index {
        i if i >= 1 && (i as usize) <= MODELS.len() => MODELS[i as usize - 1],
        _ => "circuitos",
    }
}

/// Índice a partir do nome no `aparencia.conf`.
pub fn index(name: &str) -> Option<i32> {
    if name == "circuitos" {
        return Some(0);
    }
    MODELS.iter().position(|m| *m == name).map(|i| i as i32 + 1)
}

/// Menor resolução que cobre a tela (largura física); a maior se a tela for
/// maior que todas ou ainda não tiver tamanho.
pub fn best_size(screen_width: u32) -> (u32, u32) {
    if screen_width == 0 {
        return SIZES[SIZES.len() - 1];
    }
    SIZES
        .iter()
        .copied()
        .find(|(w, _)| *w >= screen_width)
        .unwrap_or(SIZES[SIZES.len() - 1])
}

/// Pastas onde procurar os arquivos: a da ISO e a do repositório (dev).
fn dirs() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/share/backgrounds/kalango"),
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/wallpapers/16x9")),
    ]
}

/// Acha o arquivo de um modelo (`suffix` = "3840x2160", "miniatura"...).
fn file(model: &str, suffix: &str) -> Option<PathBuf> {
    dirs()
        .into_iter()
        .map(|d| d.join(format!("Kalango_{model}_{suffix}.png")))
        .find(|p| p.is_file())
}

/// Carrega o papel de parede `index` para uma tela de `screen_width` pixels.
fn load(index: i32, screen_width: u32) -> Image {
    if index < 1 {
        return Image::default();
    }
    let (w, h) = best_size(screen_width);
    file(name(index), &format!("{w}x{h}"))
        .and_then(|p| Image::load_from_path(&p).ok())
        .unwrap_or_default()
}

/// Liga os papéis de parede à janela: a área de trabalho mostra o escolhido
/// (e troca quando muda nas Configurações, via `aparencia.conf`); as
/// Configurações recebem as miniaturas. O timer devolvido precisa ficar vivo.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    // Miniaturas: nas Configurações e na área de trabalho (no protótipo as
    // Configurações são uma tela dela). São pequenas (384×216).
    if matches!(mode, Mode::Settings | Mode::Desktop) {
        let thumbs: Vec<Image> = std::iter::once(Image::default())
            .chain(MODELS.iter().map(|m| {
                file(m, "miniatura")
                    .and_then(|p| Image::load_from_path(&p).ok())
                    .unwrap_or_default()
            }))
            .collect();
        ui.global::<Shell>()
            .set_wallpaper_thumbs(ModelRc::from(Rc::new(VecModel::from(thumbs))));
    }
    match mode {
        Mode::Desktop => {
            // Confere a escolha e a largura da tela de tempos em tempos: muda
            // quando o usuário escolhe outro nas Configurações ou troca a
            // resolução do monitor.
            let weak = ui.as_weak();
            let mut loaded: Option<(i32, (u32, u32))> = None;
            let timer = Timer::default();
            let mut check = move || {
                let Some(ui) = weak.upgrade() else { return };
                let index = ui.global::<Theme>().get_wallpaper();
                // Antes de a janela ter tamanho (ao iniciar): já carrega o de
                // 1920 px, para a primeira imagem ser o papel de parede
                // escolhido e não os circuitos (que trocavam até 0,7 s depois).
                // Numa tela maior, a próxima conferência troca pelo maior.
                let width = match ui.window().size().width {
                    0 => 1920,
                    w => w,
                };
                let wanted = (index, best_size(width));
                if loaded != Some(wanted) {
                    ui.global::<Shell>().set_wallpaper(load(index, width));
                    loaded = Some(wanted);
                }
            };
            check();
            timer.start(TimerMode::Repeated, Duration::from_millis(700), check);
            Some(timer)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nomes_e_indices() {
        assert_eq!(name(0), "circuitos");
        assert_eq!(name(1), "PretoAzul");
        assert_eq!(name(7), "Branco");
        assert_eq!(name(99), "circuitos");
        assert_eq!(index("circuitos"), Some(0));
        assert_eq!(index("Verde"), Some(4));
        assert_eq!(index("Nenhum"), None);
    }

    #[test]
    fn resolucao_que_cobre_a_tela() {
        assert_eq!(best_size(1440), (1920, 1080));
        assert_eq!(best_size(1920), (1920, 1080));
        assert_eq!(best_size(2560), (2560, 1440));
        assert_eq!(best_size(2880), (3840, 2160));
        assert_eq!(best_size(5120), (3840, 2160));
        assert_eq!(best_size(0), (3840, 2160));
    }

    #[test]
    fn arquivos_do_repositorio_existem() {
        for m in MODELS {
            assert!(file(m, "miniatura").is_some(), "{m}");
            for (w, h) in SIZES {
                assert!(file(m, &format!("{w}x{h}")).is_some(), "{m} {w}x{h}");
            }
        }
    }
}
