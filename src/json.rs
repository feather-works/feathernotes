//! JSON: validar (o erro com linha e coluna), formatar, compactar e ordenar as chaves.
//! Números e textos saem como foram escritos: sem perder precisão nem trocar os escapes.

/// Onde o JSON quebrou, contando de 1.
#[derive(Debug, PartialEq)]
pub struct Erro {
    pub linha: usize,
    pub coluna: usize,
    pub motivo: &'static str,
}

enum Valor<'a> {
    /// As chaves guardam as aspas: ordenar pelo texto cru basta.
    Objeto(Vec<(&'a str, Valor<'a>)>),
    Lista(Vec<Valor<'a>>),
    Simples(&'a str),
}

/// Quantos `{` e `[` um dentro do outro: um `[[[[…` sem fim não derruba o programa.
const FUNDO_MAXIMO: usize = 500;

struct Leitor<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Leitor<'a> {
    fn erro<T>(&self, motivo: &'static str) -> Result<T, Erro> {
        let antes = &self.s[..self.i.min(self.s.len())];
        let inicio = antes.rfind('\n').map_or(0, |n| n + 1);
        Err(Erro { linha: antes.matches('\n').count() + 1, coluna: antes[inicio..].chars().count() + 1, motivo })
    }
    fn olhar(&self) -> Option<u8> {
        self.s.as_bytes().get(self.i).copied()
    }
    fn espacos(&mut self) {
        while matches!(self.olhar(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }
    fn digitos(&mut self) -> usize {
        let desde = self.i;
        while self.olhar().is_some_and(|b| b.is_ascii_digit()) {
            self.i += 1;
        }
        self.i - desde
    }
    fn valor(&mut self, fundo: usize) -> Result<Valor<'a>, Erro> {
        self.espacos();
        let desde = self.i;
        match self.olhar() {
            Some(b'{' | b'[') if fundo >= FUNDO_MAXIMO => self.erro("aninhado demais"),
            Some(b'{') => self.objeto(fundo),
            Some(b'[') => self.lista(fundo),
            Some(b'"') => self.texto().map(Valor::Simples),
            Some(b'-' | b'0'..=b'9') => {
                if self.olhar() == Some(b'-') {
                    self.i += 1;
                }
                match self.olhar() {
                    Some(b'0') => self.i += 1,
                    Some(b'1'..=b'9') => {
                        self.digitos();
                    }
                    _ => return self.erro("número inválido"),
                }
                if self.olhar() == Some(b'.') {
                    self.i += 1;
                    if self.digitos() == 0 {
                        return self.erro("faltam dígitos depois do ponto");
                    }
                }
                if matches!(self.olhar(), Some(b'e' | b'E')) {
                    self.i += 1;
                    if matches!(self.olhar(), Some(b'+' | b'-')) {
                        self.i += 1;
                    }
                    if self.digitos() == 0 {
                        return self.erro("faltam dígitos no expoente");
                    }
                }
                Ok(Valor::Simples(&self.s[desde..self.i]))
            }
            _ => {
                for palavra in ["true", "false", "null"] {
                    if self.s[self.i..].starts_with(palavra) {
                        self.i += palavra.len();
                        return Ok(Valor::Simples(palavra));
                    }
                }
                self.erro(if self.olhar().is_none() { "o JSON acabou antes da hora" } else { "esperava um valor (texto entre aspas duplas, número, true, false, null, { ou [)" })
            }
        }
    }
    fn texto(&mut self) -> Result<&'a str, Erro> {
        let desde = self.i;
        self.i += 1;
        loop {
            match self.olhar() {
                None => return self.erro("texto sem as aspas de fechamento"),
                Some(b'"') => break,
                Some(b'\\') => {
                    self.i += 1;
                    match self.olhar() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {}
                        Some(b'u') if self.s.as_bytes().get(self.i + 1..self.i + 5).is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit)) => self.i += 4,
                        _ => return self.erro("escape inválido depois da \\"),
                    }
                }
                Some(b) if b < 0x20 => return self.erro("quebra de linha ou tab dentro do texto (use \\n ou \\t)"),
                Some(_) => {}
            }
            self.i += 1;
        }
        self.i += 1;
        Ok(&self.s[desde..self.i])
    }
    /// Depois de um item: `,` segue, `fim` fecha. Devolve se fechou.
    fn separador(&mut self, fim: u8, motivo: &'static str) -> Result<bool, Erro> {
        self.espacos();
        match self.olhar() {
            Some(b',') => {
                self.i += 1;
                self.espacos();
                if self.olhar() == Some(fim) {
                    return self.erro("vírgula sobrando antes de fechar");
                }
                Ok(false)
            }
            Some(b) if b == fim => {
                self.i += 1;
                Ok(true)
            }
            _ => self.erro(motivo),
        }
    }
    fn objeto(&mut self, fundo: usize) -> Result<Valor<'a>, Erro> {
        self.i += 1;
        let mut itens = Vec::new();
        self.espacos();
        if self.olhar() == Some(b'}') {
            self.i += 1;
            return Ok(Valor::Objeto(itens));
        }
        loop {
            self.espacos();
            if self.olhar() != Some(b'"') {
                return self.erro("esperava o nome da chave entre aspas duplas");
            }
            let chave = self.texto()?;
            self.espacos();
            if self.olhar() != Some(b':') {
                return self.erro("esperava ':' depois da chave");
            }
            self.i += 1;
            itens.push((chave, self.valor(fundo + 1)?));
            if self.separador(b'}', "esperava ',' ou '}'")? {
                return Ok(Valor::Objeto(itens));
            }
        }
    }
    fn lista(&mut self, fundo: usize) -> Result<Valor<'a>, Erro> {
        self.i += 1;
        let mut itens = Vec::new();
        self.espacos();
        if self.olhar() == Some(b']') {
            self.i += 1;
            return Ok(Valor::Lista(itens));
        }
        loop {
            itens.push(self.valor(fundo + 1)?);
            if self.separador(b']', "esperava ',' ou ']'")? {
                return Ok(Valor::Lista(itens));
            }
        }
    }
}

