//! Um FeatherNotes só: o segundo que abre (o atalho do post-it, outro clique no ícone) entrega o
//! pedido ao que já está aberto e sai. Duas cópias gravariam a sessão uma por cima da outra.
use pelicano::futures;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

pub enum Inicio {
    /// Outro já estava aberto e recebeu o pedido.
    Entregue,
    /// Somos o primeiro, com a senha que os pedidos trazem; `None` se não deu para escutar (aí cada
    /// um abre sozinho, como antes).
    Primeiro(Option<(TcpListener, String)>),
}

const PREFIXO: &str = "feathernotes ";

/// Tenta entregar `pedido` ao que já está aberto; senão passa a escutar os próximos.
// shortcut: dois abrindo no mesmo instante podem os dois virar "primeiro"; um trava de arquivo quando alguém notar.
pub fn iniciar(pedido: &str) -> Inicio {
    let arquivo = crate::arquivo::arquivo_porta();
    // "porta senha"; as versões até a 0.1.5 anotam só a porta.
    let anotado = std::fs::read_to_string(&arquivo).unwrap_or_default();
    let mut campos = anotado.split_whitespace();
    let porta = campos.next().and_then(|p| p.parse::<u16>().ok());
    let senha = campos.next().map_or(String::new(), |s| format!("{s} "));
    if let Some(porta) = porta {
        let endereco = SocketAddr::from((Ipv4Addr::LOCALHOST, porta));
        if let Ok(mut s) = TcpStream::connect_timeout(&endereco, Duration::from_millis(300)) {
            // Só é entregue se o FeatherNotes responder "ok" (ou fechar sem nada, como as versões até a 0.1.0):
            // a porta velha pode ser de outro programa agora, que nem responde nem fecha.
            let _ = s.set_read_timeout(Some(Duration::from_millis(300)));
            let mut resposta = String::new();
            if writeln!(s, "{PREFIXO}{senha}{pedido}").is_ok() {
                match BufReader::new(s).read_line(&mut resposta) {
                    Ok(n) if n == 0 || resposta.trim() == "ok" => return Inicio::Entregue,
                    // O aberto é de antes de uma atualização: ele guarda a sessão e sai; esperamos a porta soltar.
                    Ok(_) if resposta.trim() == "trocar" => {
                        for _ in 0..50 {
                            if TcpStream::connect_timeout(&endereco, Duration::from_millis(100)).is_err() {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    // Só a máquina local: a porta é escolhida pelo sistema e anotada para o próximo.
    let Ok(l) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else { return Inicio::Primeiro(None) };
    let senha = nova_senha();
    let gravou = l.local_addr().ok().is_some_and(|a| {
        let _ = std::fs::create_dir_all(arquivo.parent().expect("tem pasta"));
        crate::arquivo::gravar(&arquivo, format!("{} {senha}", a.port()).as_bytes()).is_ok() && so_do_dono(&arquivo)
    });
    Inicio::Primeiro(gravou.then_some((l, senha)))
}

/// Qualquer programa da máquina chega na porta (até de outro usuário); só quem lê o arquivo `porta`
/// sabe a senha. O `RandomState` vem de números aleatórios do sistema.
fn nova_senha() -> String {
    use std::hash::{BuildHasher, Hasher};
    let parte = || std::collections::hash_map::RandomState::new().build_hasher().finish();
    format!("{:016x}{:016x}", parte(), parte())
}

/// O arquivo da porta só para o dono ler (no Windows a pasta do usuário já é só dele).
fn so_do_dono(arquivo: &std::path::Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(arquivo, std::fs::Permissions::from_mode(0o600)).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = arquivo;
        true
    }
}

/// Os pedidos que chegam (uma linha cada); sem a senha, ou qualquer outra coisa na porta, é ignorado.
pub fn pedidos(l: TcpListener, senha: String) -> impl futures::Stream<Item = String> + Send {
    let prefixo = format!("{PREFIXO}{senha} ");
    let (mut envia, recebe) = futures::channel::mpsc::channel(4);
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
            let mut linha = String::new();
            if BufReader::new(&s).take(64 * 1024).read_line(&mut linha).is_ok() {
                if let Some(p) = linha.trim().strip_prefix(prefixo.as_str()) {
                    let p = if substituido() { "trocar" } else { p };
                    let _ = writeln!(&s, "{}", if p == "trocar" { "trocar" } else { "ok" });
                    if futures::executor::block_on(futures::SinkExt::send(&mut envia, p.to_owned())).is_err() {
                        return;
                    }
                }
            }
        }
    });
    recebe
}

/// A atualização trocou o executável no disco (o Linux marca o antigo como apagado): este
/// processo é a versão velha e deve dar lugar à nova.
fn substituido() -> bool {
    std::fs::read_link("/proc/self/exe").is_ok_and(|e| e.to_string_lossy().ends_with(" (deleted)"))
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O pedido chega inteiro pela porta; lixo de outro programa não vira pedido.
    #[test]
    fn pedido_atravessa_a_porta() {
        let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let porta = l.local_addr().unwrap().port();
        let mut fluxo = Box::pin(pedidos(l, "s3nha".into()));
        let mandar = |t: &str| writeln!(TcpStream::connect((Ipv4Addr::LOCALHOST, porta)).unwrap(), "{t}").unwrap();
        mandar("GET / HTTP/1.1");
        // Sem a senha (outro programa, outro usuário): não vira pedido.
        mandar("feathernotes trocar");
        mandar("feathernotes errada trocar");
        mandar("feathernotes s3nha postit");
        assert_eq!(futures::executor::block_on(futures::StreamExt::next(&mut fluxo)), Some("postit".to_string()));
    }

    /// A porta anotada é de outro programa (aceita e não responde): não conta como entregue.
    #[test]
    fn porta_velha_nao_engana() {
        let outro = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let arquivo = crate::arquivo::arquivo_porta();
        std::fs::create_dir_all(arquivo.parent().unwrap()).unwrap();
        crate::arquivo::gravar(&arquivo, outro.local_addr().unwrap().port().to_string().as_bytes()).unwrap();
        let Inicio::Primeiro(Some((l, senha))) = iniciar("postit") else { panic!("devia virar o primeiro") };
        let anotado = std::fs::read_to_string(&arquivo).unwrap();
        assert!(anotado.ends_with(&senha) && senha.len() == 32, "a porta e a senha ficam anotadas");
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&arquivo).unwrap().permissions()) & 0o777, 0o600);
        // Uma versão antiga lê o pedido e fecha sem responder: conta como entregue.
        let antigo = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        crate::arquivo::gravar(&arquivo, antigo.local_addr().unwrap().port().to_string().as_bytes()).unwrap();
        let fecha = std::thread::spawn(move || {
            let (s, _) = antigo.accept().unwrap();
            BufReader::new(s).read_line(&mut String::new()).unwrap();
        });
        assert!(matches!(iniciar("mostrar"), Inicio::Entregue));
        fecha.join().unwrap();
        crate::arquivo::gravar(&arquivo, anotado.as_bytes()).unwrap();
        // Agora a porta é a de um FeatherNotes de verdade: ele responde e o pedido chega.
        let mut fluxo = Box::pin(pedidos(l, senha));
        assert!(matches!(iniciar("mostrar"), Inicio::Entregue));
        assert_eq!(futures::executor::block_on(futures::StreamExt::next(&mut fluxo)), Some("mostrar".to_string()));
        // O aberto é de antes de uma atualização: responde "trocar" e sai; o novo vira o primeiro.
        let velho = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        crate::arquivo::gravar(&arquivo, velho.local_addr().unwrap().port().to_string().as_bytes()).unwrap();
        let sai = std::thread::spawn(move || {
            let (s, _) = velho.accept().unwrap();
            BufReader::new(&s).read_line(&mut String::new()).unwrap();
            writeln!(&s, "trocar").unwrap();
        });
        assert!(matches!(iniciar("mostrar"), Inicio::Primeiro(Some(_))));
        sai.join().unwrap();
    }
}
