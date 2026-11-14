//! Pendrives: grava logo o que foi copiado e avisa na barra enquanto grava.
//!
//! O Linux guarda na memória o que é copiado para um disco e só depois grava
//! de verdade. A janela de cópia termina antes da gravação e quem tira o
//! pendrive nessa hora perde o arquivo. Duas defesas, além da regra do udev
//! (`etc/udev/rules.d/90-kalango-pendrive.rules`, que limita a 16 MB o que fica
//! na memória por pendrive, para o progresso da cópia andar na velocidade real):
//!
//! - a cada [`INTERVAL`], cada sistema de arquivos de disco USB montado recebe um
//!   `sync -f` (syncfs): o que estava na memória vai para o pendrive em até
//!   2 s, mesmo no exFAT/ext4/NTFS, que senão esperariam até 30 s;
//! - enquanto há gravação no pendrive (setores escritos mudando, pedidos em
//!   andamento ou o `sync` demorando), `Shell.usb-writing` mostra o aviso
//!   no centro da barra; ao terminar, um aviso curto diz que acabou.
//!
//! Só roda na área de trabalho da sessão real (é ela que tem a barra).

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use slint::ComponentHandle;

use crate::shell::session::Mode;
use crate::{i18n, AppWindow, Shell};

/// Intervalo entre as verificações (e entre os `sync`).
const INTERVAL: Duration = Duration::from_secs(2);

/// Um `sync` mais lento que isto gravou alguma coisa.
const SLOW_SYNC: Duration = Duration::from_millis(300);

/// Um sistema de arquivos montado de um disco USB.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbMount {
    /// Disco inteiro ("sdb"), onde ficam os contadores de gravação.
    pub disk: String,
    /// Onde está montado ("/media/user/PENDRIVE").
    pub mountpoint: String,
}

/// Desfaz os escapes octais de `/proc/self/mounts` (`\040` = espaço).
pub fn unescape_mount_field(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let code = &bytes[i + 1..i + 4];
            if code.iter().all(|b| (b'0'..=b'7').contains(b)) {
                out.push((code[0] - b'0') * 64 + (code[1] - b'0') * 8 + (code[2] - b'0'));
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Dispositivos `/dev/...` montados e onde, a partir de `/proc/self/mounts`.
pub fn parse_mounts(content: &str) -> Vec<(String, String)> {
    content
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let device = fields.next()?;
            let mountpoint = fields.next()?;
            device
                .starts_with("/dev/")
                .then(|| (device.to_string(), unescape_mount_field(mountpoint)))
        })
        .collect()
}

/// Setores escritos e pedidos em andamento, de `/sys/block/<disco>/stat`.
///
/// Campos: leituras, leituras juntadas, setores lidos, ms lendo, escritas,
/// escritas juntadas, **setores escritos** (7º), ms escrevendo, **em
/// andamento** (9º)...
pub fn parse_block_stat(content: &str) -> Option<(u64, u64)> {
    let values: Vec<u64> = content
        .split_whitespace()
        .map(|v| v.parse().ok())
        .collect::<Option<_>>()?;
    Some((*values.get(6)?, *values.get(8)?))
}

/// Disco inteiro de um dispositivo ("/dev/sdb1" → "sdb"), se for de USB.
///
/// A partição aparece em `/sys/class/block/sdb1` (com o arquivo `partition`)
/// dentro da pasta do disco; o disco é USB se o caminho real passa por um
/// controlador USB.
fn usb_disk_of(device: &str) -> Option<String> {
    let name = Path::new(device).file_name()?.to_str()?;
    let real = fs::canonicalize(Path::new("/sys/class/block").join(name)).ok()?;
    let disk_dir = if real.join("partition").exists() { real.parent()?.to_path_buf() } else { real };
    let path = disk_dir.to_string_lossy();
    path.contains("/usb").then(|| disk_dir.file_name()?.to_str().map(str::to_string))?
}

/// Sistemas de arquivos de discos USB montados agora.
fn usb_mounts() -> Vec<UsbMount> {
    let content = fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    parse_mounts(&content)
        .into_iter()
        .filter_map(|(device, mountpoint)| {
            Some(UsbMount { disk: usb_disk_of(&device)?, mountpoint })
        })
        .collect()
}

