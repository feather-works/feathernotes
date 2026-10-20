//! As codificações de texto: ler os bytes de um arquivo e gravá-los de volta sem perder nada.
//! O `encoding_rs` cobre as legadas (Windows-125x, ISO-8859, CJK…); UTF-16/32 ao gravar,
//! Latin-1 de verdade, ASCII e as páginas do DOS são feitas aqui, porque ele não faz.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cod {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
    Ascii,
    /// ISO-8859-1 de verdade: cada byte é o caractere de mesmo número (o `encoding_rs` troca por Windows-1252).
    Latin1,
    Cp437,
    Cp850,
    /// Uma do `encoding_rs`, pelo nome.
    Legado(&'static str),
}

/// As do menu principal, na ordem em que aparecem.
pub const PRINCIPAIS: &[Cod] = &[Cod::Utf8, Cod::Utf8Bom, Cod::Utf16Le, Cod::Utf16Be, Cod::Utf32Le, Cod::Utf32Be, Cod::Ascii, Cod::Legado("windows-1252"), Cod::Latin1, Cod::Legado("iso-8859-15")];

/// "Outras codificações".
pub const OUTRAS: &[Cod] = &[
    Cod::Cp850,
    Cod::Cp437,
    Cod::Legado("macintosh"),
    Cod::Legado("windows-1250"),
    Cod::Legado("windows-1251"),
    Cod::Legado("windows-1253"),
    Cod::Legado("windows-1254"),
    Cod::Legado("windows-1255"),
    Cod::Legado("windows-1256"),
    Cod::Legado("windows-1257"),
    Cod::Legado("windows-1258"),
    Cod::Legado("iso-8859-2"),
    Cod::Legado("koi8-r"),
    Cod::Legado("koi8-u"),
    Cod::Legado("shift_jis"),
    Cod::Legado("euc-jp"),
    Cod::Legado("iso-2022-jp"),
    Cod::Legado("gbk"),
    Cod::Legado("gb18030"),
    Cod::Legado("big5"),
    Cod::Legado("euc-kr"),
];

const CP437: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜ¢£¥₧ƒáíóúñÑªº¿⌐¬½¼¡«»░▒▓│┤╡╢╖╕╣║╗╝╜╛┐└┴┬├─┼╞╟╚╔╩╦╠═╬╧╨╤╥╙╘╒╓╫╪┘┌█▄▌▐▀αßΓπΣσµτΦΘΩδ∞φε∩≡±≥≤⌠⌡÷≈°∙·√ⁿ²■\u{a0}";
const CP850: &str = "ÇüéâäàåçêëèïîìÄÅÉæÆôöòûùÿÖÜø£Ø×ƒáíóúñÑªº¿®¬½¼¡«»░▒▓│┤ÁÂÀ©╣║╗╝¢¥┐└┴┬├─┼ãÃ╚╔╩╦╠═╬¤ðÐÊËÈıÍÎÏ┘┌█▄¦Ì▀ÓßÔÒõÕµþÞÚÛÙýÝ¯´\u{ad}±‗¾¶§÷¸°¨·¹³²■\u{a0}";

