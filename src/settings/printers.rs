//! Configurações → Impressoras: impressoras de rede e USB pelo CUPS.
//!
//! - **Lista**: `lpstat -l -p` (estado e descrição), `-v` (endereço: rede ou
//!   USB), `-d` (padrão) e `-o` (trabalhos na fila). Relida a cada 3 s com a
//!   página aberta. As impressoras de rede e as USB modernas costumam aparecer
//!   sozinhas (`cups-browsed` e `ipp-usb`, sem driver: IPP Everywhere).
//! - **Adicionar**: `lpinfo -l -v` lista os aparelhos (USB e rede); IPP usa o
//!   modelo `everywhere` (sem driver), USB antiga usa o primeiro driver de
//!   `lpinfo -m --device-id`. Também pelo endereço (IP ou nome): IPP
//!   Everywhere em `ipp://<endereço>/ipp/print`.
//! - **Permissão**: quem está no grupo `lpadmin` (administradores) fala direto
//!   com o CUPS; os outros passam pelo
//!   `pkexec /usr/local/sbin/kalango-impressoras` (senha do administrador).
//! - Opções que a página não tem (drivers à mão, bandejas): `system-config-printer`.

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};

use crate::common::i18n::tr;
use crate::{AppWindow, Mode, Printer, PrinterDevice, Printers};

/// Script de root (via pkexec) para quem não está no grupo lpadmin.
pub const HELPER: &str = "/usr/local/sbin/kalango-impressoras";
/// Índice da página nas Configurações (`ui/screens/settings.slint`).
pub const PAGE: i32 = 12;

/// Uma impressora lida do CUPS.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Queue {
    pub name: String,
    pub info: String,
    /// "idle", "printing" ou "disabled".
    pub state: String,
    pub uri: String,
    pub is_default: bool,
    pub jobs: i32,
}

/// Um aparelho de `lpinfo -l -v`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Device {
    pub uri: String,
    pub class: String,
    pub info: String,
    pub make_model: String,
    pub device_id: String,
}

/// Lê `lpstat -l -p` (com `LC_ALL=C`): nome, estado e descrição.
pub fn parse_printers(text: &str) -> Vec<Queue> {
    let mut list: Vec<Queue> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("printer ") {
            let mut words = rest.split_whitespace();
            let name = words.next().unwrap_or_default().to_string();
            let state = if rest.contains(" disabled") {
                "disabled"
            } else if rest.contains("now printing") {
                "printing"
            } else {
                "idle"
            };
            list.push(Queue { info: name.replace('_', " "), name, state: state.into(), ..Default::default() });
        } else if let Some(desc) = line.trim().strip_prefix("Description:") {
            if let Some(q) = list.last_mut() {
                // Sem descrição própria, o CUPS repete o nome da fila.
                let desc = desc.trim();
                if !desc.is_empty() && desc != q.name {
                    q.info = desc.to_string();
                }
            }
        }
    }
    list
}

/// Endereços das filas, de `lpstat -v` ("device for NOME: URI").
pub fn parse_uris(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let rest = l.strip_prefix("device for ")?;
            let (name, uri) = rest.split_once(": ")?;
            Some((name.trim().to_string(), uri.trim().to_string()))
        })
        .collect()
}

/// Impressora padrão, de `lpstat -d` ("system default destination: NOME").
pub fn parse_default(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.split_once("default destination:").map(|(_, n)| n.trim().to_string()))
        .filter(|n| !n.is_empty())
}

/// Trabalhos por fila, de `lpstat -o` ("NOME-123 usuário tamanho data").
pub fn parse_jobs(text: &str) -> HashMap<String, i32> {
    let mut jobs = HashMap::new();
    for line in text.lines() {
        let Some(id) = line.split_whitespace().next() else { continue };
        if let Some((name, n)) = id.rsplit_once('-') {
            if n.chars().all(|c| c.is_ascii_digit()) {
                *jobs.entry(name.to_string()).or_insert(0) += 1;
            }
        }
    }
    jobs
}

