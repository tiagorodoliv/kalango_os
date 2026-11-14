//! Configurações → Usuários: contas de pessoas, como no Ubuntu.
//!
//! - **A própria senha** muda pelo `passwd` (pede a senha atual), rodado num
//!   terminal falso pelo `script` do util-linux: o `passwd` só lê senhas de um
//!   terminal. As respostas vão uma a uma, depois de cada pergunta aparecer.
//! - **Outras contas** (criar, tipo, nome, senha, remover) passam pelo
//!   `pkexec /usr/local/sbin/kalango-usuarios` (política
//!   `org.kalango.usuarios`, senha do administrador lembrada por alguns
//!   minutos; a janela de senha é do lxpolkit). Senhas vão pela entrada padrão,
//!   nunca pela linha de comando. Cada conta nova tem a própria pasta pessoal,
//!   fechada para as outras (modo 700), e entra pela tela de login.

use std::collections::HashSet;
use std::fs;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::i18n::tr;
use crate::{Accounts, AppWindow, Mode, UserAccount};

/// Script de root (via pkexec) que mexe nas contas.
pub const HELPER: &str = "/usr/local/sbin/kalango-usuarios";

/// Uma conta de pessoa do `/etc/passwd`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub login: String,
    /// Nome do GECOS (ou o login).
    pub name: String,
    pub uid: u32,
}

/// Contas de pessoas (UID 1000–59999 com shell de login), na ordem do arquivo.
pub fn people(passwd: &str) -> Vec<Person> {
    passwd
        .lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(':').collect();
            let uid: u32 = f.get(2)?.parse().ok()?;
            let shell = f.get(6).copied().unwrap_or("");
            if !(1000..60000).contains(&uid) || shell.ends_with("nologin") || shell.ends_with("false") {
                return None;
            }
            let gecos = f.get(4).and_then(|g| g.split(',').next()).unwrap_or("").trim();
            let name = if gecos.is_empty() { f[0] } else { gecos };
            Some(Person { login: f[0].to_string(), name: name.to_string(), uid })
        })
        .collect()
}

/// Membros do grupo `sudo` (administradores) de um `/etc/group`.
pub fn admins(group: &str) -> HashSet<String> {
    group
        .lines()
        .find_map(|l| l.strip_prefix("sudo:"))
        .and_then(|rest| rest.splitn(3, ':').nth(2))
        .map(|members| members.split(',').map(str::trim).filter(|m| !m.is_empty()).map(String::from).collect())
        .unwrap_or_default()
}

/// Logins com sessão aberta (`loginctl list-sessions`).
fn signed_in() -> HashSet<String> {
    crate::device::output("loginctl", &["list-sessions", "--no-legend"])
        .map(|text| text.lines().filter_map(|l| l.split_whitespace().nth(2)).map(String::from).collect())
        .unwrap_or_default()
}

/// Login válido para o `useradd` (o mesmo teste do `kalango-usuarios`).
pub fn valid_login(login: &str) -> bool {
    let mut chars = login.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && login.len() <= 32
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Tira acentos das letras do português (e de outras línguas latinas comuns).
fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'ç' => 'c',
        'ñ' => 'n',
        'ý' | 'ÿ' => 'y',
        c => c,
    }
}