impl Cod {
    pub fn nome(self) -> &'static str {
        match self {
            Cod::Utf8 => "UTF-8",
            Cod::Utf8Bom => "UTF-8 com BOM",
            Cod::Utf16Le => "UTF-16 LE",
            Cod::Utf16Be => "UTF-16 BE",
            Cod::Utf32Le => "UTF-32 LE",
            Cod::Utf32Be => "UTF-32 BE",
            Cod::Ascii => "US-ASCII",
            Cod::Latin1 => "ISO-8859-1",
            Cod::Cp437 => "CP437 (DOS)",
            Cod::Cp850 => "CP850 (DOS)",
            Cod::Legado("macintosh") => "MacRoman",
            Cod::Legado("gb18030") => "GB18030",
            Cod::Legado(n) if n.starts_with("windows-") => match n {
                "windows-1250" => "Windows-1250",
                "windows-1251" => "Windows-1251",
                "windows-1252" => "Windows-1252",
                "windows-1253" => "Windows-1253",
                "windows-1254" => "Windows-1254",
                "windows-1255" => "Windows-1255",
                "windows-1256" => "Windows-1256",
                "windows-1257" => "Windows-1257",
                _ => "Windows-1258",
            },
            Cod::Legado(n) => encoding_rs::Encoding::for_label(n.as_bytes()).map_or(n, |e| e.name()),
        }
    }

    fn dos(self) -> Option<&'static str> {
        match self {
            Cod::Cp437 => Some(CP437),
            Cod::Cp850 => Some(CP850),
            _ => None,
        }
    }

    /// O texto dos bytes, e se algum byte não fazia sentido nesta codificação (virou �).
    pub fn ler(self, bytes: &[u8]) -> (String, bool) {
        match self {
            Cod::Utf8 | Cod::Utf8Bom => {
                let b = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
                let t = String::from_utf8_lossy(b);
                let ruim = matches!(t, std::borrow::Cow::Owned(_));
                (t.into_owned(), ruim)
            }
            Cod::Utf16Le | Cod::Utf16Be => {
                let b = bytes.strip_prefix(if self == Cod::Utf16Le { b"\xff\xfe" } else { b"\xfe\xff" }).unwrap_or(bytes);
                let u: Vec<u16> = b.chunks_exact(2).map(|p| if self == Cod::Utf16Le { u16::from_le_bytes([p[0], p[1]]) } else { u16::from_be_bytes([p[0], p[1]]) }).collect();
                let t: String = char::decode_utf16(u.iter().copied()).map(|r| r.unwrap_or('\u{fffd}')).collect();
                let ruim = t.contains('\u{fffd}') || b.len() % 2 != 0;
                (t, ruim)
            }
            Cod::Utf32Le | Cod::Utf32Be => {
                let b = bytes.strip_prefix(if self == Cod::Utf32Le { &b"\xff\xfe\0\0"[..] } else { &b"\0\0\xfe\xff"[..] }).unwrap_or(bytes);
                let mut ruim = b.len() % 4 != 0;
                let t = b
                    .chunks_exact(4)
                    .map(|p| {
                        let v = if self == Cod::Utf32Le { u32::from_le_bytes([p[0], p[1], p[2], p[3]]) } else { u32::from_be_bytes([p[0], p[1], p[2], p[3]]) };
                        char::from_u32(v).unwrap_or_else(|| {
                            ruim = true;
                            '\u{fffd}'
                        })
                    })
                    .collect();
                (t, ruim)
            }
            Cod::Ascii => (bytes.iter().map(|&b| if b < 128 { b as char } else { '\u{fffd}' }).collect(), bytes.iter().any(|&b| b >= 128)),
            Cod::Latin1 => (bytes.iter().map(|&b| b as char).collect(), false),
            Cod::Cp437 | Cod::Cp850 => {
                let alto: Vec<char> = self.dos().expect("página do DOS").chars().collect();
                (bytes.iter().map(|&b| if b < 128 { b as char } else { alto[b as usize - 128] }).collect(), false)
            }
            Cod::Legado(n) => {
                let e = encoding_rs::Encoding::for_label(n.as_bytes()).unwrap_or(encoding_rs::WINDOWS_1252);
                let (t, ruim) = e.decode_without_bom_handling(bytes);
                (t.into_owned(), ruim)
            }
        }
    }

    /// Os bytes do texto, ou os caracteres que não cabem nesta codificação (nada é trocado em silêncio).
    pub fn gravar(self, texto: &str) -> Result<Vec<u8>, Vec<char>> {
        let faltam = |cabe: &dyn Fn(char) -> bool| {
            let mut f: Vec<char> = texto.chars().filter(|&c| !cabe(c)).collect();
            f.dedup();
            f
        };
        let ok_ou = |bytes: Vec<u8>, f: Vec<char>| if f.is_empty() { Ok(bytes) } else { Err(f) };
        match self {
            Cod::Utf8 => Ok(texto.as_bytes().to_vec()),
            Cod::Utf8Bom => Ok([&b"\xef\xbb\xbf"[..], texto.as_bytes()].concat()),
            Cod::Utf16Le => Ok([0xff, 0xfe].into_iter().chain(texto.encode_utf16().flat_map(u16::to_le_bytes)).collect()),
            Cod::Utf16Be => Ok([0xfe, 0xff].into_iter().chain(texto.encode_utf16().flat_map(u16::to_be_bytes)).collect()),
            Cod::Utf32Le => Ok([0xff, 0xfe, 0, 0].into_iter().chain(texto.chars().flat_map(|c| (c as u32).to_le_bytes())).collect()),
            Cod::Utf32Be => Ok([0, 0, 0xfe, 0xff].into_iter().chain(texto.chars().flat_map(|c| (c as u32).to_be_bytes())).collect()),
            Cod::Ascii => ok_ou(texto.bytes().filter(|&b| b < 128).collect(), faltam(&|c| c.is_ascii())),
            Cod::Latin1 => ok_ou(texto.chars().filter(|&c| (c as u32) < 256).map(|c| c as u8).collect(), faltam(&|c| (c as u32) < 256)),
            Cod::Cp437 | Cod::Cp850 => {
                let alto = self.dos().expect("página do DOS");
                let byte = |c: char| if c.is_ascii() { Some(c as u8) } else { alto.chars().position(|a| a == c).map(|i| 128 + i as u8) };
                ok_ou(texto.chars().filter_map(byte).collect(), faltam(&|c| byte(c).is_some()))
            }
            Cod::Legado(n) => {
                let e = encoding_rs::Encoding::for_label(n.as_bytes()).unwrap_or(encoding_rs::WINDOWS_1252);
                let (bytes, _, ruim) = e.encode(texto);
                if !ruim {
                    return Ok(bytes.into_owned());
                }
                Err(faltam(&|c| !e.encode(c.encode_utf8(&mut [0; 4])).2))
            }
        }
    }
}

