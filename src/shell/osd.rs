//! Aviso flutuante das teclas de volume, microfone e brilho (como no macOS e
//! no Ubuntu): `kalango-shell --osd=volume:45` (ou `volume:mudo:45`,
//! `microfone:30`, `brilho:80`), chamado pelo `kalango-midia`.
//!
//! O primeiro pedido abre, separado, o processo do aviso (`--osd=servidor`),
//! que fica aberto na sessão escutando `$XDG_RUNTIME_DIR/kalango-osd.sock`;
//! todo pedido só manda o texto por ele e sai na hora (~3 ms).
//!
//! O aviso é uma camada "overlay" do Wayland (`zwlr_layer_shell_v1`, que o
//! labwc tem): não recebe foco nem cliques (quem está digitando continua
//! digitando), fica por cima até de vídeo em tela cheia e vai embaixo, no
//! meio, logo acima da dock. O winit não cria camadas; por isso o desenho é
//! do renderer de software do Slint numa plataforma própria
//! ([`MinimalSoftwareWindow`]), copiado para um buffer de memória
//! compartilhada (`wl_shm`). Some 1,5 s depois do último aperto.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::os::unix::fs::FileExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use slint::platform::software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType};
use slint::ComponentHandle;
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_region, wl_registry, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::OsdView;

/// Tamanho do aviso (o mesmo do `OsdView`).
const W: u32 = 320;
const H: u32 = 64;
/// Tempo na tela depois do último aperto.
const VISIBLE: Duration = Duration::from_millis(1500);

/// Um pedido: o que mostrar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    /// 0 volume, 1 sem som, 2 microfone, 3 microfone mudo, 4 brilho
    /// (o `kind` do `OsdView`).
    pub kind: i32,
    pub level: i32,
    pub muted: bool,
}

impl Request {
    /// `volume:45`, `volume:mudo:45`, `microfone:30`, `microfone:mudo:30`,
    /// `brilho:80`.
    pub fn parse(text: &str) -> Option<Request> {
        let mut parts = text.trim().split(':');
        let what = parts.next()?;
        let mut muted = false;
        let mut level = parts.next()?;
        if level == "mudo" {
            muted = true;
            level = parts.next().unwrap_or("0");
        }
        let level = level.trim().parse::<i32>().ok()?.clamp(0, 100);
        let kind = match (what, muted) {
            ("volume", false) => 0,
            ("volume", true) => 1,
            ("microfone", false) => 2,
            ("microfone", true) => 3,
            ("brilho", _) => 4,
            _ => return None,
        };
        Some(Request { kind, level, muted })
    }
}

/// O pedido de `--osd=...` nos argumentos (`None` = não é este modo).
pub fn request_from_args<I: IntoIterator<Item = String>>(args: I) -> Option<String> {
    args.into_iter().find_map(|a| a.strip_prefix("--osd=").map(str::to_string))
}

fn socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join("kalango-osd.sock"))
}

/// Manda o pedido ao processo do aviso; `false` se ele não está aberto.
fn send(text: &str) -> bool {
    let Some(path) = socket_path() else { return false };
    let Ok(mut stream) = UnixStream::connect(path) else { return false };
    stream.write_all(text.as_bytes()).is_ok()
}

/// Plataforma do Slint só com a janela de software do aviso.
struct Platform(Rc<MinimalSoftwareWindow>);