/// Login sugerido pelo nome: o primeiro nome em minúsculas, sem acentos
/// ("Maria Silva" → "maria"). Se já existe, primeiro + último nome
/// ("mariasilva"), e depois com número ("maria2").
pub fn suggest_login(name: &str, taken: &HashSet<String>) -> String {
    let words: Vec<String> = name
        .split_whitespace()
        .map(|w| {
            w.to_lowercase()
                .chars()
                .map(fold)
                .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect();
    let Some(first) = words.first() else { return String::new() };
    // Começa com letra (o useradd não aceita login começando com número).
    let first = first.trim_start_matches(|c: char| c.is_ascii_digit()).to_string();
    if first.is_empty() {
        return String::new();
    }
    let mut base: String = first.chars().take(32).collect();
    if !taken.contains(&base) {
        return base;
    }
    if let Some(last) = words.last().filter(|_| words.len() > 1) {
        let both: String = format!("{base}{last}").chars().take(32).collect();
        if !taken.contains(&both) {
            return both;
        }
    }
    base.truncate(30);
    (2..100).map(|n| format!("{base}{n}")).find(|l| !taken.contains(l)).unwrap_or(base)
}

/// Logins (e grupos) que já existem: não podem ser usados numa conta nova.
fn taken_names() -> HashSet<String> {
    let mut names = HashSet::new();
    for file in ["/etc/passwd", "/etc/group"] {
        if let Ok(text) = fs::read_to_string(file) {
            names.extend(text.lines().filter_map(|l| l.split(':').next()).map(String::from));
        }
    }
    names
}

/// Motivo de o login não servir para uma conta nova ("" = serve).
pub fn login_problem(login: &str, taken: &HashSet<String>) -> String {
    if login.is_empty() {
        tr("Type the username", "Digite o nome de usuário").into()
    } else if !valid_login(login) {
        tr(
            "Username: lowercase letters, numbers, - and _, starting with a letter",
            "Nome de usuário: letras minúsculas, números, - e _, começando com letra",
        )
        .into()
    } else if taken.contains(login) {
        tr("This username is already in use", "Esse nome de usuário já está em uso").into()
    } else {
        String::new()
    }
}

/// Primeira letra maiúscula do nome (avatar).
fn initial(name: &str) -> String {
    crate::greeter::initial(name)
}

/// Lê as contas do sistema para a página.
fn load(me: &str) -> (UserAccount, Vec<UserAccount>, bool) {
    let passwd = fs::read_to_string("/etc/passwd").unwrap_or_default();
    let group = fs::read_to_string("/etc/group").unwrap_or_default();
    let admins = admins(&group);
    let online = signed_in();
    let to_ui = |p: &Person| UserAccount {
        login: p.login.clone().into(),
        name: p.name.clone().into(),
        initial: initial(&p.name).into(),
        admin: admins.contains(&p.login),
        me: p.login == me,
        signed_in: online.contains(&p.login),
    };
    let all = people(&passwd);
    let mine = all.iter().find(|p| p.login == me).map(to_ui).unwrap_or_else(|| UserAccount {
        login: me.into(),
        name: crate::greeter::display_name(me).into(),
        initial: initial(&crate::greeter::display_name(me)).into(),
        admin: admins.contains(me),
        me: true,
        signed_in: true,
    });
    let others = all.iter().filter(|p| p.login != me).map(to_ui).collect();
    let can_manage = mine.admin;
    (mine, others, can_manage)
}

fn refresh(ui: &AppWindow) {
    let me = std::env::var("USER").unwrap_or_default();
    let (mine, others, can_manage) = load(&me);
    let a = ui.global::<Accounts>();
    a.set_me(mine);
    a.set_others(ModelRc::from(Rc::new(VecModel::from(others))));
    a.set_can_manage(can_manage);
}

/// Resultado do `kalango-usuarios` (código de saída) em texto.
fn helper_message(code: Option<i32>) -> &'static str {
    match code {
        Some(2) => tr("This username is already in use", "Esse nome de usuário já está em uso"),
        Some(3) => tr("This user is signed in. Log them out first.", "Essa pessoa está com a sessão aberta. Saia da sessão dela antes."),
        Some(4) => tr("The computer needs at least one administrator", "O computador precisa de pelo menos um administrador"),
        Some(5) => tr("This can't be done to your own account", "Isso não pode ser feito na sua própria conta"),
        // 126/127: senha do administrador recusada ou janela fechada.
        Some(126) | Some(127) => tr("Not authorized", "Não autorizado"),
        _ => tr("Could not change the account", "Não foi possível mudar a conta"),
    }
}

/// Roda o `kalango-usuarios` pelo pkexec, com a senha (se houver) na entrada.
fn helper(args: &[&str], password: Option<&str>) -> Result<(), &'static str> {
    let mut child = Command::new("pkexec")
        .arg(HELPER)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|_| tr("pkexec is not installed", "O pkexec não está instalado"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = writeln!(stdin, "{}", password.unwrap_or(""));
    }
    let status = child.wait().map_err(|_| helper_message(None))?;
    if status.success() {
        Ok(())
    } else {
        Err(helper_message(status.code()))
    }
}

/// Pergunta do `passwd` (em inglês: roda com `LC_ALL=C`).
#[derive(Debug, PartialEq, Eq)]
enum Prompt {
    Current,
    New,
    Retype,
}

/// Reconhece a pergunta no fim da saída do `passwd` ("Current password: ").
fn prompt_in(text: &str) -> Option<Prompt> {
    let last = text.rsplit(['\n', '\r']).next().unwrap_or("").trim_end().to_lowercase();
    if !last.ends_with(':') || !last.contains("password") {
        return None;
    }
    if last.contains("current") || last.contains("(current)") {
        Some(Prompt::Current)
    } else if last.contains("retype") || last.contains("re-enter") || last.contains("again") {
        Some(Prompt::Retype)
    } else if last.contains("new") {
        Some(Prompt::New)
    } else {
        Some(Prompt::Current)
    }
}