/// Contadores de gravação de cada disco.
fn read_stats(mounts: &[UsbMount]) -> HashMap<String, (u64, u64)> {
    mounts
        .iter()
        .filter_map(|m| {
            let content = fs::read_to_string(format!("/sys/block/{}/stat", m.disk)).ok()?;
            Some((m.disk.clone(), parse_block_stat(&content)?))
        })
        .collect()
}

/// Grava no pendrive o que ainda está na memória; devolve quanto demorou.
fn sync_mount(mountpoint: &str) -> Duration {
    let start = Instant::now();
    let _ = Command::new("sync")
        .arg("-f")
        .arg(mountpoint)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    start.elapsed()
}

/// Houve gravação desde a última verificação: setores escritos mudaram ou há
/// pedidos em andamento.
pub fn disk_busy(before: Option<&(u64, u64)>, now: &(u64, u64)) -> bool {
    now.1 > 0 || before.is_some_and(|b| b.0 != now.0)
}

/// Passa o estado para a barra (e avisa quando terminou de gravar).
fn publish(weak: &slint::Weak<AppWindow>, writing: bool) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(ui) = weak.upgrade() else { return };
        ui.global::<Shell>().set_usb_writing(writing);
        if !writing {
            ui.invoke_show_toast(
                i18n::tr("Done writing to the USB drive", "Gravação no pendrive concluída").into(),
            );
        }
    });
}

/// Começa a vigiar os pendrives (só a área de trabalho da sessão real).
pub fn install(ui: &AppWindow, mode: Mode) {
    if mode != Mode::Desktop || !crate::shell::session::is_native() || !cfg!(target_os = "linux") {
        return;
    }
    let weak = ui.as_weak();
    thread::spawn(move || {
        let mut last: HashMap<String, (u64, u64)> = HashMap::new();
        let mut writing = false;
        loop {
            thread::sleep(INTERVAL);
            let mounts = usb_mounts();
            if mounts.is_empty() {
                // Pendrive removido no meio da gravação: só some o aviso.
                if writing {
                    writing = false;
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak.upgrade() {
                            ui.global::<Shell>().set_usb_writing(false);
                        }
                    });
                }
                last.clear();
                continue;
            }

            // Gravação vista antes do sync: o aviso aparece já, porque o sync
            // pode demorar (cópia grande em andamento).
            let before = read_stats(&mounts);
            let mut busy = before.iter().any(|(disk, now)| disk_busy(last.get(disk), now));
            if busy && !writing {
                writing = true;
                publish(&weak, true);
            }
            for mount in &mounts {
                if sync_mount(&mount.mountpoint) > SLOW_SYNC {
                    busy = true;
                }
            }
            // O que o próprio sync gravou também conta (arquivo pequeno que
            // estava só na memória).
            let after = read_stats(&mounts);
            busy |= after.iter().any(|(disk, now)| disk_busy(before.get(disk), now));
            last = after;

            if busy != writing {
                writing = busy;
                publish(&weak, busy);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_montagens_com_espaco_no_nome() {
        let content = "sysfs /sys sysfs rw 0 0\n\
            /dev/sda2 / ext4 rw 0 0\n\
            /dev/sdb1 /media/user/MEU\\040PEN vfat rw,flush 0 0\n";
        assert_eq!(
            parse_mounts(content),
            vec![
                ("/dev/sda2".to_string(), "/".to_string()),
                ("/dev/sdb1".to_string(), "/media/user/MEU PEN".to_string()),
            ]
        );
    }

    #[test]
    fn le_setores_escritos_e_pedidos_em_andamento() {
        let stat = "  120 3 4000 50   80 10 16384 900    2  700 950 0 0 0 0 4 12\n";
        assert_eq!(parse_block_stat(stat), Some((16384, 2)));
        assert_eq!(parse_block_stat("1 2 3"), None);
    }

    #[test]
    fn gravando_quando_muda_ou_tem_pedido() {
        assert!(disk_busy(Some(&(100, 0)), &(200, 0)));
        assert!(disk_busy(None, &(100, 1)));
        assert!(!disk_busy(Some(&(100, 0)), &(100, 0)));
        // Primeira leitura sem pedidos: nada a dizer ainda.
        assert!(!disk_busy(None, &(100, 0)));
    }
}
