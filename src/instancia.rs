//! Um FeatherNotes só: o segundo que abre (o atalho do post-it, outro clique no ícone) entrega o
//! pedido ao que já está aberto e sai. Duas cópias gravariam a sessão uma por cima da outra.
use pelicano::futures;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

pub enum Inicio {
    /// Outro já estava aberto e recebeu o pedido.
    Entregue,
    /// Somos o primeiro; `None` se não deu para escutar (aí cada um abre sozinho, como antes).
    Primeiro(Option<TcpListener>),
}

const PREFIXO: &str = "feathernotes ";

/// Tenta entregar `pedido` ao que já está aberto; senão passa a escutar os próximos.
// shortcut: dois abrindo no mesmo instante podem os dois virar "primeiro"; um trava de arquivo quando alguém notar.
pub fn iniciar(pedido: &str) -> Inicio {
    let arquivo = crate::arquivo::arquivo_porta();
    let porta = std::fs::read_to_string(&arquivo).ok().and_then(|p| p.trim().parse::<u16>().ok());
    if let Some(porta) = porta {
        let endereco = SocketAddr::from((Ipv4Addr::LOCALHOST, porta));
        if let Ok(mut s) = TcpStream::connect_timeout(&endereco, Duration::from_millis(300)) {
            if writeln!(s, "{PREFIXO}{pedido}").is_ok() {
                return Inicio::Entregue;
            }
        }
    }
    // Só a máquina local: a porta é escolhida pelo sistema e anotada para o próximo.
    let Ok(l) = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)) else { return Inicio::Primeiro(None) };
    let gravou = l.local_addr().ok().is_some_and(|a| {
        let _ = std::fs::create_dir_all(arquivo.parent().expect("tem pasta"));
        crate::arquivo::gravar(&arquivo, a.port().to_string().as_bytes()).is_ok()
    });
    Inicio::Primeiro(gravou.then_some(l))
}

/// Os pedidos que chegam (uma linha cada); qualquer outra coisa na porta é ignorada.
pub fn pedidos(l: TcpListener) -> impl futures::Stream<Item = String> + Send {
    let (mut envia, recebe) = futures::channel::mpsc::channel(4);
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
            let mut linha = String::new();
            if BufReader::new(s).take(256).read_line(&mut linha).is_ok() {
                if let Some(p) = linha.trim().strip_prefix(PREFIXO) {
                    if futures::executor::block_on(futures::SinkExt::send(&mut envia, p.to_owned())).is_err() {
                        return;
                    }
                }
            }
        }
    });
    recebe
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O pedido chega inteiro pela porta; lixo de outro programa não vira pedido.
    #[test]
    fn pedido_atravessa_a_porta() {
        let l = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let porta = l.local_addr().unwrap().port();
        let mut fluxo = Box::pin(pedidos(l));
        let mandar = |t: &str| writeln!(TcpStream::connect((Ipv4Addr::LOCALHOST, porta)).unwrap(), "{t}").unwrap();
        mandar("GET / HTTP/1.1");
        mandar("feathernotes postit");
        assert_eq!(futures::executor::block_on(futures::StreamExt::next(&mut fluxo)), Some("postit".to_string()));
    }
}
