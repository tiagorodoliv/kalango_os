//! Apps instalados no sistema, lidos dos arquivos `.desktop` (freedesktop).
//!
//! Alimenta a página "Todos os apps" do menu e a busca por apps que não estão
//! entre os fixados. Cada app vira um [`DesktopApp`] com nome em pt-BR, ícone
//! do tema (hicolor) e o comando do `Exec=` já sem os códigos de campo
//! (`%f`, `%U`...), que o menu abre sem argumentos.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Prefixo dos ids de apps do sistema no menu (`desktop:/usr/share/...`),
/// para não colidir com os ids do catálogo ("files", "terminal"...).
pub const ID_PREFIX: &str = "desktop:";

/// Um app instalado, lido de um arquivo `.desktop`.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopApp {
    /// Caminho do arquivo `.desktop`.
    pub path: PathBuf,
    /// Nome exibido (de preferência `Name[pt_BR]`).
    pub name: String,
    /// Texto extra para a busca: nome genérico, palavras-chave e programa.
    pub keywords: String,
    /// Valor de `Icon=` (nome no tema ou caminho absoluto).
    pub icon: String,
    /// Comando a rodar, já sem os códigos de campo.
    pub argv: Vec<String>,
    /// `Terminal=true`: o programa roda dentro do foot.
    pub terminal: bool,
}

impl DesktopApp {
    /// Id do app no menu (ver [`ID_PREFIX`]).
    pub fn id(&self) -> String {
        format!("{ID_PREFIX}{}", self.path.display())
    }

    /// Nome do programa (sem pasta), usado para reconhecer os apps fixados.
    pub fn program(&self) -> &str {
        self.argv
            .first()
            .map(|p| p.rsplit('/').next().unwrap_or(p))
            .unwrap_or("")
    }

    /// Comando completo para abrir o app (com o foot na frente se for de
    /// terminal).
    pub fn command(&self) -> Vec<String> {
        let mut cmd = Vec::new();
        if self.terminal {
            cmd.push("foot".to_string());
            cmd.push(format!("--title={}", self.name));
        }
        cmd.extend(self.argv.iter().cloned());
        cmd
    }
}

/// Programas que não aparecem na lista: o instalador do Debian (o Kalango
/// tem o próprio atalho) e utilitários que não fazem sentido no menu.
const HIDDEN_PROGRAMS: &[&str] = &[
    "calamares",
    "install-debian",
    "kalango-instalar",
    "kalango-shell",
    "footclient",
];

/// Lê um `.desktop`. Devolve `None` para o que não deve aparecer no menu:
/// não é aplicativo, está oculto (`NoDisplay`/`Hidden`), é só de outro
/// ambiente (`OnlyShowIn`) ou não tem `Exec`.
pub fn parse(path: &Path, content: &str) -> Option<DesktopApp> {
    let mut in_entry = false;
    let mut name = None;
    let mut name_pt = None;
    let mut name_pt_br = None;
    let mut generic = String::new();
    let mut keywords = String::new();
    let mut icon = String::new();
    let mut exec = None;
    let mut terminal = false;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // Só interessa o grupo principal; ações ficam em outros grupos.
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "Type" if value != "Application" => return None,
            "NoDisplay" | "Hidden" if value == "true" => return None,
            "OnlyShowIn" => return None,
            "Name" => name = Some(value.to_string()),
            "Name[pt]" => name_pt = Some(value.to_string()),
            "Name[pt_BR]" => name_pt_br = Some(value.to_string()),
            "GenericName" | "GenericName[pt_BR]" => {
                generic.push(' ');
                generic.push_str(value);
            }
            "Keywords" | "Keywords[pt_BR]" => {
                keywords.push(' ');
                keywords.push_str(&value.replace(';', " "));
            }
            "Icon" => icon = value.to_string(),
            "Exec" => exec = Some(value.to_string()),
            "Terminal" => terminal = value == "true",
            _ => {}
        }
    }

    let name = name_pt_br.or(name_pt).or(name)?;
    let argv = parse_exec(&exec?);
    if argv.is_empty() {
        return None;
    }
    let mut app = DesktopApp {
        path: path.to_path_buf(),
        name,
        keywords: String::new(),
        icon,
        argv,
        terminal,
    };
    if HIDDEN_PROGRAMS.contains(&app.program()) {
        return None;
    }
    app.keywords = format!("{generic}{keywords} {}", app.program());
    Some(app)
}

/// Quebra o `Exec=` em argumentos: respeita aspas duplas e barras invertidas
/// e tira os códigos de campo (`%f`, `%U`, `%i`...); `%%` vira `%`.
pub fn parse_exec(exec: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut has_arg = false;
    let mut quoted = false;
    let mut chars = exec.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                has_arg = true;
            }
            '\\' if quoted => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            ' ' | '\t' if !quoted => {
                if has_arg {
                    args.push(std::mem::take(&mut current));
                    has_arg = false;
                }
            }
            // `%%` é um "%" literal; outro código de campo some (junto com o
            // argumento, se ele ficar vazio).
            '%' => {
                if chars.next() == Some('%') {
                    current.push('%');
                    has_arg = true;
                }
            }
            other => {
                current.push(other);
                has_arg = true;
            }
        }
    }
    if has_arg && !current.is_empty() {
        args.push(current);
    }
    args.retain(|a| !a.is_empty());
    args
}

