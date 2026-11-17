//! Integridade da interface: avisa quando os arquivos do KalangOS mudaram fora
//! de uma atualização.
//!
//! O KalangOS é software livre: quem usa pode mudar o que quiser. A regra é
//! que isso seja **claro e com a senha do administrador**. Os arquivos da
//! interface (o `kalango-shell` e os scripts de `/usr/local`, do pacote
//! `kalango-shell`) já são de root; esta verificação pega o que mudou por fora
//! do apt (um binário copiado à mão, um script editado, um programa com sudo).
//!
//! Ao entrar na sessão, a área de trabalho compara cada arquivo do pacote com
//! o MD5 que o dpkg guardou na instalação (`/var/lib/dpkg/info/
//! kalango-shell.md5sums`, o mesmo do `dpkg --verify`). Se algum mudou ou
//! sumiu, e a mudança ainda não foi aceita, abre um aviso (zenity) que diz
//! quais arquivos e dá duas saídas, as duas pelo
//! `pkexec /usr/local/sbin/kalango-integridade` (senha do administrador
//! sempre, política `org.kalango.integridade`):
//!
//! - **Aceitar as mudanças**: grava o MD5 atual de cada arquivo em
//!   [`ACCEPTED`] (de root). O aviso só volta se o arquivo mudar de novo;
//! - **Restaurar o original**: reinstala o pacote pelo apt.
//!
//! Uma atualização oficial (apt, assinada) troca os MD5 do dpkg junto com os
//! arquivos, então não gera aviso.

use std::path::Path;
use std::process::{Command, Stdio};

/// MD5 de cada arquivo do pacote, gravados pelo dpkg ("md5  caminho" sem a `/`).
pub const MD5SUMS: &str = "/var/lib/dpkg/info/kalango-shell.md5sums";

/// Mudanças aceitas pelo administrador ("md5  /caminho", ou "ausente  /caminho").
pub const ACCEPTED: &str = "/var/lib/kalango/integridade/aceitos";

/// Script de root que aceita ou desfaz as mudanças.
const HELPER: &str = "/usr/local/sbin/kalango-integridade";

/// Marca de um arquivo apagado na lista de aceitos.
const MISSING: &str = "ausente";

/// Um arquivo do pacote diferente do instalado pelo apt.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// Caminho absoluto.
    pub path: String,
    /// MD5 atual, ou `None` se o arquivo sumiu.
    pub md5: Option<String>,
}

/// Lê pares (md5, caminho) de um arquivo no formato do `md5sum`.
fn pairs(text: &str) -> impl Iterator<Item = (&str, &str)> {
    text.lines().filter_map(|l| {
        let (sum, path) = l.split_once("  ")?;
        Some((sum.trim(), path.trim()))
    })
}

/// Arquivos que mudaram em relação ao `md5sums` do dpkg e não estão em
/// `accepted`. `read` devolve o conteúdo de um caminho absoluto (`None` = não
/// existe); separado para os testes.
pub fn changes(md5sums: &str, accepted: &str, read: impl Fn(&str) -> Option<Vec<u8>>) -> Vec<Change> {
    let ok: Vec<(&str, &str)> = pairs(accepted).collect();
    let mut out = Vec::new();
    for (sum, rel) in pairs(md5sums) {
        let path = format!("/{}", rel.trim_start_matches('/'));
        let now = read(&path).map(|data| crate::tools::codec::md5(&data));
        if now.as_deref() == Some(sum) {
            continue;
        }
        let tag = now.as_deref().unwrap_or(MISSING);
        if ok.iter().any(|(s, p)| *p == path && *s == tag) {
            continue;
        }
        out.push(Change { path, md5: now });
    }
    out
}

/// Confere agora os arquivos do pacote (vazio fora do KalangOS instalado).
pub fn check() -> Vec<Change> {
    let Ok(md5sums) = std::fs::read_to_string(MD5SUMS) else {
        return Vec::new();
    };
    let accepted = std::fs::read_to_string(ACCEPTED).unwrap_or_default();
    changes(&md5sums, &accepted, |p| std::fs::read(p).ok())
}

