//! Tela de login de verdade: o shell como greeter do greetd.
//!
//! O greetd roda `kalango-greeter` (labwc só com `kalango-shell --greeter`) como
//! usuário `_greetd`. A tela é a mesma `LoginScreen` do protótipo, mas a senha
//! vai para o greetd, que a confere no PAM e, se estiver certa, inicia a sessão
//! (`kalango-session`) quando o greeter sai.
//!
//! Protocolo (IPC do greetd, socket Unix em `$GREETD_SOCK`): cada mensagem é um
//! JSON precedido do tamanho em 4 bytes (u32, ordem nativa).
//! 1. `create_session {username}`;
//! 2. o greetd responde `auth_message` (ex.: "Password:") e o greeter manda
//!    `post_auth_message_response {response}`, até vir `success` ou `error`;
//! 3. `start_session {cmd, env}` e o greeter encerra.
//!
//! Sem `$GREETD_SOCK` (desenvolvimento) qualquer senha não vazia entra.

use crate::i18n::tr;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppWindow, LoginUser, Shell};

/// Comando da sessão iniciada após o login.
pub const SESSION_CMD: &str = "kalango-session";

/// Escapa um texto como string JSON (com aspas).
pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Lê um objeto JSON plano (valores texto, número, booleano ou null) das
/// respostas do greetd. Valores que não são texto viram o texto bruto.
pub fn parse_object(json: &str) -> Option<HashMap<String, String>> {
    let mut chars = json.trim().chars().peekable();
    let mut map = HashMap::new();
    if chars.next()? != '{' {
        return None;
    }
    loop {
        skip_ws(&mut chars);
        match chars.peek()? {
            '}' => return Some(map),
            ',' => {
                chars.next();
                continue;
            }
            '"' => {}
            _ => return None,
        }
        let key = read_string(&mut chars)?;
        skip_ws(&mut chars);
        if chars.next()? != ':' {
            return None;
        }
        skip_ws(&mut chars);
        let value = if *chars.peek()? == '"' {
            read_string(&mut chars)?
        } else {
            let mut raw = String::new();
            while let Some(&c) = chars.peek() {
                if c == ',' || c == '}' || c.is_whitespace() {
                    break;
                }
                raw.push(c);
                chars.next();
            }
            raw
        };
        map.insert(key, value);
    }
}

fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }
}

