//! A pena na bandeja do sistema (Linux: StatusNotifierItem pelo D-Bus, que o KDE e a extensão
//! AppIndicator do GNOME mostram). Mostra que o FeatherNotes segue aberto por causa dos
//! post-its e dá o "Sair" sem abrir a janela.
// shortcut: no Windows não há bandeja (pede um laço de mensagens Win32, como no launcher); fazer com a crate `tray-icon` quando pedirem.
use ksni::TrayMethods;
use pelicano::futures::channel::mpsc::{unbounded, UnboundedSender};
use pelicano::futures::{stream, Stream, StreamExt};

/// O que se escolheu no menu da pena.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Acao {
    Mostrar,
    NovoPostIt,
    PostIts,
    Sair,
}

struct Bandeja {
    envia: UnboundedSender<Acao>,
}

impl ksni::Tray for Bandeja {
    fn id(&self) -> String {
        "feathernotes".into()
    }

    fn title(&self) -> String {
        "FeatherNotes".into()
    }

    /// O ícone instalado (o .deb, ou o que o app põe em ~/.local/share/icons ao abrir).
    fn icon_name(&self) -> String {
        "feathernotes".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.envia.unbounded_send(Acao::Mostrar);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        let item = |rotulo: &str, acao: Acao| {
            StandardItem {
                label: rotulo.into(),
                activate: Box::new(move |b: &mut Self| {
                    let _ = b.envia.unbounded_send(acao);
                }),
                ..Default::default()
            }
            .into()
        };
        vec![item("Abrir o FeatherNotes", Acao::Mostrar), item("Novo post-it", Acao::NovoPostIt), item("Mostrar os post-its", Acao::PostIts), ksni::MenuItem::Separator, item("Sair", Acao::Sair)]
    }
}

/// Põe a pena na bandeja; as escolhas do menu chegam pelo fluxo. Sem bandeja no sistema (GNOME
/// sem a extensão), o fluxo fica vazio e nada muda.
pub fn iniciar() -> impl Stream<Item = Acao> + Send {
    let (envia, recebe) = unbounded();
    stream::once(async move { (Bandeja { envia }.spawn().await.ok(), recebe) }).flat_map(|(alca, recebe)| {
        // A alça vive junto do fluxo: soltá-la tiraria a pena da bandeja.
        recebe.map(move |a| {
            let _ = &alca;
            a
        })
    })
}