/// Lê `lpinfo -l -v`: blocos "Device: uri = ..." com class, info,
/// make-and-model e device-id.
pub fn parse_devices(text: &str) -> Vec<Device> {
    let mut list: Vec<Device> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim().to_string());
        match key {
            "Device: uri" => list.push(Device { uri: value, ..Default::default() }),
            _ => {
                let Some(d) = list.last_mut() else { continue };
                match key {
                    "class" => d.class = value,
                    "info" => d.info = value,
                    "make-and-model" => d.make_model = value,
                    "device-id" => d.device_id = value,
                    _ => {}
                }
            }
        }
    }
    list
}

/// Nome do serviço anunciado na rede ("ipps://Impressora%20Teste._ipps._tcp.local/"
/// → "Impressora Teste").
pub fn service_name(uri: &str) -> Option<String> {
    let host = uri.split_once("://")?.1.split('/').next()?;
    let (name, _) = host.split_once("._ipp")?;
    let mut out = String::new();
    let bytes = name.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&name[i + 1..i + 3], 16) {
                out.push(b as char);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    Some(out)
}

/// Aparelhos que dá para adicionar: impressoras de verdade (USB ou rede com
/// endereço), sem as que já são filas (pelo endereço, ou pelo nome do
/// serviço, que o cups-browsed usa nas filas que cria sozinho) e sem repetir
/// a mesma impressora anunciada por IPP e pelo dnssd.
pub fn addable(devices: &[Device], installed: &[String], queues: &[String]) -> Vec<Device> {
    let mut seen: Vec<String> = Vec::new();
    devices
        .iter()
        .filter(|d| d.uri.contains("://") && !installed.iter().any(|u| u == &d.uri))
        .filter(|d| service_name(&d.uri).is_none_or(|s| !queues.contains(&queue_name(&s, &[]))))
        .filter(|d| matches!(d.class.as_str(), "direct" | "network") || d.uri.starts_with("dnssd://"))
        .filter(|d| !d.uri.starts_with("ipp://localhost") || d.class == "network")
        .filter(|d| {
            let key = display_name(d).to_lowercase();
            if seen.contains(&key) {
                return false;
            }
            seen.push(key);
            true
        })
        .cloned()
        .collect()
}

/// Nome para mostrar de um aparelho.
pub fn display_name(d: &Device) -> String {
    [&d.make_model, &d.info]
        .into_iter()
        .find(|s| !s.is_empty() && s.as_str() != "Unknown")
        .cloned()
        .unwrap_or_else(|| d.uri.clone())
}

/// Endereço de rede (e não USB)?
pub fn is_network(uri: &str) -> bool {
    !uri.starts_with("usb://") && !uri.starts_with("hp:/usb") && !uri.contains("://localhost")
}

/// Nome de fila a partir do modelo ("HP DeskJet 2600 series" →
/// "HP_DeskJet_2600_series"), sem repetir um que já existe.
pub fn queue_name(model: &str, taken: &[String]) -> String {
    let mut base: String = model
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if base.is_empty() {
        base = "Impressora".into();
    }
    base.truncate(60);
    if !taken.contains(&base) {
        return base;
    }
    (2..100).map(|n| format!("{base}_{n}")).find(|n| !taken.contains(n)).unwrap_or(base)
}

/// Endereço IPP Everywhere de um IP ou nome digitado ("192.168.0.50",
/// "ipp://..." já pronto).
pub fn address_uri(address: &str) -> Option<String> {
    let a = address.trim();
    if a.is_empty() || a.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'') {
        return None;
    }
    if a.contains("://") {
        return Some(a.to_string());
    }
    Some(format!("ipp://{a}/ipp/print"))
}

