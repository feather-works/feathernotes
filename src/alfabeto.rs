//! Links falsos (phishing): palavras e domínios com letras que se passam por outras, como `pаypal`
//! com um "а" cirílico ou `аррӏе.com` todo em cirílico. As letras parecidas vêm da lista oficial
//! do Unicode (TR39, `confusables.txt`).

use std::ops::Range;
use unicode_security::skeleton;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Alfabeto {
    Latino,
    Cirilico,
    Grego,
}

impl Alfabeto {
    fn nome(self) -> &'static str {
        match self {
            Alfabeto::Latino => "latino",
            Alfabeto::Cirilico => "cirílico",
            Alfabeto::Grego => "grego",
        }
    }
}

/// O alfabeto da letra; dígitos, pontuação e os outros alfabetos ficam de fora.
pub fn alfabeto(c: char) -> Option<Alfabeto> {
    if !c.is_alphabetic() {
        return None;
    }
    match c {
        'a'..='z' | 'A'..='Z' | 'ª' | 'º' => Some(Alfabeto::Latino),
        '\u{c0}'..='\u{24f}' | '\u{1e00}'..='\u{1eff}' | '\u{ff21}'..='\u{ff3a}' | '\u{ff41}'..='\u{ff5a}' => Some(Alfabeto::Latino),
        '\u{370}'..='\u{3ff}' | '\u{1f00}'..='\u{1fff}' => Some(Alfabeto::Grego),
        '\u{400}'..='\u{52f}' | '\u{1c80}'..='\u{1c8f}' | '\u{2de0}'..='\u{2dff}' | '\u{a640}'..='\u{a69f}' => Some(Alfabeto::Cirilico),
        _ => None,
    }
}

/// Texto só com ASCII e os acentos do latim (português, espanhol, francês…) não tem o que
/// procurar: conta sem copiar o texto.
pub fn vale_procurar(c: char) -> bool {
    !c.is_ascii() && c.is_alphanumeric() && !('\u{c0}'..='\u{ff}').contains(&c)
}

/// A letra ASCII que esta imita ("а" cirílico → "a"); `None` se já é ASCII ou não imita nenhuma.
fn imita(c: char) -> Option<String> {
    if c.is_ascii() || !c.is_alphanumeric() {
        return None;
    }
    let s: String = skeleton(c.encode_utf8(&mut [0; 4])).collect();
    (!s.is_empty() && s.is_ascii()).then_some(s)
}

/// Uma palavra suspeita no texto: onde está (linha contando de 0, bytes dentro dela) e o que dizer.
#[derive(Clone, Debug, PartialEq)]
pub struct Suspeita {
    pub linha: usize,
    pub bytes: Range<usize>,
    pub palavra: String,
    /// Uma linha, para o rodapé: "а cirílico (U+0430) no lugar de a em pаypal".
    pub resumo: String,
    /// A dica sobre a palavra: o nome oficial de cada letra estranha e o endereço real do link.
    pub dica: String,
}

/// As palavras suspeitas, na ordem do texto (as repetidas também, cada uma no seu lugar):
/// - latino com cirílico na mesma palavra;
/// - latino com uma letra grega que imita outra (`gοogle`; "Δt" e "2πr" não);
/// - num domínio com partes em ASCII, a parte que inteira imita ASCII (`аррӏе.com`, `paypaⅼ.com`).
pub fn procurar(texto: &str) -> Vec<Suspeita> {
    let mut achadas = Vec::new();
    for (linha, l) in texto.split('\n').enumerate() {
        let l = l.strip_suffix('\r').unwrap_or(l);
        for token in l.split(char::is_whitespace).filter(|t| !t.is_empty()) {
            let t0 = token.as_ptr() as usize - l.as_ptr() as usize;
            let host = host(token);
            let host_ascii = host.clone().is_some_and(|h| token[h].chars().any(|c| c.is_ascii_alphabetic()));
            for palavra in token.split(|c: char| !c.is_alphanumeric()).filter(|p| !p.is_ascii()) {
                let p0 = palavra.as_ptr() as usize - token.as_ptr() as usize;
                let no_host = host.as_ref().is_some_and(|h| h.contains(&p0));
                if suspeita(palavra) || no_host && host_ascii && skeleton(palavra).all(|c| c.is_ascii()) {
                    let endereco = host.clone().filter(|_| no_host).and_then(|h| real(&token[h]));
                    let (resumo, dica) = descrever(palavra, endereco);
                    achadas.push(Suspeita { linha, bytes: t0 + p0..t0 + p0 + palavra.len(), palavra: palavra.to_owned(), resumo, dica });
                }
            }
        }
    }
    achadas
}

fn suspeita(palavra: &str) -> bool {
    let (mut latino, mut cirilico, mut grego_que_imita) = (false, false, false);
    for c in palavra.chars() {
        match alfabeto(c) {
            Some(Alfabeto::Latino) => latino = true,
            Some(Alfabeto::Cirilico) => cirilico = true,
            Some(Alfabeto::Grego) => grego_que_imita |= imita(c).is_some(),
            None => {}
        }
    }
    latino && (cirilico || grego_que_imita)
}

