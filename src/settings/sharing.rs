//! Configurações → Compartilhamento: pastas na rede local e computadores da rede.
//!
//! **Compartilhar** usa o Samba (o Windows, o macOS e outros Linux abrem as
//! pastas por SMB) e o `wsdd2` (o computador aparece na "Rede" do Windows).
//! Tudo o que precisa de root passa pelo `/usr/local/sbin/kalango-compartilhar`
//! via `pkexec` (política `org.kalango.compartilhar`, senha do administrador
//! lembrada por alguns minutos): ligar/desligar os serviços e gravar as
//! pastas em `/etc/samba/kalango.conf`. A escolha (pastas e acesso sem senha)
//! fica em `~/.config/kalango/compartilhamento.conf`.
//!
//! **Computadores da rede**: `avahi-browse` lista quem anuncia pastas SMB
//! (Linux, macOS, NAS); um clique abre no Thunar (`smb://...`, pelo gvfs).
//! Máquinas Windows não se anunciam pelo Avahi: para elas há o campo de
//! endereço e o "Procurar na rede" (`network:///` do Thunar).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel, Weak};

use crate::settings::device::{output, spawn_detached};
use crate::common::i18n::tr;
use crate::{AppWindow, Device, Mode, NetComputer, SharedFolder};

/// Script de root (via pkexec) que mexe no Samba.
pub const HELPER: &str = "/usr/local/sbin/kalango-compartilhar";

/// Pastas oferecidas para compartilhar: chave do `user-dirs.dirs` e nome
/// padrão (inglês) se a chave não existir.
const FOLDERS: [(&str, &str); 6] = [
    ("XDG_PUBLICSHARE_DIR", "Public"),
    ("XDG_DOCUMENTS_DIR", "Documents"),
    ("XDG_PICTURES_DIR", "Pictures"),
    ("XDG_MUSIC_DIR", "Music"),
    ("XDG_VIDEOS_DIR", "Videos"),
    ("XDG_DOWNLOAD_DIR", "Downloads"),
];

/// Escolha da pessoa (`compartilhamento.conf`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
    /// Visitantes entram sem senha (só leitura).
    pub guest: bool,
    /// Caminhos das pastas compartilhadas.
    pub folders: Vec<String>,
}

impl Prefs {
    /// Lê o arquivo; sem ele, compartilha a pasta pública, sem senha.
    pub fn parse(text: Option<&str>, public_dir: &str) -> Prefs {
        let Some(text) = text else {
            return Prefs { guest: true, folders: vec![public_dir.to_string()] };
        };
        let mut p = Prefs { guest: true, folders: Vec::new() };
        for line in text.lines() {
            match line.trim().split_once('=') {
                Some(("sem-senha", v)) => p.guest = v.trim() != "false",
                Some(("pasta", v)) if !v.trim().is_empty() => p.folders.push(v.trim().to_string()),
                _ => {}
            }
        }
        p
    }

    /// Texto do arquivo.
    pub fn to_text(&self) -> String {
        let mut text = format!("# Configurações → Compartilhamento\nsem-senha={}\n", self.guest);
        for f in &self.folders {
            text.push_str(&format!("pasta={f}\n"));
        }
        text
    }
}

/// Lê a saída de `avahi-browse -tpr _smb._tcp`: um computador por nome.
pub fn parse_avahi(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split(';').collect();
        // =;interface;IPv4;nome;_smb._tcp;local;host.local;endereço;porta;txt
        if f.len() < 9 || f[0] != "=" {
            continue;
        }
        let name = unescape_avahi(f[3]);
        let host = if f[6].is_empty() { f[7].to_string() } else { f[6].to_string() };
        if !out.iter().any(|(n, _)| *n == name) {
            out.push((name, host));
        }
    }
    out.sort_by_key(|(n, _)| n.to_lowercase());
    out
}

/// O Avahi escreve caracteres especiais como `\DDD` (decimal): `\032` = espaço.
fn unescape_avahi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1..i + 4].iter().all(u8::is_ascii_digit) {
            if let Ok(n) = s[i + 1..i + 4].parse::<u8>() {
                out.push(n);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Endereço digitado → URL para o Thunar: `192.168.0.10` e `pc.local` viram
/// `smb://.../`; `\\pc\pasta` (jeito do Windows) vira `smb://pc/pasta`; uma
/// URL (`sftp://`, `smb://`...) passa direto.
pub fn server_url(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    if s.contains("://") {
        return Some(s.to_string());
    }
    let path = s.trim_start_matches(['\\', '/']).replace('\\', "/");
    if path.is_empty() {
        return None;
    }
    Some(format!("smb://{path}{}", if path.contains('/') { "" } else { "/" }))
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn prefs_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(&home()).join(".config"));
    base.join("kalango/compartilhamento.conf")
}

/// Pastas oferecidas (as que existem), com o caminho real.
fn candidates() -> Vec<String> {
    let home = home();
    FOLDERS
        .iter()
        .map(|(key, fallback)| crate::shell::session::xdg_dir(&home, key, fallback))
        .filter(|p| Path::new(p).is_dir() && *p != home)
        .fold(Vec::new(), |mut v, p| {
            if !v.contains(&p) {
                v.push(p);
            }
            v
        })
}

fn load_prefs() -> Prefs {
    let public = crate::shell::session::xdg_dir(&home(), FOLDERS[0].0, FOLDERS[0].1);
    Prefs::parse(fs::read_to_string(prefs_path()).ok().as_deref(), &public)
}

fn save_prefs(p: &Prefs) {
    let path = prefs_path();
    let result = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|_| fs::write(&path, p.to_text()));
    if let Err(err) = result {
        eprintln!("kalango-shell: falha ao gravar {}: {err}", path.display());
    }
}