/// Primeiro driver de `lpinfo -m` ("MODELO descrição").
pub fn first_model(text: &str) -> Option<String> {
    text.lines().find_map(|l| l.split_whitespace().next()).map(String::from)
}

fn cmd(program: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(program).args(args).env("LC_ALL", "C").stdin(Stdio::null()).output()
}

/// Roda um comando do CUPS que precisa de permissão: direto (grupo lpadmin)
/// ou, recusado, pelo pkexec com o script de root.
fn admin(direct: (&str, &[&str]), helper_args: &[&str]) -> Result<String, String> {
    if let Ok(out) = cmd(direct.0, direct.1) {
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
    }
    let out = Command::new("pkexec")
        .arg(HELPER)
        .args(helper_args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { tr("Not authorized", "Não autorizado").into() } else { err })
    }
}

/// Lê as filas do CUPS (`None` sem CUPS).
fn read_queues() -> Option<Vec<Queue>> {
    let out = cmd("lpstat", &["-l", "-p"]).ok()?;
    let mut list = parse_printers(&String::from_utf8_lossy(&out.stdout));
    if !out.status.success() && list.is_empty() {
        // Sem impressoras o lpstat sai com erro ("No destinations added"):
        // o CUPS está lá se o -r responde.
        let running = cmd("lpstat", &["-r"]).is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("is running"));
        return running.then(Vec::new);
    }
    let text = |args: &[&str]| cmd("lpstat", args).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let uris = parse_uris(&text(&["-v"]));
    let default = parse_default(&text(&["-d"]));
    let jobs = parse_jobs(&text(&["-o"]));
    for q in &mut list {
        q.uri = uris.get(&q.name).cloned().unwrap_or_default();
        q.is_default = default.as_deref() == Some(q.name.as_str());
        q.jobs = jobs.get(&q.name).copied().unwrap_or(0);
    }
    Some(list)
}

fn state_text(q: &Queue) -> String {
    match q.state.as_str() {
        "disabled" => tr("Stopped", "Parada").into(),
        "printing" => tr("Printing", "Imprimindo").into(),
        _ => tr("Ready", "Pronta").into(),
    }
}

/// Mostra as filas na página.
fn show(ui: &AppWindow, queues: Option<Vec<Queue>>) {
    let p = ui.global::<Printers>();
    p.set_available(queues.is_some());
    let list: Vec<Printer> = queues
        .unwrap_or_default()
        .iter()
        .map(|q| Printer {
            name: q.name.clone().into(),
            info: q.info.clone().into(),
            state: state_text(q).into(),
            stopped: q.state == "disabled",
            is_default: q.is_default,
            jobs: q.jobs,
            network: is_network(&q.uri),
        })
        .collect();
    p.set_list(ModelRc::from(Rc::new(VecModel::from(list))));
}

/// Relê em segundo plano e mostra.
fn refresh(weak: slint::Weak<AppWindow>) {
    std::thread::spawn(move || {
        let queues = read_queues();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                show(&ui, queues);
            }
        });
    });
}

/// Roda uma ação em segundo plano, mostra o resultado e relê a lista.
fn run(ui: &AppWindow, success: String, work: impl FnOnce() -> Result<(), String> + Send + 'static) {
    let p = ui.global::<Printers>();
    p.set_busy(true);
    p.set_message("".into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let result = work();
        let queues = read_queues();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let p = ui.global::<Printers>();
            p.set_busy(false);
            match result {
                Ok(()) => {
                    p.set_message_error(false);
                    p.set_message(success.into());
                }
                Err(err) => {
                    p.set_message_error(true);
                    p.set_message(err.into());
                }
            }
            show(&ui, queues);
        });
    });
}