/// Mensagem do `passwd` em texto para a pessoa.
fn passwd_problem(output: &str) -> String {
    let low = output.to_lowercase();
    if low.contains("authentication token manipulation") || low.contains("authentication failure") {
        tr("The current password is wrong", "A senha atual está errada").into()
    } else if low.contains("longer") || low.contains("too short") {
        tr("The new password is too short", "A nova senha é curta demais").into()
    } else if low.contains("too simple") || low.contains("simplistic") || low.contains("palindrome") {
        tr("The new password is too simple", "A nova senha é simples demais").into()
    } else if low.contains("similar") || low.contains("same as") {
        tr("The new password is too similar to the old one", "A nova senha é parecida demais com a antiga").into()
    } else if low.contains("do not match") {
        tr("The passwords do not match", "As senhas não conferem").into()
    } else {
        // Última linha útil do passwd (em inglês).
        output
            .lines()
            .map(str::trim)
            .rfind(|l| !l.is_empty() && !l.to_lowercase().contains("password:"))
            .map(|l| l.trim_start_matches("passwd: ").to_string())
            .unwrap_or_else(|| tr("Could not change the password", "Não foi possível mudar a senha").into())
    }
}

/// Muda a própria senha pelo `passwd`, respondendo às perguntas uma a uma.
pub fn change_own_password(current: &str, new: &str) -> Result<(), String> {
    let mut child = Command::new("script")
        .args(["-q", "-e", "-c", "passwd", "/dev/null"])
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("script: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("script")?;
    let mut stdout = child.stdout.take().ok_or("script")?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    let mut all = String::new();
    // Texto desde a última resposta (onde a próxima pergunta aparece).
    let mut since = String::new();
    let mut answered: Vec<Prompt> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut failed = false;
    loop {
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            failed = true;
            break;
        };
        match rx.recv_timeout(left.min(Duration::from_millis(200))) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                all.push_str(&text);
                since.push_str(&text);
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        let Some(prompt) = prompt_in(&since) else { continue };
        // A mesma pergunta de novo = a resposta anterior foi recusada
        // (senha curta, simples...): para e mostra o motivo.
        if answered.contains(&prompt) {
            failed = true;
            break;
        }
        // O PAM limpa a entrada ao desligar o eco, logo depois de escrever a
        // pergunta: espera um pouco antes de responder.
        std::thread::sleep(Duration::from_millis(250));
        let answer = if prompt == Prompt::Current { current } else { new };
        if writeln!(stdin, "{answer}").is_err() {
            break;
        }
        answered.push(prompt);
        since.clear();
    }
    if failed {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !failed && status.success() && all.to_lowercase().contains("updated successfully") {
        Ok(())
    } else {
        Err(passwd_problem(&all))
    }
}

/// Roda `work` numa thread e mostra o resultado na página (e relê as contas).
fn run(ui: &AppWindow, success: &'static str, work: impl FnOnce() -> Result<(), String> + Send + 'static) {
    let a = ui.global::<Accounts>();
    a.set_busy(true);
    a.set_message("".into());
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let result = work();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let a = ui.global::<Accounts>();
            a.set_busy(false);
            match result {
                Ok(()) => {
                    a.set_form("".into());
                    a.set_message_error(false);
                    a.set_message(success.into());
                }
                Err(msg) => {
                    a.set_message_error(true);
                    a.set_message(msg.into());
                }
            }
            refresh(&ui);
        });
    });
}

