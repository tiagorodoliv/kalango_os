//! Papéis de parede da área de trabalho.
//!
//! Os modelos (gerados por `assets/wallpapers/gerar.py`) ficam em
//! `/usr/share/backgrounds/kalango` na ISO, ou em `assets/wallpapers/16x9` no
//! desenvolvimento, em três resoluções e uma miniatura cada. A área de
//! trabalho carrega o escolhido (`Theme.wallpaper`) na menor resolução que
//! cubra a tela; as Configurações mostram as miniaturas para escolher.
//!
//! **Imagem própria** ([`CUSTOM`]): a pessoa escolhe um arquivo (zenity) e o
//! shell grava uma cópia reduzida (cobre 2560×1440, JPEG) e uma miniatura em
//! `~/.local/share/backgrounds`, como o Ubuntu. Reduzir importa: uma foto de
//! celular aberta inteira ocuparia ~100 MB na área de trabalho.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Image, ModelRc, Timer, TimerMode, VecModel};

use crate::{AppWindow, Mode, Shell, Theme};

/// Modelos na ordem de `Theme.wallpaper-names` (o índice 0 é "Circuitos",
/// o desenho do próprio shell, que não tem arquivo).
pub const MODELS: [&str; 7] = ["PretoAzul", "Preto", "Azul", "Verde", "Roxo", "Laranja", "Branco"];
/// Resoluções disponíveis de cada modelo (largura × altura), da menor para a maior.
const SIZES: [(u32, u32); 3] = [(1920, 1080), (2560, 1440), (3840, 2160)];

/// Índice da imagem própria (depois dos modelos).
pub const CUSTOM: i32 = MODELS.len() as i32 + 1;
/// A cópia da imagem própria cobre esta resolução (o resto é corte).
const CUSTOM_COVER: (u32, u32) = (2560, 1440);
/// Tamanho das miniaturas (o mesmo dos modelos).
const THUMB: (u32, u32) = (384, 216);

/// Nome no `aparencia.conf` ("circuitos", o modelo ou "arquivo").
pub fn name(index: i32) -> &'static str {
    match index {
        CUSTOM => "arquivo",
        i if i >= 1 && (i as usize) <= MODELS.len() => MODELS[i as usize - 1],
        _ => "circuitos",
    }
}