impl slint::platform::Platform for Platform {
    fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

/// Estado da conexão Wayland.
#[derive(Default)]
struct Wl {
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    configured: bool,
    closed: bool,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Wl {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_compositor" => state.compositor = Some(registry.bind(name, version.min(4), qh, ())),
                "wl_shm" => state.shm = Some(registry.bind(name, 1, qh, ())),
                "zwlr_layer_shell_v1" => state.layer_shell = Some(registry.bind(name, version.min(4), qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for Wl {
    fn event(state: &mut Self, surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, event: zwlr_layer_surface_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, .. } => {
                surface.ack_configure(serial);
                state.configured = true;
            }
            zwlr_layer_surface_v1::Event::Closed => state.closed = true,
            _ => {}
        }
    }
}

delegate_noop!(Wl: ignore wl_compositor::WlCompositor);
delegate_noop!(Wl: ignore wl_surface::WlSurface);
delegate_noop!(Wl: ignore wl_shm::WlShm);
delegate_noop!(Wl: ignore wl_shm_pool::WlShmPool);
delegate_noop!(Wl: ignore wl_buffer::WlBuffer);
delegate_noop!(Wl: ignore wl_region::WlRegion);
delegate_noop!(Wl: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

/// Aparência do aviso tirada do `aparencia.conf`: cor de destaque, tema
/// claro e distância da borda de baixo (acima da dock, se ela está embaixo).
struct Look {
    accent: slint::Color,
    light: bool,
    margin: i32,
}

/// Lê o `aparencia.conf` (só as chaves que o aviso usa).
fn look() -> Look {
    let text = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .and_then(|base| std::fs::read_to_string(base.join("kalango/aparencia.conf")).ok())
        .unwrap_or_default();
    look_from(&text)
}

fn look_from(text: &str) -> Look {
    let get = |key: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(key).and_then(|r| r.strip_prefix('=')))
            .map(str::trim)
    };
    let accent_index = get("destaque").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0).min(5);
    let accent = crate::settings::appearance::ACCENTS[accent_index];
    let hex = u32::from_str_radix(accent.trim_start_matches('#'), 16).unwrap_or(0x3dd6f5);
    let light = get("tema") == Some("claro");
    let accent = if light {
        slint::Color::from_argb_encoded(0xff00_0000 | hex).darker(0.45)
    } else {
        slint::Color::from_argb_encoded(0xff00_0000 | hex)
    };
    // Dock embaixo e sempre visível: o aviso fica acima dela (como a do
    // Ubuntu); escondida ou nas laterais, perto da borda.
    let bottom = get("dock-posicao").is_none_or(|v| v == "embaixo");
    let visible = get("dock-sempre-visivel") != Some("false");
    let item = get("dock-tamanho").and_then(|v| v.parse::<i32>().ok()).unwrap_or(52).clamp(16, 72);
    let bar = get("dock-estilo") == Some("barra");
    let gap = if get("dock-solta") == Some("true") && !bar { crate::settings::appearance::DOCK_GAP as i32 } else { 0 };
    let dock = if bottom && visible { item + if bar { 12 } else { 18 } + gap } else { 0 };
    Look { accent, light, margin: dock + 28 }
}

/// O aviso na tela: camada, buffers e a janela de software do Slint.
struct Osd {
    conn: Connection,
    queue: EventQueue<Wl>,
    wl: Wl,
    window: Rc<MinimalSoftwareWindow>,
    view: OsdView,
    /// Arquivo de memória compartilhada (dois buffers, alternados).
    file: File,
    buffers: [wl_buffer::WlBuffer; 2],
    next: usize,
    shown: Option<(wl_surface::WlSurface, zwlr_layer_surface_v1::ZwlrLayerSurfaceV1)>,
    pixels: Vec<PremultipliedRgbaColor>,
}

impl Osd {
    fn new() -> Option<Osd> {
        let conn = Connection::connect_to_env().ok()?;
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut wl = Wl::default();
        queue.roundtrip(&mut wl).ok()?;
        let shm = wl.shm.clone()?;
        wl.compositor.as_ref()?;
        wl.layer_shell.as_ref()?;

        // Memória compartilhada: um arquivo apagado logo depois de aberto.
        let dir = PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?);
        let path = dir.join(format!("kalango-osd-{}.shm", std::process::id()));
        let file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path).ok()?;
        let _ = std::fs::remove_file(&path);
        let size = (W * H * 4) as i32;
        file.set_len(2 * size as u64).ok()?;
        let pool = shm.create_pool(file.as_fd(), 2 * size, &qh, ());
        let buffers = [0, 1].map(|i| {
            pool.create_buffer(i * size, W as i32, H as i32, (W * 4) as i32, wl_shm::Format::Argb8888, &qh, ())
        });
        pool.destroy();

        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Platform(window.clone()))).ok()?;
        window.set_size(slint::PhysicalSize::new(W, H));
        let view = OsdView::new().ok()?;
        view.show().ok()?;
        Some(Osd {
            conn,
            queue,
            wl,
            window,
            view,
            file,
            buffers,
            next: 0,
            shown: None,
            pixels: vec![PremultipliedRgbaColor::default(); (W * H) as usize],
        })
    }

    /// Mostra (ou atualiza) o aviso.
    fn show(&mut self, req: Request) -> bool {
        let look = look();
        self.view.set_kind(req.kind);
        self.view.set_level(req.level);
        self.view.set_muted(req.muted);
        self.view.set_accent(look.accent);
        self.view.set_light(look.light);
        slint::platform::update_timers_and_animations();
        let pixels = &mut self.pixels;
        self.window.request_redraw();
        self.window.draw_if_needed(|renderer| {
            renderer.render(pixels, W as usize);
        });
        // ARGB8888 em little-endian = bytes B, G, R, A (cores já
        // multiplicadas pelo alfa, como o Wayland espera).
        let bytes: Vec<u8> = self.pixels.iter().flat_map(|p| [p.blue, p.green, p.red, p.alpha]).collect();
        let offset = (self.next as u32 * W * H * 4) as u64;
        if self.file.write_all_at(&bytes, offset).is_err() {
            return false;
        }

        let qh = self.queue.handle();
        if self.shown.is_none() {
            let (Some(compositor), Some(layer_shell)) = (self.wl.compositor.clone(), self.wl.layer_shell.clone()) else {
                return false;
            };
            let surface = compositor.create_surface(&qh, ());
            // Sem área de clique: os cliques passam para o que está embaixo.
            let region = compositor.create_region(&qh, ());
            surface.set_input_region(Some(&region));
            region.destroy();
            let layer = layer_shell.get_layer_surface(
                &surface,
                None,
                zwlr_layer_shell_v1::Layer::Overlay,
                "kalango-osd".into(),
                &qh,
                (),
            );
            layer.set_size(W, H);
            layer.set_anchor(zwlr_layer_surface_v1::Anchor::Bottom);
            layer.set_margin(0, 0, look.margin, 0);
            layer.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
            layer.set_exclusive_zone(-1);
            self.wl.configured = false;
            self.wl.closed = false;
            surface.commit();
            while !self.wl.configured && !self.wl.closed {
                if self.queue.blocking_dispatch(&mut self.wl).is_err() {
                    return false;
                }
            }
            self.shown = Some((surface, layer));
        }
        let Some((surface, _)) = &self.shown else { return false };
        surface.attach(Some(&self.buffers[self.next]), 0, 0);
        surface.damage_buffer(0, 0, W as i32, H as i32);
        surface.commit();
        self.next = 1 - self.next;
        self.flush()
    }

    /// Tira o aviso da tela.
    fn hide(&mut self) -> bool {
        if let Some((surface, layer)) = self.shown.take() {
            layer.destroy();
            surface.destroy();
        }
        self.flush()
    }

    /// Envia o que está pendente e lê os eventos que chegaram (liberação de
    /// buffer), sem esperar.
    fn flush(&mut self) -> bool {
        if self.conn.flush().is_err() {
            return false;
        }
        if let Some(guard) = self.conn.prepare_read() {
            let _ = guard.read();
        }
        self.queue.dispatch_pending(&mut self.wl).is_ok() && !self.wl.closed
    }
}

