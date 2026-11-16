//! Ferramentas de programador da Calculadora (aba Ferramentas), sem
//! dependências novas: Base64, URL, hashes (MD5, SHA-1, SHA-256), timestamp
//! Unix ↔ data, cores (hex ↔ RGB ↔ HSL), UUID v4 e JSON (formatar/compactar).
//! Os hashes seguem as RFCs 1321, 3174 e o FIPS 180-4 (testados com os
//! vetores oficiais).

use crate::common::i18n::tr;

// ---------------------------------------------------------------------------
// Base64 e URL
// ---------------------------------------------------------------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 padrão (RFC 4648, com `=`).
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodifica Base64 (aceita a variante de URL `-_`, sem `=` e com espaços).
pub fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let mut bits = 0u32;
    let mut n = 0;
    let mut out = Vec::new();
    for c in text.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            _ => return Err(format!("{} '{c}'", tr("Not Base64:", "Não é Base64:"))),
        };
        bits = bits << 6 | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
        }
    }
    Ok(out)
}

/// Codifica para URL (tudo menos `A-Z a-z 0-9 - _ . ~` vira `%XX`).
pub fn url_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Decodifica `%XX` e `+` (espaço).
pub fn url_decode(text: &str) -> Result<String, String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = text.get(i + 1..i + 3).ok_or_else(|| tr("Incomplete %XX", "%XX incompleto").to_string())?;
                out.push(u8::from_str_radix(hex, 16).map_err(|_| format!("{} %{hex}", tr("Invalid", "Inválido")))?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

// ---------------------------------------------------------------------------
// Hashes
// ---------------------------------------------------------------------------

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Mensagem com o enchimento de MD5/SHA (bit 1, zeros, tamanho em 64 bits).
fn padded(data: &[u8], little_endian: bool) -> Vec<u8> {
    let mut m = data.to_vec();
    m.push(0x80);
    while m.len() % 64 != 56 {
        m.push(0);
    }
    let bits = (data.len() as u64).wrapping_mul(8);
    m.extend_from_slice(&if little_endian { bits.to_le_bytes() } else { bits.to_be_bytes() });
    m
}

/// MD5 (RFC 1321). Inseguro para senhas; útil para conferir arquivos.
pub fn md5(data: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14,
        20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6,
        10, 15, 21,
    ];
    let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32).collect();
    let mut h = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
    for block in padded(data, true).chunks(64) {
        let w: Vec<u32> = block.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        let [mut a, mut b, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let t = d;
            d = c;
            c = b;
            b = b.wrapping_add(a.wrapping_add(f).wrapping_add(k[i]).wrapping_add(w[g]).rotate_left(S[i]));
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d]) {
            *x = x.wrapping_add(y);
        }
    }
    hex(&h.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>())
}