/// O Samba está rodando.
fn samba_active() -> bool {
    Command::new("systemctl")
        .args(["is-active", "--quiet", "smbd.service"])
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Roda o script de root pelo pkexec (a janela de senha é do lxpolkit).
fn helper(args: &[&str]) -> bool {
    Command::new("pkexec")
        .arg(HELPER)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Grava as pastas no Samba (só as que ainda existem).
fn push_folders(p: &Prefs) -> bool {
    let guest = if p.guest { "1" } else { "0" };
    let folders: Vec<&str> =
        p.folders.iter().map(String::as_str).filter(|f| Path::new(f).is_dir()).collect();
    let mut args = vec!["pastas", guest];
    args.extend(folders);
    helper(&args)
}

/// Mostra as pastas e a escolha atual.
fn show_folders(d: &Device, prefs: &Prefs) {
    let folders: Vec<SharedFolder> = candidates()
        .into_iter()
        .map(|path| SharedFolder {
            name: Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into(),
            shared: prefs.folders.contains(&path),
            path: path.into(),
        })
        .collect();
    d.set_sharing_folders(ModelRc::from(Rc::new(VecModel::from(folders))));
    d.set_sharing_guest(prefs.guest);
}

/// Roda `work` numa thread e, no fim, `done(resultado)` na thread da interface.
fn in_background(weak: &Weak<AppWindow>, work: impl FnOnce() -> bool + Send + 'static, done: impl FnOnce(&Device, bool) + Send + 'static) {
    if let Some(ui) = weak.upgrade() {
        ui.global::<Device>().set_sharing_busy(true);
    }
    let weak = weak.clone();
    std::thread::spawn(move || {
        let ok = work();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                let d = ui.global::<Device>();
                d.set_sharing_busy(false);
                done(&d, ok);
            }
        });
    });
}

/// Procura os computadores da rede (alguns segundos, numa thread).
fn refresh_computers(weak: &Weak<AppWindow>, own_host: String) {
    if let Some(ui) = weak.upgrade() {
        ui.global::<Device>().set_computers_busy(true);
    }
    let weak = weak.clone();
    std::thread::spawn(move || {
        let found = output("avahi-browse", &["-tpr", "_smb._tcp"]).map(|t| parse_avahi(&t)).unwrap_or_default();
        let found: Vec<(String, String)> = found
            .into_iter()
            .filter(|(_, host)| !host.eq_ignore_ascii_case(&format!("{own_host}.local")))
            .collect();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                let d = ui.global::<Device>();
                let items: Vec<NetComputer> =
                    found.into_iter().map(|(name, host)| NetComputer { name: name.into(), host: host.into() }).collect();
                d.set_computers(ModelRc::from(Rc::new(VecModel::from(items))));
                d.set_computers_busy(false);
            }
        });
    });
}

fn open_in_files(url: &str) {
    spawn_detached("thunar", &[url]);
}