/// O nome do site num link, e-mail ou domínio solto ("https://ana@site.com:80/x" → "site.com"),
/// em bytes do token; `None` se não parece domínio (precisa de um ponto entre letras).
fn host(token: &str) -> Option<Range<usize>> {
    let ini = token.find("://").map_or(0, |i| i + 3);
    let mut h = ini..ini + token[ini..].find(['/', '?', '#']).unwrap_or(token.len() - ini);
    if let Some(a) = token[h.clone()].rfind('@') {
        h.start += a + 1;
    }
    if let Some(p) = token[h.clone()].rfind(':') {
        h.end = h.start + p;
    }
    // Pontuação em volta: "(site.com),".
    let s = &token[h.clone()];
    let sem_inicio = s.trim_start_matches(|c: char| !c.is_alphanumeric());
    h.start += s.len() - sem_inicio.len();
    h.end = h.start + sem_inicio.trim_end_matches(|c: char| !c.is_alphanumeric()).len();
    let s = &token[h.clone()];
    let ponto = s.char_indices().any(|(i, c)| c == '.' && s[..i].ends_with(char::is_alphanumeric) && s[i + 1..].starts_with(char::is_alphanumeric));
    ponto.then_some(h)
}

/// O endereço como o navegador usa (punycode), se for diferente do que aparece.
fn real(host: &str) -> Option<String> {
    idna::domain_to_ascii(host).ok().filter(|a| *a != host.to_lowercase())
}

fn descrever(palavra: &str, endereco: Option<String>) -> (String, String) {
    let letras: Vec<(char, Option<Alfabeto>)> = palavra.chars().map(|c| (c, alfabeto(c))).collect();
    let quantas = |a: Alfabeto| letras.iter().filter(|l| l.1 == Some(a)).count();
    let maioria = [Alfabeto::Latino, Alfabeto::Cirilico, Alfabeto::Grego].into_iter().max_by_key(|&a| (quantas(a), a == Alfabeto::Latino)).expect("três alfabetos");
    // As de outro alfabeto; sem nenhuma (`аррӏе`, `paypaⅼ`), as que imitam ASCII.
    let de_fora = |&(_, a): &(char, Option<Alfabeto>)| a.is_some_and(|a| a != maioria);
    let algum_de_fora = letras.iter().any(de_fora);
    let mut vistas: Vec<char> = Vec::new();
    let (mut curtas, mut linhas) = (Vec::new(), vec![format!("Letras disfarçadas em {palavra}:")]);
    for &(c, a) in letras.iter().filter(|l| if algum_de_fora { de_fora(l) } else { imita(l.0).is_some() }) {
        let lugar = imita(c);
        if vistas.contains(&c) {
            continue;
        }
        vistas.push(c);
        let alfabeto = a.map_or(String::new(), |a| format!(" {}", a.nome()));
        let lugar = lugar.map_or(String::new(), |l| format!(" no lugar de {l}"));
        let nome = unicode_names2::name(c).map_or(String::new(), |n| format!("{n} "));
        curtas.push(format!("{c}{alfabeto} (U+{:04X}){lugar}", c as u32));
        linhas.push(format!("{c}   {nome}(U+{:04X}){lugar}", c as u32));
    }
    if let Some(e) = endereco {
        linhas.push(format!("Endereço real: {e}"));
    }
    (format!("{} em {palavra}", curtas.join(", ")), linhas.join("\n"))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn palavras(texto: &str) -> Vec<String> {
        procurar(texto).into_iter().map(|s| s.palavra).collect()
    }

    #[test]
    fn acha_o_a_cirilico_no_link() {
        let texto = "Sua conta.\nentre em https://pаypal.com/login já";
        let s = procurar(texto);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].linha, &texto.lines().nth(1).unwrap()[s[0].bytes.clone()]), (1, "pаypal"));
        assert_eq!(s[0].resumo, "а cirílico (U+0430) no lugar de a em pаypal");
        assert_eq!(s[0].dica, "Letras disfarçadas em pаypal:\nа   CYRILLIC SMALL LETTER A (U+0430) no lugar de a\nEndereço real: xn--pypal-4ve.com");
    }

    #[test]
    fn grego_que_imita_avisa_e_matematica_nao() {
        assert_eq!(palavras("gοogle.com e gοogle.com"), vec!["gοogle", "gοogle"]);
        assert!(procurar("gοogle")[0].resumo.starts_with("ο grego (U+03BF) no lugar de o"));
        assert!(palavras("Δt = 2πr, 5μm e 3λ").is_empty());
    }

    #[test]
    fn dominio_todo_em_cirilico_que_imita() {
        assert_eq!(palavras("abra аррӏе.com agora"), vec!["аррӏе"]);
        assert!(procurar("аррӏе.com")[0].dica.contains("Endereço real: xn--"));
        assert_eq!(palavras("site paypaⅼ.com"), vec!["paypaⅼ"]);
    }

    #[test]
    fn um_alfabeto_so_nao_avisa() {
        assert!(palavras("Привет, мир! Olá, ação, Ελλάδα, café.com, яндекс.com, пример.рф, münchen.de, сор.").is_empty());
    }

    #[test]
    fn maioria_cirilica_aponta_a_latina() {
        assert_eq!(procurar("прoкси")[0].resumo, "o latino (U+006F) em прoкси");
    }

    #[test]
    fn host_do_link() {
        let h = |t: &str| host(t).map(|r| t[r].to_owned());
        assert_eq!(h("https://ana@site.com:80/x?y").as_deref(), Some("site.com"));
        assert_eq!(h("(site.com),").as_deref(), Some("site.com"));
        assert_eq!(h("liberar."), None);
        // Link de markdown: o endereço de dentro conta.
        assert_eq!(palavras("[Banco](https://pаypal.com/x)"), vec!["pаypal"]);
    }
}