fn read_string(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<String> {
    if chars.next()? != '"' {
        return None;
    }
    let mut out = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'u' => {
                    let hex: String = (0..4).filter_map(|_| chars.next()).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
}

/// Conexão com o greetd.
struct Greetd {
    stream: UnixStream,
}

impl Greetd {
    fn connect(path: &Path) -> std::io::Result<Greetd> {
        Ok(Greetd {
            stream: UnixStream::connect(path)?,
        })
    }

    /// Envia uma requisição e devolve a resposta já lida.
    fn request(&mut self, json: &str) -> Result<HashMap<String, String>, String> {
        let io = |e: std::io::Error| format!("falha ao falar com o greetd: {e}");
        self.stream
            .write_all(&(json.len() as u32).to_ne_bytes())
            .map_err(io)?;
        self.stream.write_all(json.as_bytes()).map_err(io)?;
        let mut len = [0u8; 4];
        self.stream.read_exact(&mut len).map_err(io)?;
        let mut body = vec![0u8; u32::from_ne_bytes(len) as usize];
        self.stream.read_exact(&mut body).map_err(io)?;
        let text = String::from_utf8_lossy(&body);
        parse_object(&text).ok_or_else(|| format!("resposta inválida do greetd: {text}"))
    }
}

/// Resultado de uma tentativa de login.
#[derive(Debug, PartialEq, Eq)]
pub enum LoginError {
    /// Senha (ou usuário) errados: a pessoa pode tentar de novo.
    Denied,
    /// Problema de comunicação ou de sistema, com a descrição.
    Failed(String),
}

/// Faz o login pelo greetd e pede para iniciar `cmd` em seguida.
///
/// Responde às perguntas do PAM com a senha (as secretas) ou com o usuário
/// (as visíveis); mensagens informativas só são confirmadas.
pub fn login(sock: &Path, user: &str, password: &str, cmd: &[&str]) -> Result<(), LoginError> {
    let mut greetd = Greetd::connect(sock).map_err(|e| LoginError::Failed(e.to_string()))?;
    let mut reply = greetd
        .request(&format!(
            r#"{{"type":"create_session","username":{}}}"#,
            json_string(user)
        ))
        .map_err(LoginError::Failed)?;
    loop {
        match reply.get("type").map(String::as_str) {
            Some("success") => break,
            Some("auth_message") => {
                let answer = match reply.get("auth_message_type").map(String::as_str) {
                    Some("secret") => json_string(password),
                    Some("visible") => json_string(user),
                    _ => "null".to_string(),
                };
                reply = greetd
                    .request(&format!(
                        r#"{{"type":"post_auth_message_response","response":{answer}}}"#
                    ))
                    .map_err(LoginError::Failed)?;
            }
            _ => {
                // Erro: encerra a tentativa para o greetd aceitar a próxima.
                let _ = greetd.request(r#"{"type":"cancel_session"}"#);
                return Err(match reply.get("error_type").map(String::as_str) {
                    Some("auth_error") => LoginError::Denied,
                    _ => LoginError::Failed(reply.get("description").cloned().unwrap_or_default()),
                });
            }
        }
    }
    let cmd_json: Vec<String> = cmd.iter().map(|c| json_string(c)).collect();
    let reply = greetd
        .request(&format!(
            r#"{{"type":"start_session","cmd":[{}],"env":[]}}"#,
            cmd_json.join(",")
        ))
        .map_err(LoginError::Failed)?;
    if reply.get("type").map(String::as_str) == Some("success") {
        Ok(())
    } else {
        Err(LoginError::Failed(reply.get("description").cloned().unwrap_or_default()))
    }
}

/// Usuário de pessoa (UID 1000–59999 com shell de login) de um `/etc/passwd`:
/// devolve `(login, nome exibido)`. O nome vem do campo GECOS, se houver.
pub fn first_human_user(passwd: &str) -> Option<(String, String)> {
    crate::users::people(passwd).into_iter().next().map(|p| (p.login, p.name))
}

/// Nome exibido de um login, pelo GECOS do `/etc/passwd` (ou o próprio login).
pub fn display_name(login: &str) -> String {
    std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|p| {
            p.lines().find_map(|l| {
                let f: Vec<&str> = l.split(':').collect();
                (f.first() == Some(&login))
                    .then(|| f.get(4).and_then(|g| g.split(',').next()).unwrap_or(""))
                    .filter(|g| !g.trim().is_empty())
                    .map(|g| g.trim().to_string())
            })
        })
        .unwrap_or_else(|| login.to_string())
}

/// Primeira letra maiúscula do nome (para o avatar).
pub fn initial(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_else(|| "U".into())
}

/// Liga a tela de login ao greetd (modo `--greeter`).
pub fn install(ui: &AppWindow) {
    let shell = ui.global::<Shell>();
    shell.set_greeter(true);
    let (user, name) = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|p| first_human_user(&p))
        .unwrap_or_else(|| ("user".into(), tr("User", "Usuário").into()));
    shell.set_login_user(user.into());
    shell.set_user_initial(initial(&name).into());
    shell.set_user_name(name.into());
    // Com mais de uma conta, a tela mostra todas para escolher.
    let people: Vec<LoginUser> = std::fs::read_to_string("/etc/passwd")
        .map(|p| crate::users::people(&p))
        .unwrap_or_default()
        .into_iter()
        .map(|p| LoginUser { initial: initial(&p.name).into(), login: p.login.into(), name: p.name.into() })
        .collect();
    shell.set_login_users(ModelRc::from(Rc::new(VecModel::from(people))));

    let weak = ui.as_weak();
    shell.on_start_login(move |user, password| {
        let Some(ui) = weak.upgrade() else { return };
        let shell = ui.global::<Shell>();
        shell.set_login_error("".into());
        shell.set_login_busy(true);
        let weak = ui.as_weak();
        let (user, password) = (user.to_string(), password.to_string());
        // O PAM demora ~2 s para recusar uma senha: roda fora da thread da UI.
        std::thread::spawn(move || {
            let result = match std::env::var_os("GREETD_SOCK") {
                Some(sock) => login(Path::new(&sock), &user, &password, &[SESSION_CMD]),
                None if !password.is_empty() => Ok(()),
                None => Err(LoginError::Denied),
            };
            let _ = slint::invoke_from_event_loop(move || {
                let Some(ui) = weak.upgrade() else { return };
                let shell = ui.global::<Shell>();
                shell.set_login_busy(false);
                match result {
                    // O greetd inicia a sessão quando o greeter sai.
                    Ok(()) => {
                        let _ = slint::quit_event_loop();
                    }
                    Err(LoginError::Denied) => shell.set_login_error(tr("Wrong password", "Senha incorreta").into()),
                    Err(LoginError::Failed(msg)) => {
                        eprintln!("kalango-shell: login: {msg}");
                        shell.set_login_error(tr("Could not sign in", "Não foi possível entrar").into());
                    }
                }
            });
        });
    });

    // "Outro usuário": a UI troca o login e o Rust atualiza nome e avatar.
    let weak = ui.as_weak();
    shell.on_choose_user(move |user| {
        let Some(ui) = weak.upgrade() else { return };
        let shell = ui.global::<Shell>();
        let name = display_name(&user);
        shell.set_login_user(user);
        shell.set_user_initial(initial(&name).into());
        shell.set_user_name(name.into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn json_escapa_e_le_de_volta() {
        let s = json_string("a\"b\\c\nç");
        assert_eq!(s, r#""a\"b\\c\nç""#);
        let obj = parse_object(&format!(r#"{{"x":{s}, "n": null, "t":true}}"#)).unwrap();
        assert_eq!(obj["x"], "a\"b\\c\nç");
        assert_eq!(obj["n"], "null");
        assert_eq!(obj["t"], "true");
    }

    #[test]
    fn acha_o_primeiro_usuario_de_pessoa() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\n\
                      _greetd:x:999:999::/var/lib/greetd:/usr/sbin/nologin\n\
                      user:x:1000:1000:Live user,,,:/home/user:/bin/bash\n";
        assert_eq!(
            first_human_user(passwd),
            Some(("user".into(), "Live user".into()))
        );
        assert_eq!(initial("ana"), "A");
    }

    /// Servidor greetd falso: aceita só a senha "certa".
    fn fake_greetd(dir: &Path) -> std::path::PathBuf {
        let sock = dir.join("greetd.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut s = stream.unwrap();
                let mut ok = false;
                loop {
                    let mut len = [0u8; 4];
                    if s.read_exact(&mut len).is_err() {
                        break;
                    }
                    let mut body = vec![0u8; u32::from_ne_bytes(len) as usize];
                    s.read_exact(&mut body).unwrap();
                    let req = parse_object(&String::from_utf8(body).unwrap()).unwrap();
                    let resp = match req["type"].as_str() {
                        "create_session" => {
                            r#"{"type":"auth_message","auth_message_type":"secret","auth_message":"Password: "}"#.to_string()
                        }
                        "post_auth_message_response" if req["response"] == "certa" => {
                            ok = true;
                            r#"{"type":"success"}"#.to_string()
                        }
                        "post_auth_message_response" => {
                            r#"{"type":"error","error_type":"auth_error","description":"pam"}"#.to_string()
                        }
                        "start_session" if ok => r#"{"type":"success"}"#.to_string(),
                        _ => r#"{"type":"success"}"#.to_string(),
                    };
                    s.write_all(&(resp.len() as u32).to_ne_bytes()).unwrap();
                    s.write_all(resp.as_bytes()).unwrap();
                }
            }
        });
        sock
    }

    #[test]
    fn login_pelo_protocolo_do_greetd() {
        let dir = std::env::temp_dir().join(format!("kalango-greetd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sock = fake_greetd(&dir);
        assert_eq!(login(&sock, "user", "errada", &["x"]), Err(LoginError::Denied));
        assert_eq!(login(&sock, "user", "certa", &["x"]), Ok(()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