fn ler(s: &str) -> Result<Valor<'_>, Erro> {
    let mut l = Leitor { s, i: 0 };
    let v = l.valor(0)?;
    l.espacos();
    if l.olhar().is_some() {
        return l.erro("sobrou texto depois do fim do JSON");
    }
    Ok(v)
}

pub fn validar(s: &str) -> Result<(), Erro> {
    ler(s).map(|_| ())
}

/// Reescreve com 2 espaços de recuo (ou tudo numa linha, `compacto`); `ordenar` põe as chaves em ordem alfabética.
pub fn reescrever(s: &str, compacto: bool, ordenar: bool) -> Result<String, Erro> {
    fn escrever(v: &mut Valor, compacto: bool, ordenar: bool, nivel: usize, out: &mut String) {
        let quebra = |out: &mut String, nivel: usize| {
            if !compacto {
                out.push('\n');
                out.push_str(&"  ".repeat(nivel));
            }
        };
        match v {
            Valor::Simples(t) => out.push_str(t),
            Valor::Lista(itens) if itens.is_empty() => out.push_str("[]"),
            Valor::Objeto(itens) if itens.is_empty() => out.push_str("{}"),
            Valor::Lista(itens) => {
                out.push('[');
                for (k, item) in itens.iter_mut().enumerate() {
                    out.push_str(if k > 0 { "," } else { "" });
                    quebra(out, nivel + 1);
                    escrever(item, compacto, ordenar, nivel + 1, out);
                }
                quebra(out, nivel);
                out.push(']');
            }
            Valor::Objeto(itens) => {
                if ordenar {
                    itens.sort_by(|a, b| a.0.cmp(b.0));
                }
                out.push('{');
                for (k, (chave, item)) in itens.iter_mut().enumerate() {
                    out.push_str(if k > 0 { "," } else { "" });
                    quebra(out, nivel + 1);
                    out.push_str(chave);
                    out.push_str(if compacto { ":" } else { ": " });
                    escrever(item, compacto, ordenar, nivel + 1, out);
                }
                quebra(out, nivel);
                out.push('}');
            }
        }
    }
    let mut v = ler(s)?;
    let mut out = String::with_capacity(s.len());
    escrever(&mut v, compacto, ordenar, 0, &mut out);
    if !compacto {
        out.push('\n');
    }
    Ok(out)
}