/// Roda `--osd=PEDIDO`: manda ao processo aberto ou vira ele.
pub fn run(text: &str) {
    if text == SERVER {
        serve();
        return;
    }
    if send(text) {
        return;
    }
    // Sem o processo do aviso: abre um, separado, e manda o pedido a ele.
    // Este processo sai logo: quem chamou (o kalango-midia, que segura a
    // trava das teclas) não pode ficar esperando o aviso, que fica aberto a
    // sessão toda. Antes o primeiro aperto virava o aviso e o script ficava
    // preso esperando por ele, com a trava: as teclas paravam de responder.
    if spawn_server() {
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(20));
            if send(text) {
                return;
            }
        }
    }
}

/// Argumento do processo do aviso (`--osd=servidor`).
const SERVER: &str = "servidor";

/// Abre o processo do aviso em segundo plano, num grupo de processos
/// próprio e sem a entrada/saída de quem chamou.
fn spawn_server() -> bool {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe().unwrap_or_else(|_| "kalango-shell".into());
    std::process::Command::new(exe)
        .arg(format!("--osd={SERVER}"))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .process_group(0)
        .spawn()
        .is_ok()
}

/// O processo do aviso: escuta os pedidos e mostra/esconde a camada.
fn serve() {
    // Outro já está aberto (dois apertos ao mesmo tempo): sai.
    if socket_path().is_some_and(|p| UnixStream::connect(p).is_ok()) {
        return;
    }
    let Some(path) = socket_path() else { return };
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(err) => {
            eprintln!("kalango-shell: aviso de volume: {}: {err}", path.display());
            return;
        }
    };
    let Some(mut osd) = Osd::new() else {
        eprintln!("kalango-shell: aviso de volume: sem Wayland/layer-shell");
        return;
    };
    let (tx, rx) = mpsc::channel::<Request>();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut text = String::new();
            let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
            let _ = stream.read_to_string(&mut text);
            if let Some(req) = Request::parse(&text) {
                if tx.send(req).is_err() {
                    break;
                }
            }
        }
    });
    let mut hide_at: Option<Instant> = None;
    loop {
        let wait = hide_at.map_or(Duration::from_secs(60), |t| t.saturating_duration_since(Instant::now()));
        match rx.recv_timeout(wait) {
            Ok(req) => {
                if !osd.show(req) {
                    break;
                }
                hide_at = Some(Instant::now() + VISIBLE);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if hide_at.take().is_some() && !osd.hide() {
                    break;
                }
                // Atualização instalada: sai, e o próximo aperto abre o novo.
                if crate::shell::update::exe_replaced() {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = std::fs::remove_file(&path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_os_pedidos() {
        assert_eq!(Request::parse("volume:45"), Some(Request { kind: 0, level: 45, muted: false }));
        assert_eq!(Request::parse("volume:mudo:45"), Some(Request { kind: 1, level: 45, muted: true }));
        assert_eq!(Request::parse("microfone:mudo:0"), Some(Request { kind: 3, level: 0, muted: true }));
        assert_eq!(Request::parse("brilho:180"), Some(Request { kind: 4, level: 100, muted: false }));
        assert_eq!(Request::parse("volume:"), None);
        assert_eq!(Request::parse("rede:3"), None);
    }

    #[test]
    fn acima_da_dock() {
        assert_eq!(look_from("").margin, 52 + 18 + 28);
        assert_eq!(look_from("dock-tamanho=60\ndock-solta=true\n").margin, 60 + 18 + 8 + 28);
        assert_eq!(look_from("dock-posicao=esquerda\n").margin, 28);
        assert_eq!(look_from("dock-sempre-visivel=false\n").margin, 28);
        assert!(look_from("tema=claro\n").light);
    }
}