/// SHA-1 (RFC 3174). Inseguro para assinaturas; ainda usado pelo git.
pub fn sha1(data: &[u8]) -> String {
    let mut h = [0x67452301u32, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    for block in padded(data, false).chunks(64) {
        let mut w = [0u32; 80];
        for (i, c) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i / 20 {
                0 => ((b & c) | (!b & d), 0x5A827999),
                1 => (b ^ c ^ d, 0x6ED9EBA1),
                2 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    hex(&h.iter().flat_map(|x| x.to_be_bytes()).collect::<Vec<_>>())
}

/// SHA-256 (FIPS 180-4).
pub fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
        0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
        0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
        0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h = [
        0x6a09e667u32, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    for block in padded(data, false).chunks(64) {
        let mut w = [0u32; 64];
        for (i, c) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (x, y) in h.iter_mut().zip(v) {
            *x = x.wrapping_add(y);
        }
    }
    hex(&h.iter().flat_map(|x| x.to_be_bytes()).collect::<Vec<_>>())
}

// ---------------------------------------------------------------------------
// Tempo, cores, UUID
// ---------------------------------------------------------------------------

/// Timestamp Unix ↔ data. Vazio: agora. Número (segundos ou milissegundos):
/// a data local e UTC. Data ("2026-11-15 21:30[:05]" ou "2026-11-15"):
/// o timestamp (hora local).
pub fn timestamp(input: &str) -> Result<String, String> {
    use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone, Utc};
    let t = input.trim();
    let describe = |secs: i64| -> String {
        let local = Local.timestamp_opt(secs, 0).single();
        let utc = Utc.timestamp_opt(secs, 0).single();
        match (local, utc) {
            (Some(l), Some(u)) => format!(
                "{} {secs}\n{} {}\nUTC   {}\nISO   {}",
                tr("Unix", "Unix "),
                tr("Local", "Local"),
                l.format("%Y-%m-%d %H:%M:%S (%:z)"),
                u.format("%Y-%m-%d %H:%M:%S"),
                u.format("%Y-%m-%dT%H:%M:%SZ")
            ),
            _ => tr("Date out of range", "Data fora do intervalo").to_string(),
        }
    };
    if t.is_empty() {
        return Ok(describe(Utc::now().timestamp()));
    }
    if let Ok(n) = t.parse::<i64>() {
        // 13 algarismos: milissegundos (JavaScript).
        return Ok(describe(if n.abs() >= 100_000_000_000 { n / 1000 } else { n }));
    }
    let dt = ["%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S", "%d/%m/%Y %H:%M:%S", "%d/%m/%Y %H:%M"]
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(t, f).ok())
        .or_else(|| {
            ["%Y-%m-%d", "%d/%m/%Y"]
                .iter()
                .find_map(|f| NaiveDate::parse_from_str(t, f).ok())
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })
        .ok_or_else(|| {
            tr(
                "Use a number (Unix time) or a date like 2026-11-15 21:30",
                "Use um número (tempo Unix) ou uma data como 2026-11-15 21:30 ou 15/11/2026",
            )
            .to_string()
        })?;
    let local = Local.from_local_datetime(&dt).earliest().ok_or_else(|| tr("Invalid local time", "Hora local inválida").to_string())?;
    Ok(describe(local.timestamp()))
}

/// Cor em `#rgb`, `#rrggbb`, `rgb(r, g, b)` ou `r g b` → as outras formas.
pub fn color(input: &str) -> Result<(String, [u8; 3]), String> {
    let t = input.trim().to_lowercase();
    let bad = || tr("Use #3dd6f5, #fff or rgb(61, 214, 245)", "Use #3dd6f5, #fff ou rgb(61, 214, 245)").to_string();
    let rgb: [u8; 3] = if let Some(h) = t.strip_prefix('#').or_else(|| t.chars().all(|c| c.is_ascii_hexdigit()).then_some(t.as_str())) {
        let h: String = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
        if h.len() != 6 {
            return Err(bad());
        }
        let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| bad());
        [p(0)?, p(2)?, p(4)?]
    } else {
        let nums: Vec<u8> = t
            .trim_start_matches("rgb")
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .map(|s| s.parse::<u8>().map_err(|_| bad()))
            .collect::<Result<_, _>>()?;
        <[u8; 3]>::try_from(&nums[..]).map_err(|_| bad())?
    };
    let [r, g, b] = rgb.map(|v| v as f64 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    let (h, s) = if d == 0.0 {
        (0.0, 0.0)
    } else {
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r {
            60.0 * ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        (h, s)
    };
    let text = format!(
        "HEX  #{:02x}{:02x}{:02x}\nRGB  rgb({}, {}, {})\nHSL  hsl({:.0}, {:.0}%, {:.0}%)\nSlint/CSS  #{:02x}{:02x}{:02x}ff",
        rgb[0],
        rgb[1],
        rgb[2],
        rgb[0],
        rgb[1],
        rgb[2],
        h,
        s * 100.0,
        l * 100.0,
        rgb[0],
        rgb[1],
        rgb[2]
    );
    Ok((text, rgb))
}

/// UUID versão 4 (aleatório, de `/dev/urandom`).
pub fn uuid_v4() -> String {
    use std::io::Read;
    let mut b = [0u8; 16];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).is_err() {
        // Sem /dev/urandom (não deve acontecer no Linux): relógio + endereço.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        b = (t ^ (&b as *const _ as usize as u128)).to_le_bytes();
    }
    b[6] = b[6] & 0x0f | 0x40;
    b[8] = b[8] & 0x3f | 0x80;
    let h = hex(&b);
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

// ---------------------------------------------------------------------------
// JSON
// ---------------------------------------------------------------------------

/// Confere o JSON e devolve formatado (`indent` espaços) ou compacto (`0`).
/// O erro diz a linha e a coluna.
pub fn json_format(input: &str, indent: usize) -> Result<String, String> {
    let s: Vec<char> = input.chars().collect();
    let mut p = Json { s: &s, i: 0, out: String::new(), indent, depth: 0 };
    p.ws();
    p.value()?;
    p.ws();
    if p.i < s.len() {
        return Err(p.error(tr("Extra text after the JSON", "Texto sobrando depois do JSON")));
    }
    Ok(p.out)
}

struct Json<'a> {
    s: &'a [char],
    i: usize,
    out: String,
    indent: usize,
    depth: usize,
}

impl Json<'_> {
    fn error(&self, msg: &str) -> String {
        let before = &self.s[..self.i.min(self.s.len())];
        let line = before.iter().filter(|c| **c == '\n').count() + 1;
        let col = before.iter().rev().take_while(|c| **c != '\n').count() + 1;
        format!("{msg} ({} {line}, {} {col})", tr("line", "linha"), tr("column", "coluna"))
    }
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_whitespace() {
            self.i += 1;
        }
    }
    fn newline(&mut self) {
        if self.indent > 0 {
            self.out.push('\n');
            self.out.push_str(&" ".repeat(self.indent * self.depth));
        }
    }
    fn value(&mut self) -> Result<(), String> {
        match self.s.get(self.i) {
            Some('{') => self.container('{', '}', true),
            Some('[') => self.container('[', ']', false),
            Some('"') => self.string(),
            Some(c) if *c == '-' || c.is_ascii_digit() => {
                let start = self.i;
                while self.i < self.s.len() && matches!(self.s[self.i], '0'..='9' | '-' | '+' | '.' | 'e' | 'E') {
                    self.i += 1;
                }
                let n: String = self.s[start..self.i].iter().collect();
                if n.parse::<f64>().is_err() {
                    self.i = start;
                    return Err(self.error(tr("Invalid number", "Número inválido")));
                }
                self.out.push_str(&n);
                Ok(())
            }
            _ => {
                for word in ["true", "false", "null"] {
                    let w: Vec<char> = word.chars().collect();
                    if self.s[self.i..].starts_with(&w) {
                        self.i += w.len();
                        self.out.push_str(word);
                        return Ok(());
                    }
                }
                Err(self.error(tr("Expected a value", "Esperava um valor")))
            }
        }
    }
    fn string(&mut self) -> Result<(), String> {
        self.out.push('"');
        self.i += 1;
        while let Some(&c) = self.s.get(self.i) {
            self.i += 1;
            self.out.push(c);
            match c {
                '"' => return Ok(()),
                '\\' => {
                    if let Some(&n) = self.s.get(self.i) {
                        self.out.push(n);
                        self.i += 1;
                    }
                }
                '\n' => return Err(self.error(tr("Line break inside a string", "Quebra de linha dentro de texto"))),
                _ => {}
            }
        }
        Err(self.error(tr("Unclosed string", "Texto sem aspas de fechamento")))
    }
    fn container(&mut self, open: char, close: char, object: bool) -> Result<(), String> {
        self.out.push(open);
        self.i += 1;
        self.ws();
        if self.s.get(self.i) == Some(&close) {
            self.i += 1;
            self.out.push(close);
            return Ok(());
        }
        self.depth += 1;
        loop {
            self.newline();
            if object {
                if self.s.get(self.i) != Some(&'"') {
                    return Err(self.error(tr("Expected a \"key\"", "Esperava uma \"chave\"")));
                }
                self.string()?;
                self.ws();
                if self.s.get(self.i) != Some(&':') {
                    return Err(self.error(tr("Expected ':'", "Esperava ':'")));
                }
                self.i += 1;
                self.out.push_str(if self.indent > 0 { ": " } else { ":" });
                self.ws();
            }
            self.value()?;
            self.ws();
            match self.s.get(self.i) {
                Some(',') => {
                    self.i += 1;
                    self.out.push(',');
                    self.ws();
                }
                Some(c) if *c == close => {
                    self.i += 1;
                    self.depth -= 1;
                    self.newline();
                    self.out.push(close);
                    return Ok(());
                }
                _ => return Err(self.error(&format!("{} ',' {} '{close}'", tr("Expected", "Esperava"), tr("or", "ou")))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_e_url() {
        for (t, e) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64_encode(t.as_bytes()), e);
            assert_eq!(base64_decode(e).unwrap(), t.as_bytes());
        }
        assert_eq!(base64_decode("Zm9v YmFy").unwrap(), b"foobar");
        assert!(base64_decode("Zm9v!").is_err());
        assert_eq!(url_encode("a b/ç?"), "a%20b%2F%C3%A7%3F");
        assert_eq!(url_decode("a%20b+c%C3%A7").unwrap(), "a b cç");
    }

    #[test]
    fn hashes_com_vetores_oficiais() {
        assert_eq!(md5(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(md5(b"The quick brown fox jumps over the lazy dog"), "9e107d9d372bb6826bd81d3542a419d6");
        assert_eq!(sha1(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(sha1(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(sha256(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(sha256(long), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
        assert_eq!(sha1(long), "84983e441c3bd26ebaae4aa1f95129e5e54670f1");
    }

    #[test]
    fn cores_uuid_json() {
        let (t, rgb) = color("#3dd6f5").unwrap();
        assert_eq!(rgb, [61, 214, 245]);
        assert!(t.contains("rgb(61, 214, 245)") && t.contains("hsl(190"));
        assert_eq!(color("rgb(255, 0, 0)").unwrap().1, [255, 0, 0]);
        assert_eq!(color("#fff").unwrap().1, [255, 255, 255]);
        assert!(color("azul").is_err());
        let u = uuid_v4();
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert_ne!(uuid_v4(), u);
        assert_eq!(json_format(r#"{"a":[1,2,{"b":null}],"c":"x"}"#, 0).unwrap(), r#"{"a":[1,2,{"b":null}],"c":"x"}"#);
        assert_eq!(json_format(r#"{"a": [1, 2]}"#, 2).unwrap(), "{\n  \"a\": [\n    1,\n    2\n  ]\n}");
        assert_eq!(json_format("[]", 2).unwrap(), "[]");
        let e = json_format("{\n  \"a\": 1,\n  \"b\" 2\n}", 2).unwrap_err();
        assert!(e.contains("3"), "{e}");
        assert!(timestamp("0").unwrap().contains("1970-01-01 00:00:00"));
        assert!(timestamp("1700000000000").unwrap().contains("2023-11-14"));
        assert!(timestamp("2026-11-15 21:30").is_ok() && timestamp("15/11/2026").is_ok());
        assert!(timestamp("ontem").is_err());
    }
}
