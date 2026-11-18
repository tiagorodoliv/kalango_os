//! Estilos Mandacaru dos ícones (Configurações → Barra de tarefas → Ícones),
//! inspirados no tema do sertão da versão 0.1 Mandacaru:
//!
//! - **Barro**: ladrilho de cerâmica terracota e o ícone em tons de creme;
//! - **Caatinga**: cada app num fundo de cor da caatinga (terra, mandacaru,
//!   âmbar, céu...) com o ícone colorido de sempre;
//! - **Noite do sertão**: ladrilho escuro e o ícone em tons de âmbar, como luar.
//!
//! O Slint só sabe pintar uma imagem inteira de uma cor (`colorize`, que vira
//! silhueta). Para o ícone manter os detalhes, as versões tingidas são feitas
//! aqui: o ícone é desenhado em [`PX`] pixels (SVG pelo `resvg`, o mesmo que o
//! Slint usa; PNG pelo `image`), a luminosidade de cada pixel é esticada para
//! ocupar toda a faixa e vira uma cor da rampa do estilo.

use std::path::Path;

use slint::{Color, Image, Rgba8Pixel, SharedPixelBuffer};

/// Lado (px) das versões tingidas (o mesmo dos PNG reduzidos dos ícones).
pub const PX: u32 = 64;

/// Rampa do Barro: do creme queimado ao creme claro.
const BARRO: [(u8, u8, u8); 2] = [(196, 160, 128), (255, 244, 228)];
/// Rampa da Noite do sertão: âmbar escuro, âmbar (destaque do tema) e luar.
const NOITE: [(u8, u8, u8); 3] = [(92, 62, 22), (226, 166, 90), (255, 238, 204)];

/// Cores da caatinga para o fundo de cada app (escolhida pelo id).
const CAATINGA: [u32; 8] = [0x8a4b2a, 0x3f6b4f, 0xa8742f, 0x2f6fae, 0xa8643a, 0x5f7d5a, 0x9c4f32, 0x46648f];

/// Cor da caatinga de um app (sempre a mesma para o mesmo id).
pub fn earth(id: &str) -> Color {
    // FNV-1a: estável entre versões (o `Hash` da biblioteca padrão não é).
    let hash = id.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
    Color::from_argb_encoded(0xff00_0000 | CAATINGA[hash as usize % CAATINGA.len()])
}

/// Ícone em RGBA (não pré-multiplicado), [`PX`]×[`PX`], centralizado.
pub fn rasterize(path: &Path) -> Option<image::RgbaImage> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if ext == "svg" {
        return rasterize_svg(&std::fs::read(path).ok()?);
    }
    let img = image::open(path).ok()?;
    let small = img.resize(PX, PX, image::imageops::FilterType::Lanczos3).to_rgba8();
    let mut out = image::RgbaImage::new(PX, PX);
    let (x, y) = ((PX - small.width()) / 2, (PX - small.height()) / 2);
    image::imageops::overlay(&mut out, &small, i64::from(x), i64::from(y));
    Some(out)
}

fn rasterize_svg(data: &[u8]) -> Option<image::RgbaImage> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = PX as f32 / size.width().max(size.height());
    let dx = (PX as f32 - size.width() * scale) / 2.0;
    let dy = (PX as f32 - size.height() * scale) / 2.0;
    let mut pixmap = tiny_skia::Pixmap::new(PX, PX)?;
    resvg::render(&tree, tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy), &mut pixmap.as_mut());
    // O tiny-skia guarda as cores multiplicadas pelo alfa.
    let raw: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    image::RgbaImage::from_raw(PX, PX, raw)
}

/// Cor da rampa no ponto `t` (0 a 1).
fn ramp(stops: &[(u8, u8, u8)], t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let i = (t as usize).min(stops.len() - 2);
    let f = t - i as f32;
    let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * f).round() as u8;
    let (a, b) = (stops[i], stops[i + 1]);
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// O ícone nos tons da rampa: a luminosidade (esticada entre o pixel mais
/// escuro e o mais claro do ícone) escolhe a cor; o alfa fica.
pub fn tint(src: &image::RgbaImage, stops: &[(u8, u8, u8)]) -> image::RgbaImage {
    let lum = |p: &image::Rgba<u8>| 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]);
    let (lo, hi) = src
        .pixels()
        .filter(|p| p[3] > 128)
        .map(lum)
        .fold((255f32, 0f32), |(lo, hi), l| (lo.min(l), hi.max(l)));
    // Ícone de uma cor só (ou vazio): sem esticar, para não virar um borrão.
    let (lo, span) = if hi - lo < 30.0 { (0.0, 255.0) } else { (lo, hi - lo) };
    let mut out = src.clone();
    for p in out.pixels_mut() {
        let (r, g, b) = ramp(stops, (lum(p) - lo) / span);
        *p = image::Rgba([r, g, b, p[3]]);
    }
    out
}

fn to_image(img: &image::RgbaImage) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(img.as_raw(), img.width(), img.height()))
}

/// As versões Barro e Noite do sertão de um ícone (vazias se não deu para ler).
pub fn variants(path: &Path) -> (Image, Image) {
    match rasterize(path) {
        Some(src) => (to_image(&tint(&src, &BARRO)), to_image(&tint(&src, &NOITE))),
        None => (Image::default(), Image::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rampa_e_tinta() {
        assert_eq!(ramp(&NOITE, 0.0), NOITE[0]);
        assert_eq!(ramp(&NOITE, 0.5), NOITE[1]);
        assert_eq!(ramp(&NOITE, 1.0), NOITE[2]);
        // Preto e branco viram as pontas da rampa; o alfa fica.
        let src = image::RgbaImage::from_raw(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 100]).unwrap();
        let out = tint(&src, &BARRO);
        assert_eq!(out.get_pixel(0, 0).0, [196, 160, 128, 255]);
        assert_eq!(out.get_pixel(1, 0).0, [255, 244, 228, 100]);
    }

    #[test]
    fn desenha_svg() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect x="0" y="0" width="10" height="5" fill="#ff0000"/></svg>"##;
        let img = rasterize_svg(svg).unwrap();
        assert_eq!(img.dimensions(), (PX, PX));
        assert_eq!(img.get_pixel(32, 10).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(32, 50).0[3], 0);
    }

    #[test]
    fn cor_da_caatinga_estavel() {
        assert_eq!(earth("files"), earth("files"));
        assert_eq!(earth("files").alpha(), 255);
    }
}
