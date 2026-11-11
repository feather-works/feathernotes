//! Transformar o texto selecionado: maiúsculas e minúsculas, e codificar ou decodificar Base64,
//! URL, HTML e os escapes de texto (`\n`, `\"`, `é`). Tudo aqui, sem mandar nada para um site.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Jeito {
    Maiusculas,
    Minusculas,
    Iniciais,
    Base64,
    Url,
    Html,
    Escapes,
}

impl Jeito {
    pub fn nome(self) -> &'static str {
        match self {
            Jeito::Maiusculas => "MAIÚSCULAS",
            Jeito::Minusculas => "minúsculas",
            Jeito::Iniciais => "Iniciais",
            Jeito::Base64 => "Base64",
            Jeito::Url => "URL",
            Jeito::Html => "HTML",
            Jeito::Escapes => "Escapes \\n",
        }
    }
}

/// O texto transformado; `decodificar` só vale para os de codificação. Erro: o motivo, para o rodapé.
pub fn aplicar(t: &str, jeito: Jeito, decodificar: bool) -> Result<String, &'static str> {
    match (jeito, decodificar) {
        (Jeito::Maiusculas, _) => Ok(t.to_uppercase()),
        (Jeito::Minusculas, _) => Ok(t.to_lowercase()),
        (Jeito::Iniciais, _) => Ok(iniciais(t)),
        (Jeito::Base64, false) => Ok(base64(t.as_bytes())),
        (Jeito::Base64, true) => String::from_utf8(de_base64(t).ok_or("a seleção não é Base64 válido")?).map_err(|_| "o Base64 não guarda um texto (são bytes de outro tipo de arquivo)"),
        (Jeito::Url, false) => Ok(t.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()),
        (Jeito::Url, true) => de_url(t),
        (Jeito::Html, false) => Ok(t.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")),
        (Jeito::Html, true) => Ok(de_html(t)),
        (Jeito::Escapes, false) => Ok(escapar(t)),
        (Jeito::Escapes, true) => de_escapes(t),
    }
}

fn iniciais(t: &str) -> String {
    let mut fora = true;
    t.chars()
        .flat_map(|c| {
            let novo: Vec<char> = if fora { c.to_uppercase().collect() } else { c.to_lowercase().collect() };
            // "d'água" e "bem-vindo": o apóstrofo e o hífen não começam palavra nova.
            fora = !(c.is_alphanumeric() || (!fora && matches!(c, '\'' | '’' | '-')));
            novo
        })
        .collect()
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for g in b.chunks(3) {
        let n = (u32::from(g[0]) << 16) | (u32::from(*g.get(1).unwrap_or(&0)) << 8) | u32::from(*g.get(2).unwrap_or(&0));
        for k in 0..4 {
            s.push(if k <= g.len() { B64[(n >> (18 - 6 * k)) as usize & 63] as char } else { '=' });
        }
    }
    s
}

/// Aceita também o alfabeto das URLs (`-_`, o do JWT), sem o `=` do fim e com quebras de linha no meio.
fn de_base64(t: &str) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut n = 0;
    let mut out = Vec::new();
    let sem_fim = t.trim().trim_end_matches('=');
    for c in sem_fim.bytes().filter(|c| !c.is_ascii_whitespace()) {
        let v = match c {
            b'-' => 62,
            b'_' => 63,
            _ => B64.iter().position(|&x| x == c)? as u32,
        };
        bits = (bits << 6) | v;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
        }
    }
    // Sobrar 6 bits é um caractere a mais que nenhum Base64 gera.
    (n < 6 && !sem_fim.is_empty()).then_some(out)
}

fn de_url(t: &str) -> Result<String, &'static str> {
    let b = t.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let hex = t.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()).ok_or("um % sem dois dígitos hexadecimais depois")?;
                out.push(hex);
                i += 3;
                continue;
            }
            // Nos formulários e nas buscas, o espaço vai como "+".
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8(out).map_err(|_| "os bytes decodificados não formam um texto UTF-8")
}