/// Índice a partir do nome no `aparencia.conf`.
pub fn index(name: &str) -> Option<i32> {
    if name == "circuitos" {
        return Some(0);
    }
    if name == "arquivo" {
        return Some(CUSTOM);
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

/// Tamanho que cobre `cover` mantendo a proporção de `(w, h)`, sem
/// aumentar uma imagem menor.
pub fn cover_size((w, h): (u32, u32), cover: (u32, u32)) -> (u32, u32) {
    let scale = (cover.0 as f64 / w as f64).max(cover.1 as f64 / h as f64);
    if scale >= 1.0 || w == 0 || h == 0 {
        return (w, h);
    }
    (((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1))
}

/// Pasta das imagens próprias (`~/.local/share/backgrounds`).
fn custom_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("backgrounds"))
}

/// Miniatura de uma imagem própria (ao lado dela, `-miniatura.jpg`).
pub fn thumb_of(file: &Path) -> PathBuf {
    let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    file.with_file_name(format!("{stem}-miniatura.jpg"))
}

/// Prepara uma imagem escolhida para papel de parede: grava a cópia reduzida
/// e a miniatura e devolve o caminho da cópia.
pub fn import(source: &Path) -> Result<PathBuf, String> {
    let img = image::ImageReader::open(source)
        .and_then(|r| r.with_guessed_format())
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let dir = custom_dir().ok_or("sem pasta pessoal")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stem: String = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .take(60)
        .collect();
    let dest = dir.join(format!("kalango-{stem}.jpg"));
    let (w, h) = cover_size((img.width(), img.height()), CUSTOM_COVER);
    let big = if (w, h) == (img.width(), img.height()) {
        img.clone()
    } else {
        img.resize_exact(w, h, image::imageops::FilterType::Triangle)
    };
    // JPEG não tem canal alfa.
    big.to_rgb8().save(&dest).map_err(|e| e.to_string())?;
    img.resize_to_fill(THUMB.0, THUMB.1, image::imageops::FilterType::Triangle)
        .to_rgb8()
        .save(thumb_of(&dest))
        .map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Carrega o papel de parede `index` para uma tela de `screen_width` pixels
/// (`file` = a imagem própria, para [`CUSTOM`]).
fn load(index: i32, screen_width: u32, file_path: &str) -> Image {
    if index == CUSTOM {
        return Image::load_from_path(Path::new(file_path)).unwrap_or_default();
    }
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
        show_custom_thumb(ui);
    }
    if mode == Mode::Settings {
        install_picker(ui);
    }
    match mode {
        // A área de trabalho e o fundo dos monitores extras.
        Mode::Desktop | Mode::Fundo => {
            // Confere a escolha e a largura da tela de tempos em tempos: muda
            // quando o usuário escolhe outro nas Configurações ou troca a
            // resolução do monitor.
            let weak = ui.as_weak();
            let mut loaded: Option<(i32, (u32, u32), String)> = None;
            let timer = Timer::default();
            let mut check = move || {
                let Some(ui) = weak.upgrade() else { return };
                let index = ui.global::<Theme>().get_wallpaper();
                let custom = ui.global::<Theme>().get_wallpaper_file().to_string();
                // Antes de a janela ter tamanho (ao iniciar): já carrega o de
                // 1920 px, para a primeira imagem ser o papel de parede
                // escolhido e não os circuitos (que trocavam até 0,7 s depois).
                // Numa tela maior, a próxima conferência troca pelo maior.
                let width = match ui.window().size().width {
                    0 => 1920,
                    w => w,
                };
                let wanted = (index, best_size(width), if index == CUSTOM { custom } else { String::new() });
                if loaded.as_ref() != Some(&wanted) {
                    ui.global::<Shell>().set_wallpaper(load(index, width, &wanted.2));
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

/// Arquivo pedido por `--papel-de-parede ARQUIVO` (ou `--papel-de-parede=ARQUIVO`).
pub fn file_from_args<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        if let Some(f) = a.strip_prefix("--papel-de-parede=") {
            return Some(f.to_string());
        }
        if a == "--papel-de-parede" {
            return args.next();
        }
    }
    None
}

/// Modo `--papel-de-parede ARQUIVO` ("Abrir com" do Thunar): prepara a
/// imagem e grava a escolha; a área de trabalho troca sozinha. Sem janela.
pub fn run_cli(file: &str) -> Result<(), String> {
    let dest = import(Path::new(file))?;
    crate::settings::appearance::set_keys(&[
        ("papel-de-parede", name(CUSTOM)),
        ("papel-de-parede-arquivo", &dest.to_string_lossy()),
    ])
    .map_err(|e| e.to_string())
}

/// Mostra a miniatura da imagem própria escolhida (se houver).
fn show_custom_thumb(ui: &AppWindow) {
    let file = ui.global::<Theme>().get_wallpaper_file().to_string();
    let thumb = (!file.is_empty())
        .then(|| Image::load_from_path(&thumb_of(Path::new(&file))).ok())
        .flatten()
        .unwrap_or_default();
    ui.global::<Shell>().set_wallpaper_custom_thumb(thumb);
}

/// "Escolher imagem..." na Aparência: zenity, cópia reduzida, e a escolha vai
/// para o `aparencia.conf` (a área de trabalho troca sozinha).
fn install_picker(ui: &AppWindow) {
    let weak = ui.as_weak();
    ui.global::<Shell>().on_pick_wallpaper(move || {
        let Some(ui) = weak.upgrade() else { return };
        let shell = ui.global::<Shell>();
        if shell.get_wallpaper_busy() {
            return;
        }
        shell.set_wallpaper_busy(true);
        shell.set_wallpaper_message("".into());
        let weak = ui.as_weak();
        std::thread::spawn(move || {
            let title = crate::common::i18n::tr("Choose an image", "Escolha uma imagem");
            let filter = crate::common::i18n::tr("Images", "Imagens");
            let picked = std::process::Command::new("zenity")
                .args([
                    "--file-selection".to_string(),
                    format!("--title={title}"),
                    format!("--file-filter={filter} | *.jpg *.jpeg *.png *.JPG *.JPEG *.PNG"),
                ])
                .stdin(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|p| !p.is_empty());
            let result = picked.map(|p| import(Path::new(&p)));
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                let shell = ui.global::<Shell>();
                shell.set_wallpaper_busy(false);
                match result {
                    None => {}
                    Some(Ok(dest)) => {
                        let theme = ui.global::<Theme>();
                        theme.set_wallpaper_file(dest.to_string_lossy().into_owned().into());
                        theme.set_wallpaper(CUSTOM);
                        show_custom_thumb(&ui);
                    }
                    Some(Err(err)) => {
                        eprintln!("kalango-shell: papel de parede: {err}");
                        shell.set_wallpaper_message(
                            crate::common::i18n::tr("Could not open this image", "Não foi possível abrir essa imagem").into(),
                        );
                    }
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arquivo_pela_linha_de_comando() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(file_from_args(args(&["x", "--papel-de-parede", "/a b.png"])).as_deref(), Some("/a b.png"));
        assert_eq!(file_from_args(args(&["x", "--papel-de-parede=/c.jpg"])).as_deref(), Some("/c.jpg"));
        assert_eq!(file_from_args(args(&["x", "--settings"])), None);
    }

    #[test]
    fn imagem_propria_reduzida_cobrindo_a_tela() {
        assert_eq!(cover_size((6000, 4000), (2560, 1440)), (2560, 1707));
        assert_eq!(cover_size((4000, 6000), (2560, 1440)), (2560, 3840));
        assert_eq!(cover_size((1920, 1080), (2560, 1440)), (1920, 1080));
        assert_eq!(name(CUSTOM), "arquivo");
        assert_eq!(index("arquivo"), Some(CUSTOM));
        assert_eq!(thumb_of(Path::new("/x/kalango-praia.jpg")), PathBuf::from("/x/kalango-praia-miniatura.jpg"));
    }

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
