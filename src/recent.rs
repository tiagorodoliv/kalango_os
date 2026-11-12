//! Seção RECENTES do menu de apps na sessão real.
//!
//! Os apps GTK (Thunar, Mousepad, Ristretto...) registram os arquivos abertos
//! em `~/.local/share/recently-used.xbel` (formato XBEL). Aqui lemos os mais
//! recentes que ainda existem. Na ISO live, o primeiro item é o atalho
//! "Instalar Kalango OS".

use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AppWindow, Apps, RecentEntry};

/// Quantos itens cabem na seção (duas colunas no layout).
const SLOTS: usize = 2;

/// Um bookmark do XBEL: endereço (`file://...`) e data de modificação (ISO 8601).
#[derive(Debug, PartialEq, Eq)]
pub struct Bookmark {
    pub href: String,
    pub modified: String,
}

/// Lê os `<bookmark href="..." modified="...">` de um arquivo XBEL.
pub fn parse_xbel(xml: &str) -> Vec<Bookmark> {
    xml.split("<bookmark ")
        .skip(1)
        .filter_map(|tag| {
            // Espaço na frente: o primeiro atributo também fica como " nome=".
            let head = format!(" {}", tag.split('>').next()?);
            Some(Bookmark {
                href: attr(&head, "href")?,
                modified: attr(&head, "modified")
                    .or_else(|| attr(&head, "visited"))
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// Valor de um atributo `nome="valor"` dentro de uma tag.
fn attr(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
    let end = tag[start..].find('"')? + start;
    Some(unescape_xml(&tag[start..end]))
}

fn unescape_xml(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Converte `file:///home/x/Meus%20Docs/a.txt` em caminho. Outros esquemas → `None`.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let raw = uri.strip_prefix("file://")?;
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

/// Tipo do arquivo para o ícone: "code", "image" ou "file".
pub fn kind_for(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "rs" | "c" | "h" | "cpp" | "hpp" | "py" | "js" | "ts" | "sh" | "slint" | "toml"
        | "json" | "html" | "css" | "java" | "go" | "dart" => "code",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" => "image",
        _ => "file",
    }
}

/// Pasta do arquivo para exibição, com a pasta pessoal abreviada como "~".
pub fn display_dir(path: &Path, home: &Path) -> String {
    let dir = path.parent().unwrap_or(Path::new("/"));
    match dir.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => dir.display().to_string(),
    }
}

/// Arquivos recentes que ainda existem, do mais novo para o mais velho.
pub fn recent_files(xbel: &str, exists: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut marks = parse_xbel(xbel);
    // ISO 8601 em UTC ordena certo como texto.
    marks.sort_by(|a, b| b.modified.cmp(&a.modified));
    let mut out: Vec<PathBuf> = Vec::new();
    for mark in marks {
        if let Some(path) = uri_to_path(&mark.href) {
            if exists(&path) && !out.contains(&path) {
                out.push(path);
            }
        }
    }
    out
}

/// Diz se o sistema está rodando da ISO live (pode ser instalado).
pub fn is_live() -> bool {
    Path::new("/run/live/medium").exists()
}

/// Preenche `Apps.recents` com os recentes de verdade (sessão real).
pub fn install(ui: &AppWindow) {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let mut items: Vec<RecentEntry> = Vec::new();
    if is_live() {
        items.push(RecentEntry {
            name: "Instalar Kalango OS".into(),
            path: "Copiar o sistema para o disco".into(),
            kind: "install".into(),
            target: "".into(),
            app: "installer".into(),
        });
    }
    let xbel = std::fs::read_to_string(home.join(".local/share/recently-used.xbel"))
        .unwrap_or_default();
    for path in recent_files(&xbel, |p| p.is_file()) {
        if items.len() >= SLOTS {
            break;
        }
        items.push(RecentEntry {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into(),
            path: display_dir(&path, &home).into(),
            kind: kind_for(&path).into(),
            target: path.to_string_lossy().into_owned().into(),
            app: "".into(),
        });
    }
    ui.global::<Apps>()
        .set_recents(ModelRc::from(Rc::new(VecModel::from(items))));
}

#[cfg(test)]
mod tests {
    use super::*;

    const XBEL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0">
  <bookmark href="file:///home/user/Documentos/velho.txt" added="2026-11-01T10:00:00Z" modified="2026-11-01T10:00:00Z" visited="2026-11-01T10:00:00Z">
    <info/>
  </bookmark>
  <bookmark href="file:///home/user/Projetos/Meu%20App/main.rs" added="2026-11-10T09:00:00Z" modified="2026-11-12T08:00:00Z" visited="2026-11-12T08:00:00Z"/>
  <bookmark href="https://exemplo.com/x" modified="2026-11-12T09:00:00Z"/>
  <bookmark href="file:///home/user/sumiu.png" modified="2026-11-12T10:00:00Z"/>
</xbel>"#;

    #[test]
    fn le_os_bookmarks() {
        let marks = parse_xbel(XBEL);
        assert_eq!(marks.len(), 4);
        assert_eq!(marks[1].modified, "2026-11-12T08:00:00Z");
    }

    #[test]
    fn ordena_e_ignora_o_que_nao_existe() {
        let files = recent_files(XBEL, |p| !p.ends_with("sumiu.png"));
        assert_eq!(
            files,
            vec![
                PathBuf::from("/home/user/Projetos/Meu App/main.rs"),
                PathBuf::from("/home/user/Documentos/velho.txt"),
            ]
        );
    }

    #[test]
    fn pasta_e_tipo_para_exibir() {
        let home = Path::new("/home/user");
        let p = Path::new("/home/user/Projetos/Meu App/main.rs");
        assert_eq!(display_dir(p, home), "~/Projetos/Meu App");
        assert_eq!(display_dir(Path::new("/home/user/a.txt"), home), "~");
        assert_eq!(display_dir(Path::new("/etc/hosts"), home), "/etc");
        assert_eq!(kind_for(p), "code");
        assert_eq!(kind_for(Path::new("x.JPG")), "image");
        assert_eq!(kind_for(Path::new("x.pdf")), "file");
    }

    #[test]
    fn decodifica_uri() {
        assert_eq!(
            uri_to_path("file:///a/b%C3%A7/c%20d").unwrap(),
            PathBuf::from("/a/bç/c d")
        );
        assert_eq!(uri_to_path("https://x"), None);
    }
}