/// O palpite pela marca no começo (BOM), depois UTF-8 válido, depois o `chardetng`.
pub fn detectar(bytes: &[u8]) -> Cod {
    if bytes.starts_with(b"\xef\xbb\xbf") {
        return Cod::Utf8Bom;
    }
    if bytes.starts_with(b"\xff\xfe\0\0") {
        return Cod::Utf32Le;
    }
    if bytes.starts_with(b"\0\0\xfe\xff") {
        return Cod::Utf32Be;
    }
    if bytes.starts_with(b"\xff\xfe") {
        return Cod::Utf16Le;
    }
    if bytes.starts_with(b"\xfe\xff") {
        return Cod::Utf16Be;
    }
    if std::str::from_utf8(bytes).is_ok() {
        return Cod::Utf8;
    }
    let mut d = chardetng::EncodingDetector::new();
    d.feed(bytes, true);
    let e = d.guess(None, true);
    // O chardetng responde Windows-1252 para o que também é Latin-1; os outros nomes do encoding_rs são minúsculos.
    let rotulo: &'static str = [PRINCIPAIS, OUTRAS].concat().into_iter().find_map(|c| match c {
        Cod::Legado(n) if encoding_rs::Encoding::for_label(n.as_bytes()) == Some(e) => Some(n),
        _ => None,
    }).unwrap_or("windows-1252");
    Cod::Legado(rotulo)
}

/// "JoÃ£o": texto UTF-8 que já foi lido uma vez como Windows-1252 e gravado de novo. Devolve o conserto.
pub fn consertar_acentos(texto: &str) -> Option<String> {
    let suspeito = ["Ã¡", "Ã¢", "Ã£", "Ã§", "Ã©", "Ãª", "Ã\u{ad}", "Ã³", "Ã´", "Ãµ", "Ãº", "Ã‰", "Ã“", "â€"].iter().any(|p| texto.contains(p));
    if !suspeito {
        return None;
    }
    let (bytes, _, ruim) = encoding_rs::WINDOWS_1252.encode(texto);
    if ruim {
        return None;
    }
    String::from_utf8(bytes.into_owned()).ok()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn tabelas_do_dos_tem_128() {
        assert_eq!(CP437.chars().count(), 128);
        assert_eq!(CP850.chars().count(), 128);
    }

    #[test]
    fn ida_e_volta_sem_perder() {
        let texto = "João — ação € ✓\r\n";
        for c in [Cod::Utf8, Cod::Utf8Bom, Cod::Utf16Le, Cod::Utf16Be, Cod::Utf32Le, Cod::Utf32Be] {
            let bytes = c.gravar(texto).unwrap();
            assert_eq!(detectar(&bytes), c, "{c:?}");
            assert_eq!(c.ler(&bytes), (texto.to_string(), false), "{c:?}");
        }
        // Legadas: o que cabe volta igual; o que não cabe é recusado, não trocado.
        let pt = "João ação\r\n";
        for c in [Cod::Latin1, Cod::Cp850, Cod::Legado("windows-1252"), Cod::Legado("iso-8859-15")] {
            assert_eq!(c.ler(&c.gravar(pt).unwrap()).0, pt, "{c:?}");
        }
        assert_eq!(Cod::Latin1.gravar("€ e ✓"), Err(vec!['€', '✓']));
        assert_eq!(Cod::Ascii.gravar("ação"), Err(vec!['ç', 'ã']));
        assert_eq!(Cod::Legado("windows-1252").gravar("✓"), Err(vec!['✓']));
    }

    #[test]
    fn detecta_e_conserta() {
        let (bytes, _, _) = encoding_rs::WINDOWS_1251.encode("Привет, как дела? Это обычный русский текст для проверки.");
        assert_eq!(detectar(&bytes).nome(), "Windows-1251");
        assert_eq!(consertar_acentos("JoÃ£o e AÃ§Ã£o"), Some("João e Ação".into()));
        assert_eq!(consertar_acentos("João"), None);
    }
}