/// Liga a página Compartilhamento ao global `Device` (só nas Configurações da
/// sessão real; no protótipo a página mostra o aviso de indisponível).
pub fn install(ui: &AppWindow, mode: Mode) {
    if !(crate::shell::session::is_native() && mode == Mode::Settings) {
        return;
    }
    let d = ui.global::<Device>();
    let host = fs::read_to_string("/proc/sys/kernel/hostname").map(|h| h.trim().to_string()).unwrap_or_default();
    d.set_sharing_name(format!("\\\\{}", host.to_uppercase()).into());
    let available = Path::new(HELPER).exists() && Path::new("/usr/sbin/smbd").exists();
    d.set_sharing_available(available);
    if available {
        d.set_sharing_enabled(samba_active());
        show_folders(&d, &load_prefs());
    }

    let weak = ui.as_weak();
    d.on_set_sharing(move |on| {
        let prefs = load_prefs();
        save_prefs(&prefs);
        in_background(
            &weak,
            move || if on { helper(&["ligar"]) && push_folders(&prefs) } else { helper(&["desligar"]) },
            move |d, ok| {
                d.set_sharing_enabled(samba_active());
                d.set_sharing_message(if ok {
                    SharedString::new()
                } else {
                    tr("Sharing was not changed.", "O compartilhamento não foi alterado.").into()
                });
            },
        );
    });

    let weak = ui.as_weak();
    d.on_set_sharing_guest(move |guest| {
        let mut prefs = load_prefs();
        prefs.guest = guest;
        save_prefs(&prefs);
        apply(&weak, prefs);
    });

    let weak = ui.as_weak();
    d.on_toggle_shared_folder(move |i| {
        let Some(ui) = weak.upgrade() else { return };
        let Some(folder) = ui.global::<Device>().get_sharing_folders().row_data(i as usize) else { return };
        let path = folder.path.to_string();
        let mut prefs = load_prefs();
        if prefs.folders.contains(&path) {
            prefs.folders.retain(|f| *f != path);
        } else {
            prefs.folders.push(path);
        }
        save_prefs(&prefs);
        show_folders(&ui.global::<Device>(), &prefs);
        apply(&weak, prefs);
    });

    d.on_sharing_password(|| {
        let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
        let title = format!("--title={}", tr("Network password", "Senha de rede"));
        let script = format!(
            "echo '{}'; sudo smbpasswd -a '{user}'; printf '\\n{}'; read x",
            tr("Password used by other computers to change your shared files.",
               "Senha que os outros computadores usam para alterar as suas pastas compartilhadas."),
            tr("Press Enter to close", "Enter para fechar")
        );
        spawn_detached("foot", &[title.as_str(), "sh", "-c", script.as_str()]);
    });

    let weak = ui.as_weak();
    let own = host.clone();
    d.on_refresh_computers(move || refresh_computers(&weak, own.clone()));
    refresh_computers(&ui.as_weak(), host);

    let weak = ui.as_weak();
    d.on_open_computer(move |i| {
        let Some(ui) = weak.upgrade() else { return };
        if let Some(pc) = ui.global::<Device>().get_computers().row_data(i as usize) {
            open_in_files(&format!("smb://{}/", pc.host));
        }
    });
    d.on_browse_network(|| open_in_files("network:///"));
    d.on_connect_server(|text| {
        if let Some(url) = server_url(&text) {
            open_in_files(&url);
        }
    });
}

/// Manda as pastas para o Samba, se o compartilhamento estiver ligado.
fn apply(weak: &Weak<AppWindow>, prefs: Prefs) {
    let Some(ui) = weak.upgrade() else { return };
    if !ui.global::<Device>().get_sharing_enabled() {
        return;
    }
    in_background(weak, move || push_folders(&prefs), |d, ok| {
        d.set_sharing_message(if ok {
            SharedString::new()
        } else {
            tr("The shared folders were not changed.", "As pastas compartilhadas não foram alteradas.").into()
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferencias_padrao_e_ida_e_volta() {
        let p = Prefs::parse(None, "/home/u/Public");
        assert_eq!(p, Prefs { guest: true, folders: vec!["/home/u/Public".into()] });
        let q = Prefs { guest: false, folders: vec!["/home/u/Música".into(), "/home/u/Docs".into()] };
        assert_eq!(Prefs::parse(Some(&q.to_text()), "/x"), q);
        // Arquivo existente sem pastas: nenhuma (não volta para a pública).
        assert!(Prefs::parse(Some("sem-senha=true\n"), "/x").folders.is_empty());
    }

    #[test]
    fn le_o_avahi() {
        let t = "+;eth0;IPv4;nas;_smb._tcp;local\n\
                 =;eth0;IPv4;Casa\\032NAS;_smb._tcp;local;nas.local;192.168.0.5;445;\n\
                 =;eth0;IPv6;Casa\\032NAS;_smb._tcp;local;nas.local;fe80::1;445;\n\
                 =;eth0;IPv4;ana-pc;_smb._tcp;local;ana-pc.local;192.168.0.7;445;\"x\"\n";
        assert_eq!(
            parse_avahi(t),
            [("ana-pc".to_string(), "ana-pc.local".to_string()), ("Casa NAS".into(), "nas.local".into())]
        );
    }

    #[test]
    fn endereco_vira_url() {
        assert_eq!(server_url(" 192.168.0.10 ").as_deref(), Some("smb://192.168.0.10/"));
        assert_eq!(server_url("\\\\PC\\Fotos").as_deref(), Some("smb://PC/Fotos"));
        assert_eq!(server_url("//nas.local/share").as_deref(), Some("smb://nas.local/share"));
        assert_eq!(server_url("sftp://eu@servidor").as_deref(), Some("sftp://eu@servidor"));
        assert_eq!(server_url("  "), None);
        assert_eq!(server_url("\\\\"), None);
    }
}