/// Texto do aviso, com até 8 arquivos.
pub fn message(list: &[Change]) -> String {
    use crate::common::i18n::tr;
    let mut files = String::new();
    for c in list.iter().take(8) {
        let what = if c.md5.is_some() { tr("changed", "alterado") } else { tr("removed", "apagado") };
        files.push_str(&format!("  • {} ({what})\n", c.path));
    }
    if list.len() > 8 {
        files.push_str(&format!("  • … +{}\n", list.len() - 8));
    }
    format!(
        "{}\n\n{files}\n{}\n\n{}",
        tr(
            "These KalangOS interface files were changed outside an official update:",
            "Estes arquivos da interface do KalangOS foram mudados fora de uma atualização oficial:",
        ),
        tr(
            "KalangOS is free software and you may change it. If you did this, accept the changes \
             (the administrator password is required). If you did not, restore the original files.",
            "O KalangOS é software livre e você pode mudá-lo. Se foi você, aceite as mudanças \
             (pede a senha do administrador). Se não foi, restaure os arquivos originais.",
        ),
        tr(
            "Until you choose, this notice appears every time you log in.",
            "Enquanto você não escolher, este aviso aparece a cada vez que você entrar.",
        ),
    )
}

/// Ao entrar na sessão (área de trabalho real): confere numa thread e, se
/// houver mudança não aceita, mostra o aviso e faz o que a pessoa escolher.
pub fn start() {
    if !Path::new(MD5SUMS).exists() {
        return;
    }
    std::thread::spawn(|| {
        // Depois de a área de trabalho e a dock aparecerem.
        std::thread::sleep(std::time::Duration::from_secs(8));
        let list = check();
        if list.is_empty() {
            return;
        }
        for c in &list {
            eprintln!("kalango-shell: integridade: {} mudou fora do apt", c.path);
        }
        ask(&list);
    });
}

/// Aviso com três botões: aceitar, restaurar ou deixar para depois.
fn ask(list: &[Change]) {
    use crate::common::i18n::tr;
    let accept = tr("Accept changes", "Aceitar as mudanças");
    let restore = tr("Restore original", "Restaurar o original");
    let out = Command::new("zenity")
        .args([
            "--question",
            "--icon=dialog-warning",
            "--width=560",
            "--no-markup",
            &format!("--title={}", tr("KalangOS interface changed", "Interface do KalangOS mudada")),
            &format!("--text={}", message(list)),
            &format!("--ok-label={accept}"),
            &format!("--cancel-label={}", tr("Not now", "Agora não")),
            &format!("--extra-button={restore}"),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let Ok(out) = out else {
        return;
    };
    // O zenity escreve o rótulo do botão extra na saída padrão.
    let action = if out.status.success() {
        "aceitar"
    } else if String::from_utf8_lossy(&out.stdout).trim() == restore {
        "restaurar"
    } else {
        return;
    };
    let ok = Command::new("pkexec")
        .args([HELPER, action])
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    let note = match (action, ok) {
        ("aceitar", true) => tr("Changes accepted.", "Mudanças aceitas."),
        ("restaurar", true) => {
            tr("Original files restored. Log in again to use them.", "Arquivos originais restaurados. Entre de novo para usá-los.")
        }
        _ => tr("Nothing was changed (not authorized or failed).", "Nada foi mudado (sem autorização ou com erro)."),
    };
    let _ = Command::new("zenity")
        .args(["--info", "--no-markup", "--width=360", &format!("--text={note}")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMS: &str = "0cc175b9c0f1b6a831c399e269772661  usr/local/bin/a\n\
                        92eb5ffee6ae2fec3ad71c777531578f  usr/local/bin/b\n\
                        4a8a08f09d37b73795649038408b5f33  usr/local/bin/c\n";

    fn disk(path: &str) -> Option<Vec<u8>> {
        match path {
            "/usr/local/bin/a" => Some(b"a".to_vec()),  // igual ao pacote
            "/usr/local/bin/b" => Some(b"x".to_vec()),  // mudado
            _ => None,                                   // c sumiu
        }
    }

    #[test]
    fn acha_o_que_mudou_e_o_que_sumiu() {
        let list = changes(SUMS, "", disk);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], Change { path: "/usr/local/bin/b".into(), md5: Some(crate::tools::codec::md5(b"x")) });
        assert_eq!(list[1], Change { path: "/usr/local/bin/c".into(), md5: None });
    }

    #[test]
    fn mudanca_aceita_nao_avisa_ate_mudar_de_novo() {
        let x = crate::tools::codec::md5(b"x");
        let accepted = format!("{x}  /usr/local/bin/b\nausente  /usr/local/bin/c\n");
        assert!(changes(SUMS, &accepted, disk).is_empty());
        // O b mudou outra vez: avisa de novo.
        let other = changes(SUMS, &accepted, |p| if p.ends_with("/b") { Some(b"y".to_vec()) } else { disk(p) });
        assert_eq!(other.len(), 1);
        assert_eq!(other[0].path, "/usr/local/bin/b");
    }

    #[test]
    fn mensagem_lista_os_arquivos() {
        let text = message(&[Change { path: "/usr/local/bin/kalango-shell".into(), md5: Some("x".into()) }]);
        assert!(text.contains("/usr/local/bin/kalango-shell"));
    }
}