/// Liga a página Usuários (só nas Configurações).
pub fn install(ui: &AppWindow, mode: Mode) {
    if mode != Mode::Settings {
        return;
    }
    refresh(ui);
    let a = ui.global::<Accounts>();
    a.on_suggest_login(|name| suggest_login(&name, &taken_names()).into());
    a.on_check_login(|login| login_problem(&login, &taken_names()).into());

    let native = crate::session::is_native();
    let weak = ui.as_weak();
    a.on_change_own_password(move |current, new| {
        let Some(ui) = weak.upgrade() else { return };
        let (current, new) = (current.to_string(), new.to_string());
        run(&ui, tr("Password changed.", "Senha alterada."), move || {
            if native {
                change_own_password(&current, &new)
            } else {
                Err("protótipo: o passwd só roda na sessão do Kalango OS".into())
            }
        });
    });

    let weak = ui.as_weak();
    a.on_create(move |name, login, password, admin| {
        let Some(ui) = weak.upgrade() else { return };
        let (name, login, password) = (name.to_string(), login.to_string(), password.to_string());
        run(&ui, tr("User added. They can sign in from the login screen.", "Usuário adicionado. Já pode entrar pela tela de login."), move || {
            let kind = if admin { "admin" } else { "padrao" };
            helper(&["criar", &login, &name, kind], Some(&password)).map_err(String::from)
        });
    });

    let weak = ui.as_weak();
    a.on_update(move |login, name, admin, password| {
        let Some(ui) = weak.upgrade() else { return };
        let (login, name, password) = (login.to_string(), name.to_string(), password.to_string());
        // Só o que mudou (cada passo pede a senha do administrador uma vez só:
        // o pkexec lembra por alguns minutos).
        let me = std::env::var("USER").unwrap_or_default();
        let (_, others, _) = load(&me);
        let Some(before) = others.into_iter().find(|u| u.login == login.as_str()) else { return };
        run(&ui, tr("Changes saved.", "Alterações salvas."), move || {
            if before.name != name.as_str() {
                helper(&["nome", &login, &name], None)?;
            }
            if before.admin != admin {
                helper(&["tipo", &login, if admin { "admin" } else { "padrao" }], None)?;
            }
            if !password.is_empty() {
                helper(&["senha", &login], Some(&password))?;
            }
            Ok(())
        });
    });

    let weak = ui.as_weak();
    a.on_remove(move |login, delete_files| {
        let Some(ui) = weak.upgrade() else { return };
        let login = login.to_string();
        run(&ui, tr("User removed.", "Usuário removido."), move || {
            helper(&["remover", &login, if delete_files { "apagar" } else { "manter" }], None).map_err(String::from)
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\n\
                          tiago:x:1000:1000:Tiago Silas,,,:/home/tiago:/bin/zsh\n\
                          nobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin\n\
                          maria:x:1001:1001::/home/maria:/bin/bash\n\
                          svc:x:1002:1002::/srv:/usr/sbin/nologin\n";

    #[test]
    fn contas_de_pessoas() {
        let p = people(PASSWD);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], Person { login: "tiago".into(), name: "Tiago Silas".into(), uid: 1000 });
        assert_eq!(p[1].name, "maria");
    }

    #[test]
    fn administradores() {
        let a = admins("root:x:0:\nsudo:x:27:tiago,ana\nusers:x:100:\n");
        assert!(a.contains("tiago") && a.contains("ana") && a.len() == 2);
        assert!(admins("sudo:x:27:\n").is_empty());
    }

    #[test]
    fn login_sugerido() {
        let taken: HashSet<String> = ["maria".to_string(), "joao".to_string()].into();
        assert_eq!(suggest_login("João", &HashSet::new()), "joao");
        assert_eq!(suggest_login("Maria Conceição", &taken), "mariaconceicao");
        assert_eq!(suggest_login("Maria", &taken), "maria2");
        assert_eq!(suggest_login("  ", &taken), "");
        assert_eq!(suggest_login("2pac Shakur", &taken), "pac");
    }

    #[test]
    fn login_valido() {
        assert!(valid_login("maria"));
        assert!(valid_login("ana-b_2"));
        assert!(!valid_login("Maria"));
        assert!(!valid_login("2ana"));
        assert!(!valid_login(""));
        assert!(!valid_login("a b"));
        let taken: HashSet<String> = ["root".to_string()].into();
        assert!(!login_problem("root", &taken).is_empty());
        assert!(login_problem("ana", &taken).is_empty());
    }

    #[test]
    fn perguntas_do_passwd() {
        assert_eq!(prompt_in("Changing password for tiago.\r\nCurrent password: "), Some(Prompt::Current));
        assert_eq!(prompt_in("\r\nNew password: "), Some(Prompt::New));
        assert_eq!(prompt_in("\r\nRetype new password: "), Some(Prompt::Retype));
        assert_eq!(prompt_in("passwd: password updated successfully\r\n"), None);
        assert_eq!(
            passwd_problem("Current password: \r\npasswd: Authentication token manipulation error\r\n"),
            tr("The current password is wrong", "A senha atual está errada")
        );
        assert_eq!(
            passwd_problem("You must choose a longer password.\r\nNew password: "),
            tr("The new password is too short", "A nova senha é curta demais")
        );
    }
}