/// O papel de cada trecho de uma linha, para as cores: (bytes, papel). Texto seguido de `:` é chave.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Papel {
    Chave,
    Valor,
    Sinal,
}

/// Linha por linha basta: num JSON válido um texto não passa de uma linha.
pub fn papeis(linha: &str) -> Vec<(std::ops::Range<usize>, Papel)> {
    let b = linha.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let desde = i;
        match b[i] {
            b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                i += 1;
                out.push((desde..i, Papel::Sinal));
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(b.len());
                let chave = linha[i..].trim_start().starts_with(':');
                out.push((desde..i, if chave { Papel::Chave } else { Papel::Valor }));
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                while i < b.len() && !matches!(b[i], b'{' | b'}' | b'[' | b']' | b':' | b',' | b'"') && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                out.push((desde..i, Papel::Valor));
            }
        }
    }
    out
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn formata_compacta_e_ordena() {
        let s = r#"{"b": [1, 2.5e-3, {}], "a": {"x\"y": null, "c": true}, "z": []}"#;
        assert_eq!(reescrever(s, true, false).unwrap(), r#"{"b":[1,2.5e-3,{}],"a":{"x\"y":null,"c":true},"z":[]}"#);
        assert_eq!(reescrever(s, false, true).unwrap(), "{\n  \"a\": {\n    \"c\": true,\n    \"x\\\"y\": null\n  },\n  \"b\": [\n    1,\n    2.5e-3,\n    {}\n  ],\n  \"z\": []\n}\n");
        // Número grande demais para um f64 continua igual.
        assert_eq!(reescrever("[12345678901234567890123]", true, false).unwrap(), "[12345678901234567890123]");
    }

    #[test]
    fn papeis_da_linha() {
        use Papel::*;
        assert_eq!(papeis(r#"  "a\"b": [1, "x"],"#), vec![(2..8, Chave), (8..9, Sinal), (10..11, Sinal), (11..12, Valor), (12..13, Sinal), (14..17, Valor), (17..18, Sinal), (18..19, Sinal)]);
        // Texto sem fechar não passa do fim da linha.
        assert_eq!(papeis(r#""ab\"#), vec![(0..4, Valor)]);
    }

    #[test]
    fn erro_com_linha_e_coluna() {
        let e = |s: &str| validar(s).unwrap_err();
        assert_eq!(e("{\n  \"a\": 1,\n}"), Erro { linha: 3, coluna: 1, motivo: "vírgula sobrando antes de fechar" });
        assert_eq!(e("{\"a\" 1}").motivo, "esperava ':' depois da chave");
        assert_eq!(e("{'a': 1}").motivo, "esperava o nome da chave entre aspas duplas");
        assert_eq!(e("[01]").motivo, "esperava ',' ou ']'");
        assert_eq!(e("[1] x").motivo, "sobrou texto depois do fim do JSON");
        assert_eq!(e("\"ação\nx\"").coluna, 6);
        assert_eq!(e("").motivo, "o JSON acabou antes da hora");
        assert!(validar(&"[".repeat(100_000)).is_err());
        // O limite: 500 níveis abrem, formatam e compactam; 501 dá erro claro, sem derrubar.
        let fundo = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
        let f = reescrever(&fundo(500), false, true).unwrap();
        assert_eq!((f.lines().count(), reescrever(&f, true, false).unwrap()), (999, fundo(500)));
        let objetos = |n: usize| format!("{}1{}", "{\"a\":".repeat(n), "}".repeat(n));
        assert!(validar(&objetos(500)).is_ok());
        assert_eq!(e(&objetos(501)).motivo, "aninhado demais");
        assert_eq!(e(&fundo(501)), Erro { linha: 1, coluna: 501, motivo: "aninhado demais" });
        assert!(validar(" {\"a\": [true, false, null, -0.5E+2, \"\\u00e9\"]} ").is_ok());
    }
}