thread_local! {
    /// Aparelhos encontrados (para escolher o driver ao adicionar).
    static FOUND: std::cell::RefCell<Vec<Device>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Procura os aparelhos (alguns segundos) e mostra os que dá para adicionar.
fn search(ui: &AppWindow) {
    let p = ui.global::<Printers>();
    if p.get_searching() {
        return;
    }
    p.set_searching(true);
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let text = admin(("lpinfo", &["-l", "-v", "--timeout", "8"]), &["procurar"]).unwrap_or_default();
        let queues = read_queues().unwrap_or_default();
        let installed: Vec<String> = queues.iter().map(|q| q.uri.clone()).collect();
        let names: Vec<String> = queues.into_iter().map(|q| q.name).collect();
        let found = addable(&parse_devices(&text), &installed, &names);
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let p = ui.global::<Printers>();
            p.set_searching(false);
            let rows: Vec<PrinterDevice> = found
                .iter()
                .map(|d| PrinterDevice {
                    uri: d.uri.clone().into(),
                    name: display_name(d).into(),
                    kind: if is_network(&d.uri) { tr("Network", "Rede").into() } else { "USB".into() },
                })
                .collect();
            p.set_found(ModelRc::from(Rc::new(VecModel::from(rows))));
            FOUND.with_borrow_mut(|f| *f = found);
        });
    });
}

/// Driver para um aparelho: IPP (rede ou USB moderna) = `everywhere`; USB
/// antiga = o primeiro driver instalado para o device-id dela.
fn model_for(device: &Device) -> Result<String, String> {
    let ipp = ["ipp://", "ipps://", "dnssd://"].iter().any(|s| device.uri.starts_with(s));
    if ipp || device.device_id.is_empty() {
        return Ok("everywhere".into());
    }
    let text = admin(("lpinfo", &["-m", "--device-id", &device.device_id]), &["modelos", &device.device_id])?;
    first_model(&text).ok_or_else(|| {
        tr(
            "No driver found for this printer. Use Advanced options to choose one.",
            "Nenhum driver encontrado para essa impressora. Use Opções avançadas para escolher um.",
        )
        .into()
    })
}

/// Cria a fila (lpadmin) e a deixa aceitando trabalhos.
fn add_queue(name: &str, uri: &str, model: &str) -> Result<(), String> {
    admin(("lpadmin", &["-p", name, "-E", "-v", uri, "-m", model]), &["adicionar", name, uri, model])?;
    // Quem passou pelo pkexec já saiu com tudo ligado; direto, liga aqui.
    let _ = cmd("cupsenable", &[name]);
    let _ = cmd("cupsaccept", &[name]);
    Ok(())
}

