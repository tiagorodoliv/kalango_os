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
use slint::{Model, ComponentHandle, PhysicalSize, Rgb8Pixel};

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

/// Renderiza a janela no estado atual (no tamanho atual dela) e grava em `path`.
fn save(window: &MinimalSoftwareWindow, path: &Path) {
    let size = window.size();
    let (w, h) = (size.width.max((WIDTH as f32 * SCALE) as u32), size.height.max((HEIGHT as f32 * SCALE) as u32));
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

    // Painel rápido com todas as partes (volume, brilho, Wi-Fi ligado).
    let device = ui.global::<kalango_os::Device>();
    device.set_sound_available(true);
    device.set_volume(0.65);
    device.set_brightness_available(true);
    device.set_brightness(0.8);
    device.set_wifi_present(true);
    device.set_network_ready(true);
    device.set_wifi_enabled(true);
    device.set_wifi_name("Casa".into());
    device.set_bt_available(true);
    device.set_bt_powered(true);
    device.set_bt_connected("Fone JBL".into());
    ui.global::<kalango_os::Shell>().set_battery("78%".into());
    ui.set_quick_open(true);
    save(&window, &out.join("painel_rapido.png"));
    ui.set_quick_open(false);

    // Painel rápido com a lista de redes e o campo de senha aberto.
    let nets: Vec<kalango_os::WifiNetwork> = [("Casa", 92, true, true), ("Vizinho 5G", 70, true, false),
        ("Cafe Aberto", 55, false, false), ("Escritorio", 31, true, false)]
        .into_iter()
        .map(|(ssid, signal, secure, active)| kalango_os::WifiNetwork { ssid: ssid.into(), signal, secure, active, saved: active })
        .collect();
    device.set_networks(slint::ModelRc::new(slint::VecModel::from(nets)));
    ui.set_quick_wifi_open(true);
    ui.set_quick_wifi_selected("Vizinho 5G".into());
    ui.set_quick_open(true);
    save(&window, &out.join("painel_rapido_wifi.png"));
    ui.set_quick_open(false);
    ui.set_quick_wifi_open(false);

    ui.set_screen(Screen::Settings);
    save(&window, &out.join("config_aparencia.png"));

    // Temas do terminal (da pasta da distro), no fim da Aparência.
    {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("distro/config/includes.chroot/usr/share/kalango/terminal");
        let themes = kalango_os::settings::terminal::to_ui(&kalango_os::settings::terminal::load_from(&dir));
        ui.global::<kalango_os::Shell>().set_terminal_themes(slint::ModelRc::new(slint::VecModel::from(themes)));
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (1700.0 * SCALE) as u32));
        save(&window, &out.join("config_aparencia_terminal.png"));
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (HEIGHT as f32 * SCALE) as u32));
    }

    // Configurações em meia tela de um notebook 1366×768 (janela estreita:
    // barra lateral só com ícones, cards menores).
    window.set_size(PhysicalSize::new((683.0 * SCALE) as u32, (645.0 * SCALE) as u32));
    save(&window, &out.join("config_estreita.png"));
    ui.set_settings_page(0);
    save(&window, &out.join("config_estreita_rede.png"));
    ui.set_settings_page(3);
    save(&window, &out.join("config_estreita_barra.png"));
    // Barra de tarefas inteira (modelos da dock e as opções novas).
    window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (1500.0 * SCALE) as u32));
    save(&window, &out.join("config_barra.png"));
    ui.set_settings_page(2);
    window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (HEIGHT as f32 * SCALE) as u32));

    // Rede: conexões, IP fixo e VPN (com o editor de IP e o formulário do
    // OpenVPN abertos). Janela alta para caber tudo.
    {
        let d = ui.global::<kalango_os::Device>();
        d.set_network_available(true);
        d.set_network_status("Conectado ao Wi-Fi Casa".into());
        d.set_wifi_enabled(false);
        let conns = vec![
            kalango_os::NetConnection { name: "Casa".into(), kind: "wifi".into(), active: true },
            kalango_os::NetConnection { name: "Wired connection 1".into(), kind: "ethernet".into(), active: false },
        ];
        d.set_connections(slint::ModelRc::new(slint::VecModel::from(conns)));
        let vpns = vec![
            kalango_os::VpnConnection { name: "Trabalho".into(), kind: "OpenVPN".into(), active: true },
            kalango_os::VpnConnection { name: "casa-wg".into(), kind: "WireGuard".into(), active: false },
        ];
        d.set_vpns(slint::ModelRc::new(slint::VecModel::from(vpns)));
        d.set_vpn_available(true);
        d.set_openvpn_available(true);
        d.set_ip_for("Casa".into());
        d.set_ip_manual(true);
        d.set_ip_address("192.168.1.50".into());
        d.set_ip_mask("255.255.255.0".into());
        d.set_ip_gateway("192.168.1.1".into());
        d.set_ip_dns("8.8.8.8, 1.1.1.1".into());
        // Painel "Detalhes" de um Wi-Fi em uso (IP automático).
        let rows: Vec<kalango_os::InfoItem> = [
            ("Signal", "Good (68%)"),
            ("Link speed", "270 Mb/s"),
            ("Security", "WPA2 / WPA3"),
            ("Band", "5 GHz"),
            ("IPv4 address", "192.168.1.50"),
            ("Router", "192.168.1.1"),
            ("DNS", "192.168.1.1"),
            ("Hardware address", "AA:BB:CC:DD:EE:FF"),
        ]
        .into_iter()
        .map(|(l, v)| kalango_os::InfoItem { label: l.into(), value: v.into() })
        .collect();
        d.set_conn_details(slint::ModelRc::new(slint::VecModel::from(rows)));
        d.set_conn_kind("wifi".into());
        d.set_conn_active(true);
        d.set_conn_secured(true);
        d.set_ip_manual(false);
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (1100.0 * SCALE) as u32));
        ui.set_settings_page(0);
        save(&window, &out.join("config_rede_detalhes.png"));
        d.set_ip_manual(true);
        d.set_vpn_form(1);
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (1500.0 * SCALE) as u32));
        ui.set_settings_page(0);
        save(&window, &out.join("config_rede_vpn.png"));
        d.set_vpn_form(2);
        d.set_ip_for("".into());
        d.set_wg_public_key("xTIBA5rboUvnH4htodjb6e697QjLERt1NAB4mZqp8Dg=".into());
        save(&window, &out.join("config_rede_wireguard.png"));
        d.set_vpn_form(0);
        d.set_network_available(false);
        ui.set_settings_page(2);
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (HEIGHT as f32 * SCALE) as u32));
    }

    // Dock em janela própria (faixa de 1366×70, encostada embaixo).
    {
        let shell = ui.global::<kalango_os::Shell>();
        shell.set_dock_strip_width(1366.0);
        shell.set_dock_strip_height(70.0);
        ui.set_mode(kalango_os::ShellMode::Dock);
        window.set_size(PhysicalSize::new((1366.0 * SCALE) as u32, (70.0 * SCALE) as u32));
        save(&window, &out.join("dock_janela.png"));
        // Modelos da dock: ícones 3D, ladrilhos e cantos retos/pílula.
        let theme = ui.global::<Theme>();
        for (name, icons, corners, transp, icon_transp) in [
            ("dock_3d_pilula", 1, 2, 0.4, 0.0),
            ("dock_ladrilhos", 2, 1, 0.2, 0.0),
            ("dock_minima", 0, 0, 0.6, 0.25),
        ] {
            theme.set_dock_icon_style(icons);
            theme.set_dock_corners(corners);
            theme.set_dock_transparency(transp);
            theme.set_dock_icon_transparency(icon_transp);
            save(&window, &out.join(format!("{name}.png")));
        }
        theme.set_dock_icon_style(0);
        theme.set_dock_corners(1);
        theme.set_dock_transparency(0.0);
        theme.set_dock_icon_transparency(0.0);
        ui.set_mode(kalango_os::ShellMode::Desktop);
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (HEIGHT as f32 * SCALE) as u32));
    }

    // Monitores: arranjo com dois monitores (notebook + monitor largo).
    {
        let d = ui.global::<kalango_os::Device>();
        let empty_res: Vec<kalango_os::DisplayResolution> = Vec::new();
        let empty_lbl: Vec<slint::SharedString> = Vec::new();
        let mk = |name: &str, label: &str, x: i32, w: i32, h: i32, n: i32, primary: bool| kalango_os::DisplayOutput {
            name: name.into(),
            label: label.into(),
            enabled: true,
            resolutions: slint::ModelRc::new(slint::VecModel::from(empty_res.clone())),
            resolution_labels: slint::ModelRc::new(slint::VecModel::from(empty_lbl.clone())),
            resolution: 0,
            rate: 0,
            scale: 2,
            rotation: 0,
            x,
            y: 0,
            width_px: w,
            height_px: h,
            number: n,
            primary,
        };
        let outs = vec![
            mk("eDP-1", "BOE 0x07E2 (eDP-1)", 0, 1366, 768, 1, true),
            mk("HDMI-A-1", "LG Electronics 25UM58G (HDMI-A-1)", 1366, 2560, 1080, 2, false),
        ];
        d.set_displays(slint::ModelRc::new(slint::VecModel::from(outs)));
        d.set_displays_available(true);
        d.set_display_bounds_x(-1280);
        d.set_display_bounds_y(-540);
        d.set_display_bounds_w(3926 + 2560);
        d.set_display_bounds_h(1080 + 1080);
        d.set_display_arranged(true);
        ui.set_settings_page(4);
        save(&window, &out.join("config_monitores.png"));
        ui.set_settings_page(2);
    }

    // Energia: modo de energia, apagar/suspender, tampa e botão.
    {
        let pw = ui.global::<kalango_os::Power>();
        pw.set_profiles_available(true);
        pw.set_has_performance(true);
        pw.set_has_battery(true);
        pw.set_has_lid(true);
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (1150.0 * SCALE) as u32));
        ui.set_settings_page(6);
        save(&window, &out.join("config_energia.png"));
        ui.set_settings_page(2);
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (HEIGHT as f32 * SCALE) as u32));
    }

    // Usuários: a própria conta, outras pessoas e os formulários.
    {
        let a = ui.global::<kalango_os::Accounts>();
        let acc = |login: &str, name: &str, admin: bool, me: bool, signed_in: bool| kalango_os::UserAccount {
            login: login.into(),
            name: name.into(),
            initial: name[..1].into(),
            admin,
            me,
            signed_in,
        };
        a.set_me(acc("ana", "Ana Souza", true, true, true));
        a.set_others(slint::ModelRc::from(Rc::new(slint::VecModel::from(vec![
            acc("maria", "Maria Silva", false, false, false),
            acc("joao", "João Souza", true, false, true),
        ]))));
        a.set_can_manage(true);
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (1100.0 * SCALE) as u32));
        ui.set_settings_page(7);
        save(&window, &out.join("config_usuarios.png"));
        a.set_form("add".into());
        save(&window, &out.join("config_usuarios_novo.png"));
        a.set_form("password".into());
        save(&window, &out.join("config_usuarios_senha.png"));
        a.set_form("".into());
        ui.set_settings_page(2);
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (HEIGHT as f32 * SCALE) as u32));
    }

    // Dock com um app aberto que não está fixado (VS Code), Alt+Tab e o
    // menu do botão direito da dock.
    {
        let shell = ui.global::<kalango_os::Shell>();
        let apps = ui.global::<kalango_os::Apps>();
        let pinned: Vec<kalango_os::AppEntry> = apps.get_pinned().iter().collect();
        let vscode = kalango_os::AppEntry {
            id: "desktop:/usr/share/applications/com.microsoft.VSCode.desktop".into(),
            name: "Visual Studio Code".into(),
            letter: "V".into(),
            tint: slint::Color::from_rgb_u8(0x3d, 0x9b, 0xf5),
            ..Default::default()
        };
        shell.set_native(true);
        shell.set_running_ids(format!("files,terminal,{}", vscode.id).into());
        shell.set_active_app(vscode.id.clone());
        shell.set_running_extra(slint::ModelRc::new(slint::VecModel::from(vec![vscode.clone()])));
        ui.set_screen(Screen::Desktop);
        save(&window, &out.join("dock_apps_abertos.png"));

        let item = |app: &kalango_os::AppEntry, title: &str| kalango_os::SwitcherItem {
            app: app.clone(),
            title: title.into(),
            minimized: false,
        };
        let items = vec![
            item(&vscode, "main.rs — kalango_os — Visual Studio Code"),
            item(&pinned[1], "tiago@hp: ~/git/kalango_os"),
            item(&pinned[0], "Projetos — Arquivos"),
            item(&pinned[2], "KalangOS — Mozilla Firefox"),
        ];
        let sw = ui.global::<kalango_os::Switcher>();
        sw.set_items(slint::ModelRc::new(slint::VecModel::from(items)));
        sw.set_selected(1);
        ui.set_mode(kalango_os::ShellMode::Switcher);
        save(&window, &out.join("alt_tab.png"));

        let menu = ui.global::<kalango_os::DockMenu>();
        menu.set_app(vscode.clone());
        menu.set_pinned(false);
        menu.set_total(6);
        menu.set_offset(5.0 * 44.0 + 9.0 + 18.0);
        let wins = vec![
            kalango_os::DockMenuWindow { id: 1, title: "main.rs — kalango_os — Visual Studio Code".into() },
            kalango_os::DockMenuWindow { id: 2, title: "README.md — notas — Visual Studio Code".into() },
        ];
        menu.set_windows(slint::ModelRc::new(slint::VecModel::from(wins)));
        ui.set_mode(kalango_os::ShellMode::DockMenu);
        save(&window, &out.join("dock_menu.png"));
        ui.set_mode(kalango_os::ShellMode::Desktop);
        shell.set_native(false);
        shell.set_running_extra(slint::ModelRc::default());
    }

    // Calculadora (--calculadora): as três abas, com a lógica de verdade.
    {
        kalango_os::tools::calculator::install(&ui);
        let calc = ui.global::<kalango_os::Calc>();
        ui.set_mode(kalango_os::ShellMode::Calculator);
        window.set_size(PhysicalSize::new((900.0 * SCALE) as u32, (640.0 * SCALE) as u32));
        calc.invoke_clear_history();
        for e in ["1/3", "2^10 + sqrt(9) * 3"] {
            calc.set_expr(e.into());
            calc.invoke_commit();
        }
        calc.set_expr("200 * 15% + ans".into());
        calc.invoke_changed();
        calc.set_tab(0);
        save(&window, &out.join("calculadora.png"));
        calc.set_prog_expr("0xFF & 0b1010 << 2 | 1 << 13".into());
        calc.set_bits(16);
        calc.invoke_prog_changed();
        calc.set_tab(1);
        save(&window, &out.join("calculadora_programador.png"));
        calc.set_tab(2);
        calc.set_tool(6);
        calc.set_tool_input(r#"{"nome":"KalangOS","versao":[0,1,1],"ok":true}"#.into());
        calc.invoke_run_tool(0);
        save(&window, &out.join("calculadora_json.png"));
        calc.set_tool(4);
        calc.set_tool_input("#3dd6f5".into());
        calc.invoke_run_tool(0);
        save(&window, &out.join("calculadora_cores.png"));
        calc.invoke_clear_history();
        ui.set_mode(kalango_os::ShellMode::Desktop);
        window.set_size(PhysicalSize::new((WIDTH as f32 * SCALE) as u32, (HEIGHT as f32 * SCALE) as u32));
    }

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
