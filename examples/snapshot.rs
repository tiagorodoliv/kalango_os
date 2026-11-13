//! Gera PNGs de cada tela do protótipo, sem abrir janela, para comparar com
//! as referências em `assets/layout/`.
//!
//! Uso: `cargo run --example snapshot -- [pasta_de_saída]` (padrão: `target/snapshots`).
//!
//! Usa uma plataforma Slint mínima com o renderer de software: desenha num
//! buffer em memória, então funciona em qualquer máquina (inclusive CI).
//! A escala 2.0 gera 2880×1800, a mesma resolução das telas de referência.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use kalango_os::{Screen, Theme};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, PhysicalSize, Rgb8Pixel};

/// Tamanho lógico de referência do layout.
const WIDTH: u32 = 1440;
const HEIGHT: u32 = 900;
const SCALE: f32 = 2.0;

/// Plataforma que entrega sempre a mesma janela de software.
struct SnapshotPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for SnapshotPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
}

/// Renderiza a janela no estado atual e grava em `path`.
fn save(window: &MinimalSoftwareWindow, path: &Path) {
    let (w, h) = ((WIDTH as f32 * SCALE) as u32, (HEIGHT as f32 * SCALE) as u32);
    let mut buffer = vec![Rgb8Pixel::default(); (w * h) as usize];

    // Avança timers/animações e força o redesenho completo.
    slint::platform::update_timers_and_animations();
    window.request_redraw();
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, w as usize);
    });

    let file = std::fs::File::create(path).expect("criar PNG");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let bytes: Vec<u8> = buffer.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&bytes))
        .expect("gravar PNG");
    println!("gerado {}", path.display());
}

fn main() {
    let out: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/snapshots"));
    std::fs::create_dir_all(&out).expect("criar pasta de saída");

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    window.dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged { scale_factor: SCALE });
    window.set_size(PhysicalSize::new(
        (WIDTH as f32 * SCALE) as u32,
        (HEIGHT as f32 * SCALE) as u32,
    ));
    slint::platform::set_platform(Box::new(SnapshotPlatform { window: window.clone() }))
        .expect("plataforma já definida");

    let (ui, _timers) = kalango_os::build().expect("criar UI");
    ui.show().expect("mostrar UI");

    // Fundo de circuitos, como nas telas de referência (sem papel de parede).
    ui.global::<Theme>().set_wallpaper(0);
    ui.set_screen(Screen::Login);
    save(&window, &out.join("login.png"));

    ui.set_screen(Screen::Desktop);
    save(&window, &out.join("desktop.png"));

    ui.set_launcher_open(true);
    save(&window, &out.join("menu_apps.png"));
    ui.set_launcher_open(false);

    ui.set_screen(Screen::Settings);
    save(&window, &out.join("config_aparencia.png"));

    // Variações de tema para conferir a troca ao vivo.
    let theme = ui.global::<Theme>();
    theme.set_accent_index(1);
    theme.set_mode(kalango_os::ThemeMode::Light);
    save(&window, &out.join("config_claro_roxo.png"));
    ui.set_screen(Screen::Desktop);
    save(&window, &out.join("desktop_claro_roxo.png"));
    // Login no tema claro: confere a variante do logotipo com letras escuras.
    ui.set_screen(Screen::Login);
    save(&window, &out.join("login_claro_roxo.png"));
}