/// Pastas `applications` em ordem de prioridade: a do usuário primeiro
/// (`$XDG_DATA_HOME`), depois as do sistema (`$XDG_DATA_DIRS`).
pub fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let user = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| format!("{home}/.local/share"));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    std::iter::once(user)
        .chain(system.split(':').map(str::to_string))
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Lê todos os apps instalados, sem repetir: um `.desktop` com o mesmo nome
/// de arquivo numa pasta de prioridade maior esconde o das outras.
/// Volta em ordem alfabética (sem diferenciar maiúsculas nem acentos).
pub fn scan(dirs: &[PathBuf]) -> Vec<DesktopApp> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir.join("applications")) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "desktop"))
            .collect();
        files.sort();
        for file in files {
            let Some(file_name) = file.file_name().map(|n| n.to_os_string()) else {
                continue;
            };
            if !seen.insert(file_name) {
                continue;
            }
            if let Some(app) = fs::read_to_string(&file)
                .ok()
                .and_then(|text| parse(&file, &text))
            {
                apps.push(app);
            }
        }
    }
    apps.sort_by_key(|a| crate::apps::normalize(&a.name));
    apps
}

/// Acha o arquivo do ícone (`Icon=`) no tema hicolor, no Adwaita ou em
/// `/usr/share/pixmaps`. Prefere PNG pequeno (rápido de carregar) e cai para
/// SVG. `None` se não achar; o menu mostra a inicial do app.
pub fn find_icon(icon: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    if icon.is_empty() {
        return None;
    }
    let path = Path::new(icon);
    if path.is_absolute() {
        return path.is_file().then(|| path.to_path_buf());
    }
    const SIZES: &[&str] = &["64x64", "48x48", "96x96", "128x128", "scalable", "256x256", "512x512"];
    const THEMES: &[&str] = &["hicolor", "Adwaita"];
    for dir in dirs {
        for theme in THEMES {
            for size in SIZES {
                let ext = if *size == "scalable" { "svg" } else { "png" };
                let candidate = dir.join(format!("icons/{theme}/{size}/apps/{icon}.{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    ["png", "svg"]
        .iter()
        .flat_map(|ext| dirs.iter().map(move |d| d.join(format!("pixmaps/{icon}.{ext}"))))
        .find(|p| p.is_file())
}

/// Lê o app de um id do menu (`desktop:<caminho>`). `None` se o id não é de
/// app do sistema ou o arquivo sumiu.
pub fn from_id(id: &str) -> Option<DesktopApp> {
    let path = Path::new(id.strip_prefix(ID_PREFIX)?);
    let text = fs::read_to_string(path).ok()?;
    parse(path, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIMP: &str = "\
[Desktop Entry]
Type=Application
Name=GNU Image Manipulation Program
Name[pt_BR]=Programa de Manipulação de Imagens GNU
GenericName=Image Editor
Keywords=GIMP;graphic;
Icon=gimp
Exec=gimp-2.10 %U
Terminal=false

[Desktop Action new]
Name=Nova janela
Exec=gimp-2.10 --new
";

    #[test]
    fn le_nome_em_portugues_e_comando() {
        let app = parse(Path::new("/x/gimp.desktop"), GIMP).unwrap();
        assert_eq!(app.name, "Programa de Manipulação de Imagens GNU");
        assert_eq!(app.argv, ["gimp-2.10"]);
        assert_eq!(app.icon, "gimp");
        assert_eq!(app.id(), "desktop:/x/gimp.desktop");
        assert!(app.keywords.contains("Image Editor"));
        assert!(app.keywords.contains("graphic"));
        assert!(app.keywords.contains("gimp-2.10"));
    }

    #[test]
    fn ignora_ocultos_e_de_outros_ambientes() {
        let p = Path::new("/x/a.desktop");
        let base = "[Desktop Entry]\nType=Application\nName=A\nExec=a\n";
        assert!(parse(p, base).is_some());
        assert!(parse(p, &format!("{base}NoDisplay=true\n")).is_none());
        assert!(parse(p, &format!("{base}Hidden=true\n")).is_none());
        assert!(parse(p, &format!("{base}OnlyShowIn=GNOME;\n")).is_none());
        assert!(parse(p, "[Desktop Entry]\nType=Link\nName=A\nExec=a\n").is_none());
        assert!(parse(p, "[Desktop Entry]\nType=Application\nName=A\n").is_none());
        // O instalador do Debian some: o Kalango tem o próprio atalho.
        assert!(parse(p, "[Desktop Entry]\nType=Application\nName=I\nExec=install-debian\n").is_none());
    }

    #[test]
    fn app_de_terminal_abre_no_foot() {
        let app = parse(
            Path::new("/x/htop.desktop"),
            "[Desktop Entry]\nType=Application\nName=Htop\nExec=htop\nTerminal=true\n",
        )
        .unwrap();
        assert_eq!(app.command(), ["foot", "--title=Htop", "htop"]);
    }

    #[test]
    fn quebra_o_exec_em_argumentos() {
        assert_eq!(parse_exec("firefox-esr %u"), ["firefox-esr"]);
        assert_eq!(parse_exec("sh -c \"echo oi; read x\" %F"), ["sh", "-c", "echo oi; read x"]);
        assert_eq!(parse_exec("prog --taxa=50%%"), ["prog", "--taxa=50%"]);
        assert_eq!(parse_exec("\"/opt/meu app/bin\" --a"), ["/opt/meu app/bin", "--a"]);
        assert_eq!(parse_exec("/usr/bin/thunar"), ["/usr/bin/thunar"]);
    }

    #[test]
    fn programa_sem_pasta() {
        let app = parse(
            Path::new("/x/t.desktop"),
            "[Desktop Entry]\nType=Application\nName=T\nExec=/usr/bin/thunar %F\n",
        )
        .unwrap();
        assert_eq!(app.program(), "thunar");
    }
}