fn de_html(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut resto = t;
    while let Some(i) = resto.find('&') {
        out.push_str(&resto[..i]);
        resto = &resto[i..];
        // Até o ";" (no máximo 10 letras): "&amp;", "&#233;", "&#xe9;".
        let fim = resto.char_indices().take(12).find(|&(_, c)| c == ';').map(|(k, _)| k);
        let troca = fim.and_then(|f| {
            let nome = &resto[1..f];
            let c = match nome {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => '\u{a0}',
                _ => {
                    let num = nome.strip_prefix('#')?;
                    let v = match num.strip_prefix(['x', 'X']) {
                        Some(h) => u32::from_str_radix(h, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(v)?
                }
            };
            Some((c, f + 1))
        });
        match troca {
            Some((c, n)) => {
                out.push(c);
                resto = &resto[n..];
            }
            None => {
                out.push('&');
                resto = &resto[1..];
            }
        }
    }
    out + resto
}

fn escapar(t: &str) -> String {
    let mut s = String::with_capacity(t.len());
    for c in t.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if c.is_control() => s.push_str(&format!("\\u{:04x}", c as u32)),
            c => s.push(c),
        }
    }
    s
}

fn de_escapes(t: &str) -> Result<String, &'static str> {
    let mut out = String::with_capacity(t.len());
    let mut cs = t.chars();
    let hex4 = |cs: &mut std::str::Chars| -> Option<u32> { u32::from_str_radix(&cs.by_ref().take(4).collect::<String>(), 16).ok() };
    while let Some(c) = cs.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match cs.next().ok_or("uma \\ sozinha no fim")? {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            '0' => out.push('\0'),
            'u' => {
                let alto = hex4(&mut cs).ok_or("\\u sem quatro dígitos hexadecimais")?;
                // Emoji e outros fora do plano básico vêm em dois: "😀".
                let v = if (0xd800..0xdc00).contains(&alto) {
                    let baixo = (cs.next() == Some('\\') && cs.next() == Some('u')).then(|| hex4(&mut cs)).flatten().filter(|b| (0xdc00..0xe000).contains(b)).ok_or("\\u de emoji sem a segunda metade")?;
                    0x10000 + ((alto - 0xd800) << 10) + (baixo - 0xdc00)
                } else {
                    alto
                };
                out.push(char::from_u32(v).ok_or("\\u que não é um caractere")?);
            }
            // \" \\ \/ \' e qualquer outro: o próprio caractere.
            c => out.push(c),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ida_e_volta() {
        let t = "Olá, \"mundo\" & <b>café</b>\n\tação 😀 a+b=c/d?x=1";
        for j in [Jeito::Base64, Jeito::Url, Jeito::Html, Jeito::Escapes] {
            let ida = aplicar(t, j, false).unwrap();
            assert_eq!(aplicar(&ida, j, true).as_deref(), Ok(t), "{j:?}: {ida}");
        }
    }

    #[test]
    fn codifica_como_os_outros() {
        let a = |t, j| aplicar(t, j, false).unwrap();
        let d = |t, j| aplicar(t, j, true);
        assert_eq!([a("M", Jeito::Base64), a("Ma", Jeito::Base64), a("Man", Jeito::Base64)], ["TQ==", "TWE=", "TWFu"]);
        // JWT: alfabeto da URL, sem "=".
        assert_eq!(d("eyJhbGciOiJIUzI1NiJ9", Jeito::Base64).as_deref(), Ok("{\"alg\":\"HS256\"}"));
        assert_eq!(d("TWFu\nTWE", Jeito::Base64).as_deref(), Ok("ManMa"));
        assert!(d("não é", Jeito::Base64).is_err() && d("TWFuT", Jeito::Base64).is_err() && d("", Jeito::Base64).is_err());
        assert_eq!(a("ação 1+1", Jeito::Url), "a%C3%A7%C3%A3o%201%2B1");
        assert_eq!(d("busca?q=p%C3%A3o+de+queijo", Jeito::Url).as_deref(), Ok("busca?q=pão de queijo"));
        assert!(d("100%", Jeito::Url).is_err());
        assert_eq!(d("&lt;a&gt; &amp;amp; &#233;&#xE9; &nbsp;&copy; R&D", Jeito::Html).unwrap(), "<a> &amp; éé \u{a0}&copy; R&D");
        assert_eq!(d(r#"{\"a\":\"linha\\né 😀\"}"#, Jeito::Escapes).as_deref(), Ok("{\"a\":\"linha\\né 😀\"}"));
        assert!(d("fim \\", Jeito::Escapes).is_err() && d("\\ud83d sozinho", Jeito::Escapes).is_err());
    }

    #[test]
    fn maiusculas_e_iniciais() {
        assert_eq!(aplicar("ÁGUA de côco", Jeito::Minusculas, false).unwrap(), "água de côco");
        assert_eq!(aplicar("água", Jeito::Maiusculas, false).unwrap(), "ÁGUA");
        assert_eq!(aplicar("SÃO PAULO, d'água e bem-vindo", Jeito::Iniciais, false).unwrap(), "São Paulo, D'água E Bem-vindo");
    }
}