/// Liga a página Impressoras (Configurações, sessão real).
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if !(crate::shell::session::is_native() && mode == Mode::Settings) {
        return None;
    }
    let p = ui.global::<Printers>();
    refresh(ui.as_weak());

    let weak = ui.as_weak();
    p.on_search(move || {
        if let Some(ui) = weak.upgrade() {
            search(&ui);
        }
    });

    let weak = ui.as_weak();
    p.on_add(move |uri, _name| {
        let Some(ui) = weak.upgrade() else { return };
        let device = FOUND
            .with_borrow(|f| f.iter().find(|d| d.uri == uri.as_str()).cloned())
            .unwrap_or_else(|| Device { uri: uri.to_string(), ..Default::default() });
        let label = display_name(&device);
        let taken: Vec<String> = ui.global::<Printers>().get_list().iter().map(|q| q.name.to_string()).collect();
        let name = queue_name(&label, &taken);
        ui.global::<Printers>().set_adding(false);
        run(&ui, format!("{} {label}", tr("Printer added:", "Impressora adicionada:")), move || {
            let model = model_for(&device)?;
            add_queue(&name, &device.uri, &model)
        });
    });

    let weak = ui.as_weak();
    p.on_add_address(move |address| {
        let Some(ui) = weak.upgrade() else { return };
        let Some(uri) = address_uri(&address) else {
            let p = ui.global::<Printers>();
            p.set_message_error(true);
            p.set_message(tr("Invalid address", "Endereço inválido").into());
            return;
        };
        let taken: Vec<String> = ui.global::<Printers>().get_list().iter().map(|q| q.name.to_string()).collect();
        let name = queue_name(&format!("Impressora_{}", address.trim()), &taken);
        ui.global::<Printers>().set_adding(false);
        run(&ui, tr("Printer added.", "Impressora adicionada.").into(), move || {
            add_queue(&name, &uri, "everywhere").map_err(|_| {
                tr(
                    "The printer did not answer at this address (IPP). Check the address or use Advanced options.",
                    "A impressora não respondeu nesse endereço (IPP). Confira o endereço ou use Opções avançadas.",
                )
                .into()
            })
        });
    });

    let weak = ui.as_weak();
    p.on_set_default(move |name| {
        let Some(ui) = weak.upgrade() else { return };
        let name = name.to_string();
        // Padrão de quem está usando (lpoptions, sem senha).
        run(&ui, tr("Default printer changed.", "Impressora padrão alterada.").into(), move || {
            let out = cmd("lpoptions", &["-d", &name]).map_err(|e| e.to_string())?;
            out.status.success().then_some(()).ok_or_else(|| String::from_utf8_lossy(&out.stderr).trim().to_string())
        });
    });

    let weak = ui.as_weak();
    p.on_test_page(move |name| {
        let Some(ui) = weak.upgrade() else { return };
        let name = name.to_string();
        run(&ui, tr("Test page sent.", "Página de teste enviada.").into(), move || {
            let page = ["/usr/share/cups/data/testprint", "/usr/share/cups/data/default-testpage.pdf"]
                .into_iter()
                .find(|p| std::path::Path::new(p).exists())
                .ok_or("sem página de teste do CUPS")?;
            let out = cmd("lp", &["-d", &name, "-t", tr("Test page", "Página de teste"), page]).map_err(|e| e.to_string())?;
            out.status.success().then_some(()).ok_or_else(|| String::from_utf8_lossy(&out.stderr).trim().to_string())
        });
    });

    let weak = ui.as_weak();
    p.on_cancel_jobs(move |name| {
        let Some(ui) = weak.upgrade() else { return };
        let name = name.to_string();
        run(&ui, tr("Jobs canceled.", "Trabalhos cancelados.").into(), move || {
            let out = cmd("cancel", &["-a", &name]).map_err(|e| e.to_string())?;
            out.status.success().then_some(()).ok_or_else(|| String::from_utf8_lossy(&out.stderr).trim().to_string())
        });
    });

    let weak = ui.as_weak();
    p.on_remove(move |name| {
        let Some(ui) = weak.upgrade() else { return };
        let name = name.to_string();
        run(&ui, tr("Printer removed.", "Impressora removida.").into(), move || {
            admin(("lpadmin", &["-x", &name]), &["remover", &name]).map(|_| ())
        });
    });

    p.on_advanced(|| {
        crate::settings::device::spawn_detached("system-config-printer", &[]);
    });

    // Relê enquanto a página está aberta (estado, fila, impressoras que o
    // cups-browsed achou na rede).
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(3), move || {
        if let Some(ui) = weak.upgrade().filter(|ui| ui.get_settings_page() == PAGE) {
            if !ui.global::<Printers>().get_busy() {
                refresh(ui.as_weak());
            }
        }
    });
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filas_do_cups() {
        let p = "printer HP_DeskJet is idle.  enabled since Sat Nov 14 10:00:00 2026\n\
                 \tForm mounted:\n\tDescription: HP DeskJet 2600 series\n\tAlerts: none\n\
                 printer Epson_L3150 now printing Epson_L3150-7.  enabled since Sat Nov 14 10:00:00 2026\n\
                 \tDescription: \n\
                 printer Velha disabled since Sat Nov 14 10:00:00 2026 -\n\tPaused\n";
        let q = parse_printers(p);
        assert_eq!(q.len(), 3);
        assert_eq!((q[0].name.as_str(), q[0].info.as_str(), q[0].state.as_str()), ("HP_DeskJet", "HP DeskJet 2600 series", "idle"));
        assert_eq!((q[1].info.as_str(), q[1].state.as_str()), ("Epson L3150", "printing"));
        assert_eq!(q[2].state, "disabled");
        let uris = parse_uris("device for HP_DeskJet: usb://HP/DeskJet%202600?serial=X\ndevice for Epson_L3150: ipp://192.168.0.9/ipp/print\n");
        assert!(uris["HP_DeskJet"].starts_with("usb://"));
        assert_eq!(parse_default("system default destination: Epson_L3150\n").as_deref(), Some("Epson_L3150"));
        assert_eq!(parse_default("no system default destination\n"), None);
        let jobs = parse_jobs("Epson_L3150-7 tiago 1024 Sat\nEpson_L3150-8 tiago 1024 Sat\nHP_DeskJet-2 ana 10 Sat\n");
        assert_eq!((jobs["Epson_L3150"], jobs["HP_DeskJet"]), (2, 1));
    }

    #[test]
    fn aparelhos_para_adicionar() {
        let text = "Device: uri = socket\n        class = network\n        info = AppSocket/HP JetDirect\n\
                    Device: uri = usb://HP/DeskJet%202600%20series?serial=CN123\n        class = direct\n        info = HP DeskJet 2600 series\n        make-and-model = HP DeskJet 2600 series\n        device-id = MFG:HP;MDL:DeskJet 2600 series;\n\
                    Device: uri = ipp://EPSON.local:631/ipp/print\n        class = network\n        info = EPSON L3250\n        make-and-model = EPSON L3250 Series\n\
                    Device: uri = dnssd://EPSON%20L3250._ipp._tcp.local/?uuid=1\n        class = network\n        info = EPSON L3250\n        make-and-model = EPSON L3250 Series\n\
                    Device: uri = ipp://192.168.0.9/ipp/print\n        class = network\n        info = Já instalada\n";
        let d = parse_devices(text);
        assert_eq!(d.len(), 5);
        assert_eq!(d[1].device_id, "MFG:HP;MDL:DeskJet 2600 series;");
        let a = addable(&d, &["ipp://192.168.0.9/ipp/print".into()], &[]);
        let names: Vec<String> = a.iter().map(display_name).collect();
        assert_eq!(names, ["HP DeskJet 2600 series", "EPSON L3250 Series"]);
        assert!(!is_network(&a[0].uri) && is_network(&a[1].uri));
        // Já adicionada sozinha pelo cups-browsed (fila com o nome do serviço).
        assert!(addable(&d, &[], &["EPSON_L3250".into()]).iter().all(|x| !x.uri.starts_with("dnssd://")));
        assert_eq!(service_name("ipps://Impressora%20Teste._ipps._tcp.local/").as_deref(), Some("Impressora Teste"));
        assert_eq!(service_name("usb://HP/X"), None);
    }

    #[test]
    fn nomes_e_enderecos() {
        assert_eq!(queue_name("HP DeskJet 2600 series", &[]), "HP_DeskJet_2600_series");
        assert_eq!(queue_name("EPSON L3250 (rede)", &["EPSON_L3250_rede".into()]), "EPSON_L3250_rede_2");
        assert_eq!(queue_name("", &[]), "Impressora");
        assert_eq!(address_uri(" 192.168.0.50 ").as_deref(), Some("ipp://192.168.0.50/ipp/print"));
        assert_eq!(address_uri("ipps://imp.local/ipp/print").as_deref(), Some("ipps://imp.local/ipp/print"));
        assert_eq!(address_uri("a b"), None);
        assert_eq!(first_model("drv:///hpcups.drv/hp-deskjet_2600.ppd HP DeskJet 2600, hpcups\n").as_deref(), Some("drv:///hpcups.drv/hp-deskjet_2600.ppd"));
    }
}
