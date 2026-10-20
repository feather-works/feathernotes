#![cfg_attr(windows, windows_subsystem = "windows")]
//! FeatherNotes: bloco de notas simples da suíte FeatherWorks. Abas, localizar e substituir,
//! e uma sessão que volta como estava (com o que não foi salvo) quando o app abre de novo.
mod arquivo;
mod codificacao;
mod consumo;
mod instancia;

use arquivo::{AbaSalva, Fonte, PostItSalvo, Preferencias, Tema};
use codificacao::Cod;
use pelicano::column;
use pelicano::*;
use std::path::PathBuf;
use std::time::Duration;

const RAIO_JANELA: f32 = 10.0;
const ALTURA_BARRA: f32 = 44.0;
const TAMANHO_JANELA: Size = Size::new(980.0, 680.0);
const SEM_TITULO: &str = "Sem título.txt";
const TAMANHO_PADRAO: f32 = 15.0;
/// A pena do FeatherNotes: azul.
const COR_APP: &str = "#2f7ae5";

/// A janela principal. `escondida`: o app começa em segundo plano (início com o sistema, atalho do post-it).
fn janela_principal(escondida: bool) -> Settings {
    Settings {
        title: "FeatherNotes".into(),
        size: TAMANHO_JANELA,
        min_size: Some(Size::new(420.0, 260.0)),
        // A barra de título é desenhada pelo app; os cantos arredondados pedem a janela transparente.
        decorations: false,
        transparent: true,
        blur: true,
        // Fechar passa pelo `Msg::Fechar`, que guarda a sessão antes.
        close_on_request: false,
        app_id: Some("feathernotes".into()),
        // O × fecha só a janela: os post-its continuam e o app segue até o "Sair".
        run_in_background: true,
        start_hidden: escondida,
        // No X11 os post-its lembram o lugar, prendem no fundo e ficam fora da dock; o Wayland não deixa nada disso.
        x11: true,
        ..Default::default()
    }
}

fn main() -> Result<(), Error> {
    // `--postit`: o atalho do sistema; só abre um post-it (no que já está aberto, se houver).
    let postit = std::env::args().any(|a| a == "--postit");
    // `--fundo`: o início com o sistema; só os post-its que estavam abertos aparecem.
    let fundo = std::env::args().any(|a| a == "--fundo");
    let porta = match instancia::iniciar(if postit { "postit" } else { "mostrar" }) {
        instancia::Inicio::Entregue => return Ok(()),
        instancia::Inicio::Primeiro(l) => l,
    };
    run(janela_principal(fundo || postit), move || {
        let mut n = Notas::novo();
        // Aponta o início com o sistema para este executável, com o `--fundo` (aqui e não no
        // `novo`: os testes também criam o `Notas`).
        if n.inicio_sistema {
            let _ = arquivo::iniciar_com_sistema(true);
        }
        #[cfg(target_os = "linux")]
        if let Err(e) = arquivo::registrar_no_sistema(&include_str!("../assets/pena.svg").replace("#287846", COR_APP).replace("#72aa86", "#7fb0f0")) {
            n.aviso = Some(format!("Não deu para pôr o FeatherNotes no menu de aplicativos: {e}"));
        }
        let pedidos = porta.map(|l| Task::run(instancia::pedidos(l), Msg::Pedido));
        let novo = postit.then(|| Task::done(Msg::NovoPostIt));
        // Os post-its que estavam abertos voltam junto.
        let abrir: Vec<usize> = (0..n.postits.len()).filter(|&i| n.postits[i].aberto).collect();
        let abertos: Vec<Task<Msg>> = abrir.into_iter().map(|i| n.abrir_postit(i)).collect();
        (n, Task::batch([Task::run(tique(Duration::from_secs(2)), |_| Msg::Medir)].into_iter().chain(abertos).chain(pedidos).chain(novo)))
    })
}

// ---------- Estado ----------

struct Aba {
    /// Fixo enquanto a aba existe: nomeia a cópia de recuperação e as respostas dos diálogos.
    id: u64,
    caminho: Option<PathBuf>,
    /// `None` até a aba ser aberta: na sessão as abas são só referências.
    conteudo: Option<Content>,
    /// A versão do conteúdo igual ao disco; `None` = veio da recuperação, nunca salvo.
    salvo: Option<u64>,
    /// A versão já copiada para a recuperação.
    copiado: u64,
    cod: Cod,
    /// A quebra da primeira linha do arquivo (lida ao abrir) ou a escolhida para salvar.
    quebra: Quebra,
    /// Abriu com acentos quebrados ("JoÃ£o"): o rodapé oferece o conserto.
    acentos_quebrados: bool,
    /// O usuário escolheu a quebra de linha: ao salvar, todas as linhas passam a ela.
    converter: bool,
    /// Mostra o Markdown formatado (ligado sozinho em .md).
    markdown: bool,
}

impl Aba {
    fn nova(id: u64, caminho: Option<PathBuf>) -> Self {
        let markdown = caminho.as_deref().is_some_and(eh_markdown);
        Aba { id, caminho, conteudo: None, salvo: Some(0), copiado: 0, cod: Cod::Utf8, quebra: Quebra::Lf, acentos_quebrados: false, converter: false, markdown }
    }
    fn nome(&self) -> String {
        self.caminho.as_ref().and_then(|p| p.file_name()).map_or(SEM_TITULO.into(), |n| n.to_string_lossy().into_owned())
    }
    /// Tem algo que o disco não tem. Fechada, só se ficou cópia de recuperação.
    fn alterada(&self) -> bool {
        match &self.conteudo {
            Some(c) => self.salvo != Some(c.version()),
            None => arquivo::recuperacao(self.id).exists(),
        }
    }
}

/// Uma nota solta numa janela própria. O texto vai para o disco sozinho (não há "salvar").
struct PostIt {
    /// Também o número da janela dele.
    id: u64,
    conteudo: Content,
    cor: usize,
    aberto: bool,
    fixo: bool,
    /// Onde a janela estava (fora do Wayland, que não conta nem deixa escolher).
    posicao: Option<Point>,
    /// Na lixeira (veja `PostItSalvo::excluido`).
    excluido: bool,
    /// O × num post-it com texto: a pergunta está à mostra.
    confirmar: bool,
    /// A versão já gravada.
    gravado: u64,
}

/// (fundo, barra) de cada cor; o texto é sempre escuro, como no papel.
const CORES_POSTIT: [(&str, &str); 5] = [("#fff3a6", "#f7e47c"), ("#d4f5c9", "#b9eaa8"), ("#ffd6e0", "#ffbccd"), ("#d3e9ff", "#b5d9ff"), ("#e6dcff", "#d3c4ff")];
const TINTA_POSTIT: &str = "#3b3522";

struct Busca {
    termo: String,
    troca: String,
    maiusculas: bool,
}

struct Notas {
    abas: Vec<Aba>,
    ativa: usize,
    proximo_id: u64,
    prefs: Preferencias,
    escuro: bool,
    menu: bool,
    busca: Option<Busca>,
    /// Aba alterada esperando a resposta de "salvar antes de fechar?".
    confirmar: Option<u64>,
    /// Recado na barra de status (erro ao salvar, "não encontrado"…).
    aviso: Option<String>,
    /// Já há uma cópia de recuperação marcada para daqui a pouco.
    copia_agendada: bool,
    largura: f32,
    altura: f32,
    /// A lista de codificações aberta; `true` = "Reabrir com" em vez de "Converter para".
    menu_cod: Option<bool>,
    outras_cod: bool,
    /// "RAM 40 MB · CPU 0,1%" no rodapé, medido a cada 2 s.
    recursos: String,
    cpu_antes: Option<(u64, u64)>,
    /// A roda com Ctrl soma aqui até dar um passo de zoom (touchpad manda passos pequenos).
    zoom: std::cell::Cell<f32>,
    propriedades: bool,
    /// O número digitado em "Ir para a linha" (Ctrl+G), com o painel aberto.
    ir_para: Option<String>,
    /// Sobe a cada painel que abre ou fecha: o campo dele (ou o editor, ao fechar) pega a próxima tecla.
    foco: u64,
    /// ((aba, versão), quantos ocultos): contar o texto só quando ele muda.
    ocultos: std::cell::RefCell<Option<((u64, u64), usize)>>,
    /// ((aba, versão), [linhas, palavras, caracteres sem espaço, todos]) do painel de propriedades.
    estatisticas: std::cell::RefCell<Option<((u64, u64), [usize; 4])>>,
    postits: Vec<PostIt>,
    /// A lista de post-its à direita (no lugar das propriedades).
    painel_postits: bool,
    /// Abre junto com o sistema (lido uma vez: no Windows perguntar custa um processo).
    inicio_sistema: bool,
    /// O atalho do sistema que cria post-it está posto (só GNOME).
    atalho_postit: bool,
}

#[derive(Debug, Clone)]
enum Msg {
    Mudou,
    Tamanho(Size),
    MenuCod(Option<bool>),
    OutrasCod,
    Reabrir(Cod),
    Detectar,
    ConsertarAcentos,
    Medir,
    Fonte_(Fonte),
    Propriedades,
    IrPara,
    Linha(String),
    IrParaLinha,
    Invisiveis,
    Markdown(bool),
    Formatar(Formato),
    ProximoOculto,
    Codificacao(Cod),
    QuebraLinha(Quebra),
    AbrirPasta,
    Descartar,
    TelaCheia,
    Copiar,
    Aba(usize),
    ProximaAba,
    Nova,
    FecharAba(u64),
    Confirmar(Resposta),
    Abrir,
    Abertos(Vec<PathBuf>),
    Salvar,
    SalvarComo,
    SalvarEm(u64, Option<PathBuf>, bool),
    Menu,
    FecharMenu,
    Fonte(f32),
    Quebra,
    Tema(Tema),
    Localizar,
    Termo(String),
    Troca(String),
    Maiusculas,
    Proximo,
    Substituir,
    SubstituirTudo,
    Esc,
    Arrastar,
    Puxar(ResizeDirection),
    Minimizar,
    Maximizar,
    Fechar,
    NovoPostIt,
    MostrarPostIts,
    /// Abrir ao iniciar o sistema; vem da configuração inicial ou do menu.
    IniciarComSistema(bool),
    AtalhoPostIt(bool),
    /// O "Pronto" da configuração inicial.
    Configurado,
    /// De outro FeatherNotes que abriu: "postit" ou "mostrar".
    Pedido(String),
    PostItMudou,
    PostItCor(u64),
    PostItFixar(u64),
    PostItArrastar(u64),
    PostItPuxar(u64, ResizeDirection),
    /// O × do post-it: com texto, pergunta antes.
    PostItFechar(u64),
    /// A resposta: `None` = cancelar, `Some(false)` = fechar (fica guardado), `Some(true)` = excluir.
    PostItResposta(u64, Option<bool>),
    PostItMoveu(u64, Point),
    MostrarPrincipal,
    Sair,
    PainelPostIts,
    AbrirPostIt(u64),
    RestaurarPostIt(u64),
    /// Apagar de vez (só na lixeira).
    ApagarPostIt(u64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Quebra {
    Lf,
    Crlf,
    Cr,
}

impl Quebra {
    fn de(texto: &str) -> Quebra {
        match texto.find(['\r', '\n']).map(|i| &texto[i..]) {
            Some(t) if t.starts_with("\r\n") => Quebra::Crlf,
            Some(t) if t.starts_with('\r') => Quebra::Cr,
            _ => Quebra::Lf,
        }
    }
    fn nome(self) -> &'static str {
        match self {
            Quebra::Lf => "LF",
            Quebra::Crlf => "CRLF",
            Quebra::Cr => "CR",
        }
    }
    fn str(self) -> &'static str {
        match self {
            Quebra::Lf => "\n",
            Quebra::Crlf => "\r\n",
            Quebra::Cr => "\r",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Formato {
    Negrito,
    Italico,
    Tachado,
    Codigo,
    Link,
    Titulo(usize),
    Lista,
    Numerada,
    Tarefa,
}

#[derive(Debug, Clone, Copy)]
enum Resposta {
    Salvar,
    Descartar,
    Cancelar,
}

impl Notas {
    fn novo() -> Self {
        let prefs = arquivo::ler_preferencias();
        let (ativa, salvas) = arquivo::ler_sessao();
        let mut abas: Vec<Aba> = salvas.into_iter().map(|AbaSalva { id, caminho, markdown }| Aba { markdown: markdown || caminho.as_deref().is_some_and(eh_markdown), ..Aba::nova(id, caminho) }).collect();
        let proximo_id = abas.iter().map(|a| a.id + 1).max().unwrap_or(1);
        if abas.is_empty() {
            abas.push(Aba::nova(proximo_id, None));
        }
        let escuro = escuro(prefs.tema);
        let mut n = Notas { ativa: ativa.min(abas.len() - 1), abas, proximo_id: proximo_id + 1, prefs, escuro, menu: false, busca: None, confirmar: None, aviso: None, copia_agendada: false, largura: TAMANHO_JANELA.width, altura: TAMANHO_JANELA.height, menu_cod: None, outras_cod: false, recursos: String::new(), cpu_antes: None, zoom: Default::default(), propriedades: false, estatisticas: Default::default(), ir_para: None, foco: 0, ocultos: Default::default(), postits: Vec::new(), painel_postits: false, inicio_sistema: arquivo::inicia_com_sistema(), atalho_postit: cfg!(target_os = "linux") && arquivo::tem_atalho_postit() };
        n.postits = arquivo::ler_postits().into_iter().map(|(PostItSalvo { id, cor, aberto, fixo, posicao, excluido }, texto)| PostIt { id, conteudo: Content::with_text(&texto), cor: cor % CORES_POSTIT.len(), aberto: aberto && !excluido, fixo, posicao, excluido, confirmar: false, gravado: 0 }).collect();
        n.carregar(n.ativa);
        n
    }

    /// Lê o texto da aba na primeira vez que ela aparece: a cópia de recuperação ou o arquivo.
    fn carregar(&mut self, i: usize) {
        let aba = &mut self.abas[i];
        if aba.conteudo.is_some() {
            return;
        }
        let copia = arquivo::recuperacao(aba.id);
        let original = aba.caminho.as_ref().map(std::fs::read);
        let (texto, salvo) = if let Ok(bytes) = std::fs::read(&copia) {
            // A cópia é sempre UTF-8; a codificação para salvar vem do arquivo original.
            aba.cod = match &original {
                Some(Ok(b)) => codificacao::detectar(b),
                _ => Cod::Utf8,
            };
            (String::from_utf8_lossy(&bytes).into_owned(), None)
        } else {
            match original {
                Some(Ok(bytes)) => {
                    aba.cod = codificacao::detectar(&bytes);
                    let (texto, ruim) = aba.cod.ler(&bytes);
                    if ruim {
                        self.aviso = Some(format!("Alguns bytes não fazem sentido em {}: viraram �", aba.cod.nome()));
                    }
                    (texto, Some(0))
                }
                Some(Err(e)) => {
                    self.aviso = Some(format!("Não abriu {}: {e}", aba.caminho.as_ref().expect("tem caminho").display()));
                    (String::new(), None)
                }
                None => (String::new(), Some(0)),
            }
        };
        aba.quebra = Quebra::de(&texto);
        aba.acentos_quebrados = codificacao::consertar_acentos(&texto).is_some();
        aba.conteudo = Some(Content::with_text(&texto));
        aba.salvo = salvo;
        aba.copiado = 0;
    }

    fn ativar(&mut self, i: usize) {
        self.ativa = i.min(self.abas.len() - 1);
        self.carregar(self.ativa);
    }

    fn conteudo(&self) -> &Content {
        self.abas[self.ativa].conteudo.as_ref().expect("a aba ativa está carregada")
    }

    fn nova_aba(&mut self, caminho: Option<PathBuf>) {
        self.abas.push(Aba::nova(self.proximo_id, caminho));
        self.proximo_id += 1;
        self.ativar(self.abas.len() - 1);
    }

    fn fechar_aba(&mut self, id: u64) {
        let Some(i) = self.abas.iter().position(|a| a.id == id) else { return };
        let _ = std::fs::remove_file(arquivo::recuperacao(id));
        self.abas.remove(i);
        if self.abas.is_empty() {
            self.abas.push(Aba::nova(self.proximo_id, None));
            self.proximo_id += 1;
        }
        let ativa = if i < self.ativa || self.ativa >= self.abas.len() { self.ativa.saturating_sub(1) } else { self.ativa };
        self.ativar(ativa);
        self.guardar_sessao();
    }

    /// Copia o que não foi salvo para a recuperação e grava a lista de abas.
    fn guardar_sessao(&mut self) {
        for aba in &mut self.abas {
            let Some(c) = &aba.conteudo else { continue };
            let copia = arquivo::recuperacao(aba.id);
            if aba.salvo != Some(c.version()) {
                if aba.copiado != c.version() || !copia.exists() {
                    let _ = std::fs::create_dir_all(copia.parent().expect("tem pasta"));
                    match arquivo::gravar(&copia, c.text().as_bytes()) {
                        Ok(_) => aba.copiado = c.version(),
                        Err(e) => self.aviso = Some(format!("Não guardou a cópia de recuperação: {e}")),
                    }
                }
            } else {
                let _ = std::fs::remove_file(copia);
            }
        }
        self.guardar_postits();
        let lista: Vec<AbaSalva> = self.abas.iter().map(|a| AbaSalva { id: a.id, caminho: a.caminho.clone(), markdown: a.markdown }).collect();
        if let Err(e) = arquivo::gravar_sessao(self.ativa, &lista) {
            self.aviso = Some(format!("Não guardou a sessão: {e}"));
        }
    }

    /// Grava o texto dos post-its que mudaram e a lista (cor, aberto).
    fn guardar_postits(&mut self) {
        for p in &mut self.postits {
            if p.gravado != p.conteudo.version() {
                let caminho = arquivo::texto_postit(p.id);
                let _ = std::fs::create_dir_all(caminho.parent().expect("tem pasta"));
                match arquivo::gravar(&caminho, p.conteudo.text().as_bytes()) {
                    Ok(_) => p.gravado = p.conteudo.version(),
                    Err(e) => self.aviso = Some(format!("Não guardou o post-it: {e}")),
                }
            }
        }
        let lista: Vec<PostItSalvo> = self.postits.iter().map(|p| PostItSalvo { id: p.id, cor: p.cor, aberto: p.aberto, fixo: p.fixo, posicao: p.posicao, excluido: p.excluido }).collect();
        if let Err(e) = arquivo::gravar_postits(&lista) {
            self.aviso = Some(format!("Não guardou os post-its: {e}"));
        }
    }

    fn abrir_postit(&mut self, i: usize) -> Task<Msg> {
        let p = &mut self.postits[i];
        p.aberto = true;
        p.confirmar = false;
        // Fechar pelo sistema (Alt+F4) também passa pela pergunta.
        let s = Settings { title: "Post-it".into(), size: Size::new(280.0, 280.0), min_size: Some(Size::new(180.0, 140.0)), decorations: false, transparent: true, close_on_request: false, app_id: Some("feathernotes".into()), position: p.posicao, skip_taskbar: true, ..Default::default() };
        let fixo = p.fixo.then(|| Task::window_of(WindowId(p.id), WindowCmd::Level(WindowLevel::AlwaysOnBottom)));
        Task::batch([Task::open_window(WindowId(p.id), s)].into_iter().chain(fixo))
    }

    /// A janela fecha: o post-it vazio some de vez; `excluir` manda para a lixeira da lista; o resto
    /// fica guardado na lista.
    fn postit_fechou(&mut self, id: u64, excluir: bool) {
        let Some(i) = self.postits.iter().position(|p| p.id == id) else { return };
        if self.postits[i].conteudo.text().trim().is_empty() {
            self.apagar_postit(i);
        } else {
            self.postits[i].aberto = false;
            self.postits[i].excluido = excluir;
        }
        self.guardar_postits();
    }

    fn apagar_postit(&mut self, i: usize) {
        let p = self.postits.remove(i);
        let _ = std::fs::remove_file(arquivo::texto_postit(p.id));
    }

    /// A lista de post-its (e a lixeira), no lugar do painel de propriedades.
    fn lista_postits(&self, c: Cores) -> Element<'_, Msg> {
        let linha = || container(Space::new(Length::FILL, 1.0)).style(caixa(c.borda, Border::default()));
        fn cartao<'a>(c: Cores, filhos: Vec<Element<'a, Msg>>) -> Container<'a, Msg> {
            container(column(filhos)).width(Length::FILL).style(caixa(c.papel, Border::rounded(10.0).color(c.borda, 1.0)))
        }
        let bolinha = |cor: usize| container(Space::new(12.0, 12.0)).style(caixa(Color::hex(CORES_POSTIT[cor].1).expect("cor"), Border::rounded(6.0)));
        let resumo = |p: &PostIt| {
            let t = p.conteudo.text();
            // A primeira linha com texto, sem os sinais de Markdown do começo.
            let primeira = t.lines().map(|l| l.trim_start_matches(['#', '-', '*', '>', ' ']).trim_start_matches("[ ] ").trim_start_matches("[x] ").replace("**", "")).find(|l| !l.trim().is_empty()).unwrap_or_default();
            if primeira.chars().count() > 34 { primeira.chars().take(32).collect::<String>() + "…" } else { primeira }
        };
        let mut vivos: Vec<Element<'_, Msg>> = Vec::new();
        for p in self.postits.iter().filter(|p| !p.excluido) {
            if !vivos.is_empty() {
                vivos.push(linha().into());
            }
            let estado = if p.aberto { "Aberto" } else { "Fechado" };
            let corpo = row![bolinha(p.cor), column![text(resumo(p)).size(13).color(c.texto), text(estado).size(11).color(c.mudo)].spacing(2)].spacing(10).align(Align::Center);
            vivos.push(button(corpo).on_press(Msg::AbrirPostIt(p.id)).width(Length::FILL).padding([9u16, 14]).style(plano(c, false)).into());
        }
        let mut lixo: Vec<Element<'_, Msg>> = Vec::new();
        for p in self.postits.iter().filter(|p| p.excluido) {
            if !lixo.is_empty() {
                lixo.push(linha().into());
            }
            let acao = |rotulo, m, vermelho| button(text(rotulo).size(12).color(if vermelho { Color::rgb8(0xc0, 0x1c, 0x28) } else { c.texto })).on_press(m).padding([4u16, 8]).style(plano(c, false));
            lixo.push(
                row![bolinha(p.cor), text(resumo(p)).size(13).color(c.mudo).width(Length::FILL), acao("Restaurar", Msg::RestaurarPostIt(p.id), false), acao("Apagar", Msg::ApagarPostIt(p.id), true)]
                    .spacing(6)
                    .align(Align::Center)
                    .padding([6u16, 10])
                    .into(),
            );
        }
        let fechados = self.postits.iter().any(|p| !p.aberto && !p.excluido);
        let topo = row![text("Post-its").size(14).font(Font::SEMIBOLD).color(c.texto), horizontal_space()]
            .push_maybe(fechados.then(|| pilula(c, "Abrir todos", false, Msg::MostrarPostIts)))
            .push(pilula(c, "Novo", false, Msg::NovoPostIt))
            .spacing(4)
            .align(Align::Center);
        let mut painel = column![topo].spacing(14).padding(16.0).width(Length::FILL);
        painel = if vivos.is_empty() { painel.push(text("Nenhum post-it ainda. Crie um com Shift+Ctrl+N.").size(13).color(c.mudo)) } else { painel.push(cartao(c, vivos)) };
        if !lixo.is_empty() {
            painel = painel.push(text("Lixeira").size(14).font(Font::SEMIBOLD).color(c.texto)).push(cartao(c, lixo));
        }
        row![container(Space::new(1.0, Length::FILL)).style(caixa(c.borda, Border::default())), container(scrollable(painel)).width(320.0).height(Length::FILL).style(caixa(c.rodape, Border::default()))].into()
    }

    fn postit<'a>(&'a self, p: &'a PostIt) -> Element<'a, Msg> {
        let (fundo, barra) = CORES_POSTIT[p.cor];
        let (fundo, barra, tinta) = (Color::hex(fundo).expect("cor"), Color::hex(barra).expect("cor"), Color::hex(TINTA_POSTIT).expect("cor"));
        let proxima = Color::hex(CORES_POSTIT[(p.cor + 1) % CORES_POSTIT.len()].0).expect("cor");
        let id = p.id;
        let sobre = move |_: &Theme, s: Status| Style { background: matches!(s, Status::Hovered | Status::Pressed).then(|| tinta.alpha(0.1).into()), border: Border::rounded(6.0), ..Default::default() };
        let botao = |icone, m| button(container(traco(icone, 14.0, tinta)).center()).on_press(m).width(28.0).height(26.0).padding(0.0).style(sobre);
        let cor = tooltip(button(container(container(Space::new(14.0, 14.0)).style(caixa(proxima, Border::rounded(7.0).color(tinta.alpha(0.35), 1.0)))).center()).on_press(Msg::PostItCor(id)).width(28.0).height(26.0).padding(0.0).style(sobre), "Trocar a cor", TooltipPosition::Bottom);
        let abrir_notas = button(container(pena(15.0)).center()).on_press(Msg::MostrarPrincipal).width(28.0).height(26.0).padding(0.0).style(sobre);
        let topo = row![
            tooltip(abrir_notas, "Abrir o FeatherNotes", TooltipPosition::Bottom),
            tooltip(botao(ICONE_MAIS, Msg::NovoPostIt), "Novo post-it", TooltipPosition::Bottom),
            cor,
            mouse_area(Space::new(Length::FILL, 26.0)).on_press(Msg::PostItArrastar(id)),
            tooltip(
                button(container(traco(ICONE_ALFINETE, 14.0, if p.fixo { tinta } else { tinta.alpha(0.45) })).center()).on_press(Msg::PostItFixar(id)).width(28.0).height(26.0).padding(0.0).style(move |t, s| {
                    let mut st = sobre(t, s);
                    if p.fixo {
                        st.background = Some(tinta.alpha(0.14).into());
                    }
                    st
                }),
                if p.fixo { "Soltar do fundo da tela" } else { "Prender no fundo da tela" },
                TooltipPosition::Bottom
            ),
            tooltip(botao(ICONE_X, Msg::PostItFechar(id)), "Fechar", TooltipPosition::Bottom),
        ]
        .spacing(2)
        .align(Align::Center)
        .padding([3u16, 4]);
        let editor = text_editor(&p.conteudo)
            .size(15.0)
            .font(Font::DEFAULT)
            .wrap(true)
            .markdown(true)
            .padding(Padding { top: 6.0, right: 14.0, bottom: 10.0, left: 14.0 })
            .on_change(Msg::PostItMudou)
            .autofocus(1)
            .style(move |_, _| Style::default().background(Color::TRANSPARENT).text_color(tinta));
        let corpo = column![container(topo).width(Length::FILL).style(caixa(barra, Border::default())), editor];
        let alca = |d: ResizeDirection| mouse_area(Space::new(14.0, 14.0)).on_press(Msg::PostItPuxar(id, d)).cursor_icon(CursorIcon::SeResize);
        let canto = container(alca(ResizeDirection::SouthEast)).width(Length::FILL).height(Length::FILL).align_x(Align::End).align_y(Align::End);
        let mut camadas = stack![corpo, canto];
        if p.confirmar {
            let resposta = |rotulo, r: Option<bool>, cor: Color, fundo: Option<Color>| {
                button(text(rotulo).size(12).color(cor)).on_press(Msg::PostItResposta(id, r)).padding([5u16, 10]).style(move |t, s| {
                    let mut st = sobre(t, s);
                    st.background = st.background.or(fundo.map(Into::into));
                    st
                })
            };
            let cartao = container(
                column![
                    text("Fechar este post-it?").size(13).font(Font::SEMIBOLD).color(tinta),
                    text("Fechar guarda na lista de post-its; excluir manda para a lixeira dela.").size(12).color(tinta.alpha(0.7)),
                    row![resposta("Excluir", Some(true), Color::rgb8(0xc0, 0x1c, 0x28), None), horizontal_space(), resposta("Cancelar", None, tinta, None), resposta("Fechar", Some(false), Color::WHITE, Some(tinta))].spacing(4).align(Align::Center),
                ]
                .spacing(8),
            )
            .padding(12.0)
            .width(Length::FILL)
            .style(move |_| Style::default().background(fundo).border(Border::rounded(8.0).color(tinta.alpha(0.25), 1.0)).shadow(Shadow { color: Color::BLACK.alpha(0.18), offset: Point::new(0.0, 4.0), blur: 14.0 }));
            let veu = container(Space::new(Length::FILL, Length::FILL)).style(caixa(tinta.alpha(0.12), Border::rounded(RAIO_JANELA)));
            camadas = camadas.push(stack![mouse_area(veu).on_press(Msg::PostItResposta(id, None)), container(cartao).width(Length::FILL).height(Length::FILL).padding(12.0).center()]);
        }
        container(camadas).width(Length::FILL).height(Length::FILL).clip(true).style(move |_| Style::default().background(fundo).border(Border::rounded(RAIO_JANELA).color(tinta.alpha(0.15), 1.0)).text_color(tinta)).into()
    }

    /// Depois de uma edição: a cópia de recuperação sai em 2 s (uma só para várias teclas).
    fn mudou(&mut self) -> Task<Msg> {
        self.aviso = None;
        if self.copia_agendada {
            return Task::none();
        }
        self.copia_agendada = true;
        Task::perform(espera(Duration::from_secs(2)), |_| Msg::Copiar)
    }

    fn salvar(&mut self, id: u64, caminho: PathBuf) -> bool {
        let Some(aba) = self.abas.iter_mut().find(|a| a.id == id) else { return false };
        let Some(c) = &aba.conteudo else { return false };
        let mut texto = c.text();
        if aba.converter {
            texto = texto.replace("\r\n", "\n").replace('\r', "\n");
            if aba.quebra != Quebra::Lf {
                texto = texto.replace('\n', aba.quebra.str());
            }
        }
        // Nada some em silêncio: caractere que não cabe na codificação impede salvar.
        let bytes = match aba.cod.gravar(&texto) {
            Ok(b) => b,
            Err(faltam) => {
                let lista: String = faltam.iter().take(8).collect::<Vec<_>>().iter().map(|c| format!("“{c}” ")).collect();
                self.aviso = Some(format!("Não salvou: {} caractere(s) não cabem em {} ({lista}). Escolha UTF-8 nas propriedades.", faltam.len(), aba.cod.nome()));
                return false;
            }
        };
        match arquivo::gravar(&caminho, &bytes) {
            Ok(()) => {
                aba.salvo = Some(c.version());
                if aba.caminho.as_ref() != Some(&caminho) {
                    aba.markdown = eh_markdown(&caminho);
                }
                aba.caminho = Some(caminho);
                self.aviso = Some("Salvo".into());
                self.guardar_sessao();
                true
            }
            Err(e) => {
                self.aviso = Some(format!("Não salvou: {e}"));
                false
            }
        }
    }

    /// Salva a aba; sem arquivo ainda, pergunta onde. `fechar` fecha a aba depois.
    fn salvar_aba(&mut self, id: u64, como: bool, fechar: bool) -> Task<Msg> {
        let Some(aba) = self.abas.iter().find(|a| a.id == id) else { return Task::none() };
        match aba.caminho.clone() {
            Some(p) if !como => {
                if self.salvar(id, p) && fechar {
                    self.fechar_aba(id);
                }
                Task::none()
            }
            atual => {
                let nome = aba.nome();
                let pasta = atual.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf()).or_else(dirs::document_dir);
                Task::perform(
                    async move {
                        let mut d = rfd::AsyncFileDialog::new().set_file_name(nome).add_filter("Texto", &["txt"]);
                        if let Some(p) = pasta {
                            d = d.set_directory(p);
                        }
                        d.save_file().await.map(|f| f.path().to_path_buf())
                    },
                    move |p| Msg::SalvarEm(id, p, fechar),
                )
            }
        }
    }

    /// Lê de novo os bytes do arquivo com outra codificação (`None` = detectar).
    fn reabrir(&mut self, cod: Option<Cod>) {
        let aba = &mut self.abas[self.ativa];
        let Some(p) = aba.caminho.clone() else {
            self.aviso = Some("Ainda não salvo: não há bytes para reabrir. Use “Converter para”.".into());
            return;
        };
        if aba.alterada() {
            self.aviso = Some("Salve ou descarte as alterações antes de reabrir com outra codificação".into());
            return;
        }
        match std::fs::read(&p) {
            Ok(bytes) => {
                let cod = cod.unwrap_or_else(|| codificacao::detectar(&bytes));
                let (texto, ruim) = cod.ler(&bytes);
                aba.cod = cod;
                aba.quebra = Quebra::de(&texto);
                aba.acentos_quebrados = codificacao::consertar_acentos(&texto).is_some();
                aba.conteudo = Some(Content::with_text(&texto));
                aba.salvo = Some(0);
                aba.converter = false;
                self.aviso = ruim.then(|| format!("Alguns bytes não fazem sentido em {}: viraram �", cod.nome()));
            }
            Err(e) => self.aviso = Some(format!("Não abriu: {e}")),
        }
    }

    fn mudar_prefs(&mut self, f: impl FnOnce(&mut Preferencias)) {
        f(&mut self.prefs);
        self.escuro = escuro(self.prefs.tema);
        arquivo::gravar_preferencias(&self.prefs);
    }
}

fn eh_markdown(p: &std::path::Path) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

fn escuro(t: Tema) -> bool {
    match t {
        Tema::Claro => false,
        Tema::Escuro => true,
        Tema::Automatico => arquivo::sistema_escuro(),
    }
}

/// Um futuro que termina depois de `d`, sem prender uma thread do pool dormindo.
fn espera(d: Duration) -> impl std::future::Future<Output = ()> + Send {
    let (pronto, sinal) = futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(d);
        let _ = pronto.send(());
    });
    async move {
        let _ = sinal.await;
    }
}

/// Um tique a cada `d`, de uma thread que dorme; para sozinha quando o app solta o fluxo.
fn tique(d: Duration) -> impl futures::Stream<Item = ()> + Send {
    let (mut envia, recebe) = futures::channel::mpsc::channel(1);
    std::thread::spawn(move || loop {
        if envia.is_closed() {
            return;
        }
        let _ = envia.try_send(());
        std::thread::sleep(d);
    });
    recebe
}

// ---------- Cores e ícones ----------

#[derive(Clone, Copy)]
struct Cores {
    barra: Color,
    papel: Color,
    rodape: Color,
    borda: Color,
    texto: Color,
    mudo: Color,
    numeros: Color,
    sobre: Color,
    acento: Color,
}

impl Notas {
    fn cores(&self) -> Cores {
        let h = |s: &str| Color::hex(s).expect("cor válida");
        if self.escuro {
            Cores { barra: h("#202329").alpha(0.96), papel: h("#17191d"), rodape: h("#1c1e23"), borda: h("#2d3139"), texto: h("#e6e8eb"), mudo: h("#a3a9b0"), numeros: h("#5c626b"), sobre: Color::WHITE.alpha(0.08), acento: h("#5b9cf0") }
        } else {
            Cores { barra: h("#e8ecf3").alpha(0.96), papel: h("#ffffff"), rodape: h("#f6f7f9"), borda: h("#dfe3ea"), texto: h("#1f2328"), mudo: h("#4a4f55"), numeros: h("#a0a5ab"), sobre: Color::BLACK.alpha(0.06), acento: h(COR_APP) }
        }
    }
}

const ICONE_MENU: &str = r#"<path d="M4 6h16M4 12h16M4 18h16"/>"#;
const ICONE_MAIS: &str = r#"<path d="M12 5v14M5 12h14"/>"#;
const ICONE_X: &str = r#"<path d="M18 6L6 18M6 6l12 12"/>"#;
const ICONE_MINIMIZAR: &str = r#"<path d="M5 12h14"/>"#;
const ICONE_MAXIMIZAR: &str = r#"<rect x="5" y="5" width="14" height="14" rx="1"/>"#;
const ICONE_MENOS_ZOOM: &str = r#"<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.3-4.3M8 11h6"/>"#;
const ICONE_MAIS_ZOOM: &str = r#"<circle cx="11" cy="11" r="7"/><path d="M21 21l-4.3-4.3M8 11h6M11 8v6"/>"#;
const ICONE_INFO: &str = r#"<circle cx="12" cy="12" r="9"/><path d="M12 16v-5M12 8h.01"/>"#;
const ICONE_SETA: &str = r#"<path d="M6 9l6 6 6-6"/>"#;
const ICONE_LINK: &str = r#"<path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7"/><path d="M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7"/>"#;
const ICONE_TAREFA: &str = r#"<rect x="4" y="4" width="16" height="16" rx="4"/><path d="M8.5 12.5l2.5 2.5 4.5-5"/>"#;
const ICONE_CERTO: &str = r#"<path d="M5 12.5l4.5 4.5L19 7.5" stroke-width="3"/>"#;
const ICONE_ALFINETE: &str = r#"<path d="M9 4h6l-1 6 3 3v2H7v-2l3-3z"/><path d="M12 15v5"/>"#;
const ICONE_PASTA: &str = r#"<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#;

thread_local! {
    // Cada SVG é analisado uma vez; o mesmo `Svg` também reaproveita o cache de rasterização.
    static SVGS: std::cell::RefCell<std::collections::HashMap<&'static str, Svg>> = Default::default();
}

fn svg_cache(chave: &'static str, fonte: impl FnOnce() -> String) -> Svg {
    SVGS.with(|c| c.borrow_mut().entry(chave).or_insert_with(|| Svg::from_bytes(fonte().as_bytes()).expect("SVG embutido válido")).clone())
}

fn traco(caminho: &'static str, tamanho: f32, cor: Color) -> SvgView {
    let s = svg_cache(caminho, || format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{caminho}</svg>"##));
    svg(s).width(tamanho).height(tamanho).color(cor)
}

/// A pena da marca em azul, sem o quadrado de fundo.
fn pena(tamanho: f32) -> SvgView {
    let s = svg_cache("pena", || {
        let mut p = include_str!("../assets/pena.svg").replace("#287846", COR_APP).replace("#72aa86", "#7fb0f0");
        if let Some(i) = p.find("<rect id=\"fundo-arredondado\"") {
            let fim = p[i..].find("/>").map_or(i, |f| i + f + 2);
            p.replace_range(i..fim, "");
        }
        p
    });
    svg(s).width(tamanho).height(tamanho)
}

fn caixa(cor: Color, borda: Border) -> impl Fn(&Theme) -> Style {
    move |_| Style::default().background(cor).border(borda)
}

/// Botão sem fundo que acende ao passar o mouse.
fn plano(c: Cores, vermelho: bool) -> impl Fn(&Theme, Status) -> Style {
    move |_, s| {
        let sobre = matches!(s, Status::Hovered | Status::Pressed);
        let bg = sobre.then(|| if vermelho { Color::rgb8(0xe8, 0x11, 0x23) } else { c.sobre });
        Style { background: bg.map(Into::into), border: Border::rounded(6.0), text_color: Some(if sobre && vermelho { Color::WHITE } else { c.texto }), ..Default::default() }
    }
}

fn pilula<'a>(c: Cores, rotulo: &'a str, ativo: bool, m: Msg) -> Button<'a, Msg> {
    button(text(rotulo).size(12).color(if ativo { Color::WHITE } else { c.texto })).on_press(m).padding([4u16, 10]).style(move |t, s| {
        let mut st = plano(c, false)(t, s);
        if ativo {
            st.background = Some(c.acento.into());
        }
        st
    })
}

/// Os prefixos de linha do Markdown que um atalho troca entre si.
fn prefixo(linha: &str) -> usize {
    // Recuo antes do sinal (" ## título") conta como parte do prefixo.
    let recuo = linha.len() - linha.trim_start().len();
    recuo + sinal_de_linha(&linha[recuo..])
}

fn sinal_de_linha(l: &str) -> usize {
    let titulo = l.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&titulo) {
        return titulo + usize::from(l[titulo..].starts_with(' '));
    }
    for p in ["- [ ] ", "- [x] ", "- [X] ", "- ", "* ", "+ "] {
        if l.starts_with(p) {
            return p.len();
        }
    }
    let n = l.bytes().take_while(u8::is_ascii_digit).count();
    if n > 0 && (l[n..].starts_with(". ") || l[n..].starts_with(") ")) {
        return n + 2;
    }
    0
}

fn formatar(c: &Content, f: Formato) {
    let linha = |novo: String| move |l: &str| {
        let p = prefixo(l);
        // O mesmo atalho de novo tira o prefixo.
        if l[..p].trim_start() == novo { (p, String::new()) } else { (p, novo.clone()) }
    };
    match f {
        Formato::Negrito => c.wrap_selection("**", "**", prefixo),
        Formato::Italico => c.wrap_selection("*", "*", prefixo),
        Formato::Tachado => c.wrap_selection("~~", "~~", prefixo),
        Formato::Codigo => c.wrap_selection("`", "`", prefixo),
        Formato::Link => c.wrap_selection("[", "](https://)", prefixo),
        Formato::Titulo(n) => c.edit_lines(linha(format!("{} ", "#".repeat(n)))),
        Formato::Lista => c.edit_lines(linha("- ".into())),
        Formato::Numerada => c.edit_lines(linha("1. ".into())),
        Formato::Tarefa => c.edit_lines(linha("- [ ] ".into())),
    }
}

/// Como o caractere oculto aparece no rodapé.
fn nome_oculto(c: char) -> String {
    let nome = match c {
        '\u{0}' => "NUL",
        '\u{a0}' => "NBSP, espaço não separável",
        '\u{ad}' => "SHY, hífen suave",
        '\u{200b}' => "ZWSP, espaço de largura zero",
        '\u{200c}' => "ZWNJ, não-junção de largura zero",
        '\u{200d}' => "ZWJ, junção de largura zero",
        '\u{200e}' => "LRM, marca esquerda-direita",
        '\u{200f}' => "RLM, marca direita-esquerda",
        '\u{202a}' => "LRE",
        '\u{202b}' => "RLE",
        '\u{202c}' => "PDF",
        '\u{202d}' => "LRO",
        '\u{202e}' => "RLO, inverte o texto",
        '\u{202f}' => "NNBSP, espaço estreito não separável",
        '\u{2028}' => "separador de linha",
        '\u{2029}' => "separador de parágrafo",
        '\u{2060}' => "WJ, junção de palavra",
        '\u{2066}' => "LRI",
        '\u{2067}' => "RLI",
        '\u{2068}' => "FSI",
        '\u{2069}' => "PDI",
        '\u{3000}' => "espaço ideográfico",
        '\u{feff}' => "BOM, marca de ordem de bytes",
        '\u{2000}'..='\u{200a}' => "espaço tipográfico",
        _ if c.is_control() => "caractere de controle",
        _ => "",
    };
    if nome.is_empty() { format!("U+{:04X}", c as u32) } else { format!("U+{:04X} {nome}", c as u32) }
}

// ---------- Tela ----------

impl Notas {
    /// Uma linha só: a marca, as abas, o espaço de arrastar, o menu e os botões da janela.
    fn barra(&self, c: Cores) -> Element<'_, Msg> {
        let nome = row![pena(22.0), text("FeatherNotes").size(15).font(Font::SEMIBOLD).color(c.texto)].spacing(10).align(Align::Center).padding(Padding { left: 16.0, right: 18.0, ..Padding::ZERO }).height(ALTURA_BARRA);
        let arrastar = |e: Element<'static, Msg>| mouse_area(e).on_press(Msg::Arrastar).on_double_click(Msg::Maximizar);
        let botao = |icone, m, fechar| button(container(traco(icone, 15.0, c.texto)).center()).on_press(m).width(42.0).height(32.0).padding(0.0).style(plano(c, fechar));
        row![
            arrastar(nome.into()),
            self.abas(c),
            arrastar(Space::new(Length::FILL, ALTURA_BARRA).into()),
            row![botao(ICONE_INFO, Msg::Propriedades, false), botao(ICONE_MENU, Msg::Menu, false), botao(ICONE_MINIMIZAR, Msg::Minimizar, false), botao(ICONE_MAXIMIZAR, Msg::Maximizar, false), botao(ICONE_X, Msg::Fechar, true)].spacing(2).padding(Padding { right: 8.0, ..Padding::ZERO }),
        ]
        .align(Align::Center)
        .into()
    }

    fn abas(&self, c: Cores) -> Element<'_, Msg> {
        let mut linha = row![].spacing(4).align(Align::End);
        // As abas encolhem para caber ao lado da marca e dos botões (uns 420 px).
        let largura = ((self.largura - 460.0) / self.abas.len() as f32 - 4.0).clamp(90.0, 190.0);
        for (i, aba) in self.abas.iter().enumerate() {
            let ativa = i == self.ativa;
            let mut nome = aba.nome();
            if nome.chars().count() > 26 {
                nome = nome.chars().take(24).collect::<String>() + "…";
            }
            // Alterada: um ponto no lugar do ×, como nos editores de código.
            let fechar: Element<'_, Msg> = if aba.alterada() { text("●").size(10).color(c.mudo).into() } else { traco(ICONE_X, 13.0, c.mudo).into() };
            let x = button(container(fechar).center()).on_press(Msg::FecharAba(aba.id)).width(22.0).height(22.0).padding(0.0).style(plano(c, false));
            let corpo = row![text(nome).size(13).color(if ativa { c.texto } else { c.mudo }), horizontal_space(), x].spacing(10).align(Align::Center).padding(Padding { left: 14.0, right: 6.0, ..Padding::ZERO }).width(largura).height(34.0);
            let fundo = if ativa { c.papel } else { Color::TRANSPARENT };
            let aba_ = button(corpo).on_press(Msg::Aba(i)).padding(0.0).style(move |_, s| {
                let bg = if ativa { fundo } else if matches!(s, Status::Hovered | Status::Pressed) { c.sobre } else { fundo };
                Style { background: Some(bg.into()), border: Border { radius: [8.0, 8.0, 0.0, 0.0], ..Border::default() }, ..Default::default() }
            });
            linha = linha.push(aba_);
        }
        let mais = button(container(traco(ICONE_MAIS, 16.0, c.texto)).center()).on_press(Msg::Nova).width(32.0).height(28.0).padding(0.0).style(move |_, s| {
            let bg = if matches!(s, Status::Hovered | Status::Pressed) { c.sobre.alpha(c.sobre.a * 2.0) } else { c.sobre };
            Style { background: Some(bg.into()), border: Border::rounded(6.0), ..Default::default() }
        });
        linha.push(container(mais).padding(Padding { bottom: 3.0, left: 4.0, ..Padding::ZERO })).height(ALTURA_BARRA).into()
    }

    /// Localizar e substituir: um painel flutuante no canto, por cima do texto.
    fn busca(&self, c: Cores) -> Option<Element<'_, Msg>> {
        let b = self.busca.as_ref()?;
        fn campo<'a>(c: Cores, dica: &'a str, valor: &'a str, m: fn(String) -> Msg, foco: Option<u64>) -> Container<'a, Msg> {
            let mut t = text_input(dica, valor).on_input(m).on_submit(Msg::Proximo).size(13).padding(0.0).style(move |_, _| Style::default().text_color(c.texto));
            if let Some(f) = foco {
                t = t.autofocus(f);
            }
            container(t)
                .padding([0u16, 10])
                .width(Length::FILL)
                .height(30.0)
                .align_y(Align::Center)
                .style(caixa(c.rodape, Border::rounded(6.0).color(c.borda, 1.0)))
        }
        let botao = |rotulo, m| button(text(rotulo).size(12)).on_press(m).padding([6u16, 10]).style(plano(c, false));
        let aa = button(text("Aa").size(12).font(Font::SEMIBOLD)).on_press(Msg::Maiusculas).width(34.0).padding([6u16, 0]).style(move |t, s| {
            let mut st = plano(c, false)(t, s);
            if b.maiusculas {
                st.background = Some(c.acento.alpha(0.18).into());
            }
            st
        });
        let fechar = button(container(traco(ICONE_X, 13.0, c.mudo)).center()).on_press(Msg::Esc).width(28.0).height(28.0).padding(0.0).style(plano(c, false));
        let painel = container(
            column![
                row![campo(c, "Localizar", &b.termo, Msg::Termo, Some(self.foco)), tooltip(aa, "Diferenciar maiúsculas", TooltipPosition::Bottom), botao("Próximo", Msg::Proximo), fechar].spacing(6).align(Align::Center),
                row![campo(c, "Substituir por", &b.troca, Msg::Troca, None), botao("Substituir", Msg::Substituir), botao("Tudo", Msg::SubstituirTudo), Space::new(28.0, 0.0)].spacing(6).align(Align::Center),
            ]
            .spacing(6),
        )
        .padding(8.0)
        .width(440.0)
        .style(move |_| Style::default().background(c.papel).border(Border::rounded(10.0).color(c.borda, 1.0)).shadow(Shadow { color: Color::BLACK.alpha(0.16), offset: Point::new(0.0, 6.0), blur: 18.0 }));
        Some(container(painel).width(Length::FILL).align_x(Align::End).padding(Padding { top: ALTURA_BARRA + 8.0, right: 22.0, ..Padding::ZERO }).into())
    }

    /// "Ir para a linha": um campo só, no mesmo canto do localizar.
    fn ir_para(&self, c: Cores) -> Option<Element<'_, Msg>> {
        let valor = self.ir_para.as_deref()?;
        let total = self.conteudo().line_count();
        let dica = format!("Linha (1 a {total})");
        let campo = container(text_input(dica, valor).on_input(Msg::Linha).on_submit(Msg::IrParaLinha).autofocus(self.foco).size(13).padding(0.0).style(move |_, _| Style::default().text_color(c.texto)))
            .padding([0u16, 10])
            .width(Length::FILL)
            .height(30.0)
            .align_y(Align::Center)
            .style(caixa(c.rodape, Border::rounded(6.0).color(c.borda, 1.0)));
        let ir = button(text("Ir").size(12)).on_press(Msg::IrParaLinha).padding([6u16, 12]).style(plano(c, false));
        let fechar = button(container(traco(ICONE_X, 13.0, c.mudo)).center()).on_press(Msg::Esc).width(28.0).height(28.0).padding(0.0).style(plano(c, false));
        let painel = container(row![text("Ir para a linha").size(13).color(c.texto), campo, ir, fechar].spacing(8).align(Align::Center))
            .padding(8.0)
            .width(360.0)
            .style(move |_| Style::default().background(c.papel).border(Border::rounded(10.0).color(c.borda, 1.0)).shadow(Shadow { color: Color::BLACK.alpha(0.16), offset: Point::new(0.0, 6.0), blur: 18.0 }));
        Some(container(painel).width(Length::FILL).align_x(Align::End).padding(Padding { top: ALTURA_BARRA + 8.0, right: 22.0, ..Padding::ZERO }).into())
    }

    /// A barrinha do Markdown: só aparece com o modo ligado.
    fn formatos(&self, c: Cores) -> Option<Element<'_, Msg>> {
        if !self.abas[self.ativa].markdown {
            return None;
        }
        let b = |rotulo: &'static str, fonte: Font, f: Formato, dica: &'static str| {
            tooltip(button(container(text(rotulo).size(13).font(fonte).color(c.texto)).center()).on_press(Msg::Formatar(f)).width(34.0).height(28.0).padding(0.0).style(plano(c, false)), dica, TooltipPosition::Bottom)
        };
        let icone = |caminho: &'static str, f: Formato, dica: &'static str| {
            tooltip(button(container(traco(caminho, 16.0, c.texto)).center()).on_press(Msg::Formatar(f)).width(34.0).height(28.0).padding(0.0).style(plano(c, false)), dica, TooltipPosition::Bottom)
        };
        let fio = || container(Space::new(1.0, 18.0)).style(caixa(c.borda, Border::default()));
        let linha = row![
            b("H1", Font::BOLD, Formato::Titulo(1), "Título (Ctrl+1)"),
            b("H2", Font::BOLD, Formato::Titulo(2), "Subtítulo (Ctrl+2)"),
            b("H3", Font::BOLD, Formato::Titulo(3), "Título 3 (Ctrl+3)"),
            fio(),
            b("B", Font::BOLD, Formato::Negrito, "Negrito (Ctrl+B)"),
            b("I", Font::SERIF, Formato::Italico, "Itálico (Ctrl+I)"),
            b("S", Font::DEFAULT, Formato::Tachado, "Tachado (Ctrl+Shift+X)"),
            b("</>", Font::MONO, Formato::Codigo, "Código (Ctrl+E)"),
            icone(ICONE_LINK, Formato::Link, "Link (Ctrl+K)"),
            fio(),
            b("•", Font::BOLD, Formato::Lista, "Lista (Ctrl+Shift+8)"),
            b("1.", Font::DEFAULT, Formato::Numerada, "Lista numerada (Ctrl+Shift+7)"),
            icone(ICONE_TAREFA, Formato::Tarefa, "Lista de tarefas (Ctrl+Shift+9)"),
        ]
        .spacing(2)
        .align(Align::Center)
        .padding([4u16, 14]);
        Some(container(linha).width(Length::FILL).style(caixa(c.papel, Border::default())).into())
    }

    fn rodape(&self, c: Cores) -> Element<'_, Msg> {
        let (ln, col) = self.conteudo().cursor();
        let aba = &self.abas[self.ativa];
        let item = |s: String| text(s).size(12).color(c.mudo);
        // Clicáveis como no GNOME: a posição abre o "Ir para a linha", o resto as propriedades.
        let clicavel = |s: String, m: Msg| button(text(s).size(12).color(c.mudo)).on_press(m).padding([3u16, 8]).style(plano(c, false));
        let conteudo = self.conteudo();
        // Cursor sobre um oculto: o nome dele (o inspetor de quem não aparece).
        let sob_cursor = conteudo.char_at_cursor().filter(|&c| is_invisible(c)).map(|c| item(nome_oculto(c)));
        let ocultos = self.prefs.invisiveis.then(|| {
            let chave = (aba.id, conteudo.version());
            let mut cache = self.ocultos.borrow_mut();
            match *cache {
                Some((k, n)) if k == chave => n,
                _ => {
                    let n = conteudo.count_chars(is_invisible);
                    *cache = Some((chave, n));
                    n
                }
            }
        });
        let aviso_ocultos = ocultos.filter(|&n| n > 0).map(|n| {
            let rotulo = if n == 1 { "1 caractere oculto".to_string() } else { format!("{n} caracteres ocultos") };
            button(text(rotulo).size(12).color(Color::rgb8(0xe8, 0x71, 0x0a))).on_press(Msg::ProximoOculto).padding([3u16, 8]).style(plano(c, false))
        });
        let linha = row![clicavel(format!("Ln {ln}, Col {col}"), Msg::IrPara), item(self.recursos.clone())]
            .push_maybe(sob_cursor)
            .push_maybe(aviso_ocultos)
            .push(horizontal_space())
            .push_maybe(self.aviso.as_ref().map(|a| item(a.clone())))
            .push_maybe(aba.acentos_quebrados.then(|| button(text("Acentos quebrados (Ã£)? Consertar").size(12).color(Color::rgb8(0xe8, 0x71, 0x0a))).on_press(Msg::ConsertarAcentos).padding([3u16, 8]).style(plano(c, false))))
            .push(clicavel(aba.cod.nome().into(), Msg::MenuCod(Some(false))))
            .push(clicavel(aba.quebra.nome().into(), Msg::Propriedades))
            .push(clicavel((if aba.markdown { "Markdown" } else { "Texto Simples" }).into(), Msg::Markdown(!aba.markdown)))
            .spacing(16)
            .align(Align::Center)
            .padding([0u16, 10])
            .width(Length::FILL)
            .height(30.0);
        container(linha).style(caixa(c.rodape, Border::default())).into()
    }

    /// O menu ☰, como o do Editor de Texto do GNOME: tema em círculos, zoom e as ações.
    fn menu(&self, c: Cores) -> Element<'_, Msg> {
        let aba = &self.abas[self.ativa];
        let item = |rotulo: &'static str, atalho: &'static str, m: Option<Msg>| {
            let cor = if m.is_some() { c.texto } else { c.mudo.alpha(0.6) };
            button(row![text(rotulo).size(13).color(cor), horizontal_space(), text(atalho).size(12).color(c.mudo)].align(Align::Center)).on_press_maybe(m).width(Length::FILL).padding([7u16, 12]).style(plano(c, false))
        };
        let separador = || container(container(Space::new(Length::FILL, 1.0)).style(caixa(c.borda, Border::default()))).padding([4u16, 0]);
        let pilula = |rotulo: &'static str, ativo: bool, m: Msg| pilula(c, rotulo, ativo, m);
        let rotulo = |s: &'static str| text(s).size(13).color(c.texto);

        let circulo = |fundo: Background, t: Tema| {
            let ativo = self.prefs.tema == t;
            let b = button(Space::new(38.0, 38.0)).on_press(Msg::Tema(t)).padding(0.0).style(move |_, s| {
                let contorno = if ativo { c.acento } else if matches!(s, Status::Hovered) { c.mudo } else { c.borda };
                Style { background: Some(fundo.clone()), border: Border::rounded(19.0).color(contorno, if ativo { 2.0 } else { 1.0 }), ..Default::default() }
            });
            let selo = ativo.then(|| container(container(traco(ICONE_CERTO, 11.0, Color::WHITE)).center().width(16.0).height(16.0).style(caixa(c.acento, Border::rounded(8.0)))).width(40.0).height(40.0).align_x(Align::End).align_y(Align::End));
            let mut st = stack![b];
            if let Some(selo) = selo {
                st = st.push(selo);
            }
            tooltip(st, match t {
                Tema::Automatico => "Seguir o sistema",
                Tema::Claro => "Claro",
                Tema::Escuro => "Escuro",
            }, TooltipPosition::Bottom)
        };
        let escuro = Color::hex("#2b2d31").expect("cor");
        let temas = row![
            horizontal_space(),
            circulo(Background::Linear { angle: std::f32::consts::FRAC_PI_4, stops: vec![(0.0, Color::WHITE), (0.5, Color::WHITE), (0.5, escuro), (1.0, escuro)] }, Tema::Automatico),
            circulo(Color::WHITE.into(), Tema::Claro),
            circulo(escuro.into(), Tema::Escuro),
            horizontal_space(),
        ]
        .spacing(26)
        .padding([8u16, 0]);
        let lupa = |icone, m| button(container(traco(icone, 17.0, c.texto)).center()).on_press(m).width(36.0).height(30.0).padding(0.0).style(plano(c, false));
        let zoom = row![
            lupa(ICONE_MENOS_ZOOM, Msg::Fonte(-1.0)),
            horizontal_space(),
            button(text(format!("{:.0}%", self.prefs.tamanho / TAMANHO_PADRAO * 100.0)).size(13).color(c.mudo)).on_press(Msg::Fonte(0.0)).padding([4u16, 8]).style(plano(c, false)),
            horizontal_space(),
            lupa(ICONE_MAIS_ZOOM, Msg::Fonte(1.0)),
        ]
        .align(Align::Center)
        .padding([0u16, 8]);
        let f = self.prefs.fonte;
        let tipo = row![rotulo("Fonte"), horizontal_space(), pilula("Mono", f == Fonte::Mono, Msg::Fonte_(Fonte::Mono)), pilula("Sem serifa", f == Fonte::Sans, Msg::Fonte_(Fonte::Sans)), pilula("Serifa", f == Fonte::Serif, Msg::Fonte_(Fonte::Serif))]
            .spacing(4)
            .align(Align::Center)
            .padding([4u16, 12]);
        let quebra = row![rotulo("Quebra de linha"), horizontal_space(), pilula(if self.prefs.quebra { "Ligada" } else { "Desligada" }, self.prefs.quebra, Msg::Quebra)].align(Align::Center).padding([4u16, 12]);
        let v = self.prefs.invisiveis;
        let invisiveis = row![rotulo("Caracteres invisíveis"), horizontal_space(), pilula(if v { "Mostrar" } else { "Esconder" }, v, Msg::Invisiveis)].align(Align::Center).padding([4u16, 12]);
        let inicio = self.linha_inicio(c);
        let atalho = cfg!(target_os = "linux").then(|| self.linha_atalho(c));
        let descartar = (aba.caminho.is_some() && aba.alterada()).then_some(Msg::Descartar);
        let cartao = container(
            column![
                temas,
                separador(),
                zoom,
                separador(),
                item("Novo", "Ctrl+N", Some(Msg::Nova)),
                item("Abrir…", "Ctrl+O", Some(Msg::Abrir)),
                item("Novo post-it", "Shift+Ctrl+N", Some(Msg::NovoPostIt)),
                item("Lista de post-its", "", Some(Msg::PainelPostIts)),
                separador(),
                item("Salvar", "Ctrl+S", Some(Msg::Salvar)),
                item("Salvar como…", "Shift+Ctrl+S", Some(Msg::SalvarComo)),
                item("Descartar alterações", "", descartar),
                separador(),
                item("Localizar/substituir…", "Ctrl+F", Some(Msg::Localizar)),
                item("Ir para a linha…", "Ctrl+G", Some(Msg::IrPara)),
                item("Propriedades do arquivo", "Alt+Enter", Some(Msg::Propriedades)),
                item("Tela cheia", "F11", Some(Msg::TelaCheia)),
                item("Sair", "Ctrl+Q", Some(Msg::Sair)),
                separador(),
                tipo,
                quebra,
                invisiveis,
                inicio,
            ]
            .push_maybe(atalho)
            .spacing(1),
        )
        .padding(6.0)
        .width(300.0)
        .style(move |_| {
            Style::default().background(c.papel).border(Border::rounded(12.0).color(c.borda, 1.0)).shadow(Shadow { color: Color::BLACK.alpha(0.18), offset: Point::new(0.0, 6.0), blur: 20.0 })
        });
        stack![mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(Msg::FecharMenu), container(cartao).width(Length::FILL).align_x(Align::End).padding(Padding { top: ALTURA_BARRA, right: 140.0, ..Padding::ZERO })].into()
    }

    /// A lista de codificações: "Converter para" muda como o texto será salvo; "Reabrir com"
    /// lê de novo os bytes do arquivo de outro jeito.
    fn menu_codificacao(&self, c: Cores, reabrir: bool) -> Element<'_, Msg> {
        let atual = self.abas[self.ativa].cod;
        let item = |cod: Cod| {
            let marca = if cod == atual { "●" } else { " " };
            let m = if reabrir { Msg::Reabrir(cod) } else { Msg::Codificacao(cod) };
            button(row![text(marca).size(10).color(c.acento).width(14.0), text(cod.nome()).size(13).color(c.texto), horizontal_space()].align(Align::Center).width(Length::FILL)).on_press(m).width(Length::FILL).padding([6u16, 10]).style(plano(c, false))
        };
        let separador = || container(container(Space::new(Length::FILL, 1.0)).style(caixa(c.borda, Border::default()))).padding([4u16, 0]);
        let modo = row![pilula(c, "Converter para", !reabrir, Msg::MenuCod(Some(false))), pilula(c, "Reabrir com", reabrir, Msg::MenuCod(Some(true)))].spacing(4).padding([4u16, 6]);
        let mut lista = column![].spacing(1);
        for &cod in codificacao::PRINCIPAIS {
            lista = lista.push(item(cod));
        }
        lista = lista.push(separador()).push(
            button(row![text("Outras codificações").size(13).color(c.texto), horizontal_space(), text(if self.outras_cod { "▾" } else { "▸" }).size(12).color(c.mudo)].align(Align::Center).width(Length::FILL))
                .on_press(Msg::OutrasCod)
                .width(Length::FILL)
                .padding([6u16, 10])
                .style(plano(c, false)),
        );
        if self.outras_cod {
            for &cod in codificacao::OUTRAS {
                lista = lista.push(item(cod));
            }
        }
        let rodape = column![
            separador(),
            button(row![text("Detectar automaticamente").size(13).color(c.texto), horizontal_space()].width(Length::FILL)).on_press(Msg::Detectar).width(Length::FILL).padding([6u16, 10]).style(plano(c, false)),
        ];
        let dica = text(if reabrir { "Lê os bytes do arquivo de novo com a codificação escolhida." } else { "O texto continua igual; muda como ele será salvo." }).size(11).color(c.mudo);
        let altura = (self.altura - ALTURA_BARRA - 160.0).max(160.0);
        let cartao = container(column![modo, container(dica).padding([0u16, 10]), separador(), if self.outras_cod { container(scrollable(lista)).height(Length::Fixed(altura)) } else { container(lista) }, rodape].spacing(2))
            .padding(6.0)
            .width(300.0)
            .style(move |_| Style::default().background(c.papel).border(Border::rounded(12.0).color(c.borda, 1.0)).shadow(Shadow { color: Color::BLACK.alpha(0.18), offset: Point::new(0.0, 6.0), blur: 20.0 }));
        stack![mouse_area(Space::new(Length::FILL, Length::FILL)).on_press(Msg::MenuCod(None)), container(cartao).width(Length::FILL).align_x(Align::End).padding(Padding { top: ALTURA_BARRA + 8.0, right: 16.0, ..Padding::ZERO })].into()
    }

    /// Propriedades do arquivo, num painel à direita (como o ⓘ do Editor de Texto do GNOME).
    fn propriedades(&self, c: Cores) -> Element<'_, Msg> {
        let aba = &self.abas[self.ativa];
        let conteudo = self.conteudo();
        // Contar o texto inteiro só quando ele muda, não a cada quadro.
        let chave = (aba.id, conteudo.version());
        let [linhas, palavras, sem_espaco, todos] = {
            let mut cache = self.estatisticas.borrow_mut();
            match *cache {
                Some((k, n)) if k == chave => n,
                _ => {
                    let t = conteudo.text();
                    let n = [conteudo.line_count(), t.split_whitespace().count(), t.chars().filter(|c| !c.is_whitespace()).count(), t.chars().filter(|&c| c != '\n' && c != '\r').count()];
                    *cache = Some((chave, n));
                    n
                }
            }
        };
        let campo = |titulo: &'static str, valor: String| column![text(titulo).size(11).color(c.mudo), text(valor).size(13).color(c.texto)].spacing(3).padding([10u16, 14]).width(Length::FILL);
        let linha = || container(Space::new(Length::FILL, 1.0)).style(caixa(c.borda, Border::default()));
        let cartao = |filhos: Vec<Element<'static, Msg>>| container(column(filhos)).width(Length::FILL).style(caixa(c.papel, Border::rounded(10.0).color(c.borda, 1.0)));
        let escolha = |titulo: &'static str, a: &'static str, b: &'static str, em_b: bool, m: fn(bool) -> Msg| {
            row![text(titulo).size(13).color(c.texto), horizontal_space(), pilula(c, a, !em_b, m(false)), pilula(c, b, em_b, m(true))].spacing(4).align(Align::Center).padding([10u16, 14])
        };
        let local = aba.caminho.as_ref().and_then(|p| p.parent()).map_or("Ainda não salvo".into(), |p| p.display().to_string());
        let pasta = aba.caminho.is_some().then(|| tooltip(button(container(traco(ICONE_PASTA, 16.0, c.texto)).center()).on_press(Msg::AbrirPasta).width(30.0).height(30.0).padding(0.0).style(plano(c, false)), "Abrir a pasta", TooltipPosition::Bottom));
        let arquivo = cartao(vec![
            campo("Nome do arquivo", aba.nome()).into(),
            linha().into(),
            row![campo("Local", local)].push_maybe(pasta).align(Align::Center).padding(Padding { right: 8.0, ..Padding::ZERO }).into(),
            linha().into(),
            escolha("Tipo", "Texto simples", "Markdown", aba.markdown, Msg::Markdown).into(),
            linha().into(),
            row![
                text("Codificação").size(13).color(c.texto),
                horizontal_space(),
                button(row![text(aba.cod.nome()).size(13).color(c.texto), traco(ICONE_SETA, 14.0, c.mudo)].spacing(6).align(Align::Center)).on_press(Msg::MenuCod(Some(false))).padding([5u16, 10]).style(plano(c, false)),
            ]
            .align(Align::Center)
            .padding([6u16, 14])
            .into(),
            linha().into(),
            row![
                text("Quebra de linha").size(13).color(c.texto),
                horizontal_space(),
                pilula(c, "LF", aba.quebra == Quebra::Lf, Msg::QuebraLinha(Quebra::Lf)),
                pilula(c, "CRLF", aba.quebra == Quebra::Crlf, Msg::QuebraLinha(Quebra::Crlf)),
                pilula(c, "CR", aba.quebra == Quebra::Cr, Msg::QuebraLinha(Quebra::Cr)),
            ]
            .spacing(4)
            .align(Align::Center)
            .padding([10u16, 14])
            .into(),
        ]);
        let numero = |n: usize| n.to_string();
        let estatisticas = cartao(vec![
            campo("Linhas", numero(linhas)).into(),
            linha().into(),
            campo("Palavras", numero(palavras)).into(),
            linha().into(),
            campo("Caracteres, sem espaços", numero(sem_espaco)).into(),
            linha().into(),
            campo("Todos os caracteres", numero(todos)).into(),
        ]);
        let painel = column![arquivo, text("Estatísticas").size(14).font(Font::SEMIBOLD).color(c.texto), estatisticas].spacing(14).padding(16.0).width(Length::FILL);
        row![container(Space::new(1.0, Length::FILL)).style(caixa(c.borda, Border::default())), container(scrollable(painel)).width(320.0).height(Length::FILL).style(caixa(c.rodape, Border::default()))].into()
    }

    fn confirmacao(&self, c: Cores, id: u64) -> Element<'_, Msg> {
        let nome = self.abas.iter().find(|a| a.id == id).map_or_else(String::new, Aba::nome);
        let botao = |rotulo, m, principal: bool| {
            button(text(rotulo).size(13).color(if principal { Color::WHITE } else { c.texto })).on_press(Msg::Confirmar(m)).padding([7u16, 16]).style(move |t, s| {
                let mut st = plano(c, false)(t, s);
                if principal {
                    st.background = Some(c.acento.into());
                }
                st
            })
        };
        let cartao = container(
            column![
                text(format!("Salvar as alterações em “{nome}”?")).size(15).font(Font::SEMIBOLD).color(c.texto),
                text("Se não salvar, o que mudou desde a última vez se perde.").size(13).color(c.mudo),
                row![horizontal_space(), botao("Cancelar", Resposta::Cancelar, false), botao("Não salvar", Resposta::Descartar, false), botao("Salvar", Resposta::Salvar, true)].spacing(8),
            ]
            .spacing(14),
        )
        .padding(20.0)
        .width(440.0)
        .style(move |_| Style::default().background(c.papel).border(Border::rounded(12.0).color(c.borda, 1.0)).shadow(Shadow { color: Color::BLACK.alpha(0.25), offset: Point::new(0.0, 10.0), blur: 30.0 }));
        let veu = container(Space::new(Length::FILL, Length::FILL)).style(caixa(Color::BLACK.alpha(0.25), Border::rounded(RAIO_JANELA)));
        stack![mouse_area(veu).on_press(Msg::Confirmar(Resposta::Cancelar)), container(cartao).width(Length::FILL).height(Length::FILL).center()].into()
    }

    fn linha_inicio(&self, c: Cores) -> Row<'_, Msg> {
        let v = self.inicio_sistema;
        row![text("Abrir ao iniciar o sistema").size(13).color(c.texto), horizontal_space(), pilula(c, if v { "Ligado" } else { "Desligado" }, v, Msg::IniciarComSistema(!v))].align(Align::Center).padding([4u16, 12])
    }

    fn linha_atalho(&self, c: Cores) -> Row<'_, Msg> {
        let v = self.atalho_postit;
        let rotulo = column![text("Atalho para novo post-it").size(13).color(c.texto), text(arquivo::ATALHO_POSTIT).size(11).color(c.mudo)];
        row![rotulo, horizontal_space(), pilula(c, if v { "Ligado" } else { "Desligado" }, v, Msg::AtalhoPostIt(!v))].align(Align::Center).padding([4u16, 12])
    }

    /// A configuração inicial: só na primeira vez que o app abre.
    fn boas_vindas(&self, c: Cores) -> Element<'_, Msg> {
        let pronto = button(text("Pronto").size(13).color(Color::WHITE)).on_press(Msg::Configurado).padding([7u16, 18]).style(move |t, s| {
            let mut st = plano(c, false)(t, s);
            st.background = Some(c.acento.into());
            st
        });
        let cartao = container(
            column![
                row![pena(24.0), text("Configuração inicial").size(16).font(Font::SEMIBOLD).color(c.texto)].spacing(10).align(Align::Center),
                text("Os post-its que estavam abertos voltam junto quando o FeatherNotes abre.").size(13).color(c.mudo),
                column![self.linha_inicio(c)].push_maybe(cfg!(target_os = "linux").then(|| self.linha_atalho(c))).spacing(4),
                row![text("Dá para mudar depois no menu ☰.").size(12).color(c.mudo), horizontal_space(), pronto].align(Align::Center),
            ]
            .spacing(14),
        )
        .padding(22.0)
        .width(440.0)
        .style(move |_| Style::default().background(c.papel).border(Border::rounded(12.0).color(c.borda, 1.0)).shadow(Shadow { color: Color::BLACK.alpha(0.25), offset: Point::new(0.0, 10.0), blur: 30.0 }));
        let veu = container(Space::new(Length::FILL, Length::FILL)).style(caixa(Color::BLACK.alpha(0.25), Border::rounded(RAIO_JANELA)));
        // O véu não fecha: só o "Pronto".
        stack![mouse_area(veu), container(cartao).width(Length::FILL).height(Length::FILL).center()].into()
    }

    fn alcas(&self) -> Element<'_, Msg> {
        let alca = |w: Length, h: Length, d: ResizeDirection, i: CursorIcon| mouse_area(Space::new(w, h)).on_press(Msg::Puxar(d)).cursor_icon(i);
        stack![
            container(alca(Length::Fixed(5.0), Length::FILL, ResizeDirection::East, CursorIcon::EResize)).width(Length::FILL).height(Length::FILL).align_x(Align::End),
            container(alca(Length::Fixed(5.0), Length::FILL, ResizeDirection::West, CursorIcon::WResize)).width(Length::FILL).height(Length::FILL),
            container(alca(Length::FILL, Length::Fixed(5.0), ResizeDirection::South, CursorIcon::SResize)).width(Length::FILL).height(Length::FILL).align_y(Align::End),
            container(alca(Length::FILL, Length::Fixed(4.0), ResizeDirection::North, CursorIcon::NResize)).width(Length::FILL).height(Length::FILL),
            container(alca(Length::Fixed(14.0), Length::Fixed(14.0), ResizeDirection::SouthEast, CursorIcon::SeResize)).width(Length::FILL).height(Length::FILL).align_x(Align::End).align_y(Align::End),
        ]
        .into()
    }
}

impl App for Notas {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Mudou => return self.mudou(),
            Msg::Tamanho(t) => (self.largura, self.altura) = (t.width, t.height),
            Msg::MenuCod(m) => self.menu_cod = m,
            Msg::OutrasCod => self.outras_cod = !self.outras_cod,
            Msg::Reabrir(cod) => {
                self.menu_cod = None;
                self.reabrir(Some(cod));
            }
            Msg::Detectar => {
                self.menu_cod = None;
                self.reabrir(None);
            }
            Msg::ConsertarAcentos => {
                let aba = &mut self.abas[self.ativa];
                aba.acentos_quebrados = false;
                if let Some(c) = &aba.conteudo {
                    c.map_lines(|l| codificacao::consertar_acentos(l).unwrap_or_else(|| l.to_owned()));
                }
                return self.mudou();
            }
            Msg::Medir => {
                let agora = consumo::tempos_de_cpu();
                let cpu = match (agora, self.cpu_antes) {
                    (Some((p, t)), Some((p0, t0))) => format!(" · CPU {:.1}%", (p.saturating_sub(p0) as f32 / t.saturating_sub(t0).max(1) as f32 * 100.0).clamp(0.0, 100.0)).replace('.', ","),
                    _ => String::new(),
                };
                self.cpu_antes = agora;
                self.recursos = format!("RAM {} MB{cpu}", consumo::rss_kb().unwrap_or(0).div_ceil(1024));
            }
            Msg::Fonte_(f) => self.mudar_prefs(|p| p.fonte = f),
            Msg::Markdown(m) => {
                self.abas[self.ativa].markdown = m;
                self.guardar_sessao();
            }
            Msg::Formatar(f) => {
                formatar(self.conteudo(), f);
                return self.mudou();
            }
            Msg::Invisiveis => self.mudar_prefs(|p| p.invisiveis = !p.invisiveis),
            Msg::ProximoOculto => {
                self.conteudo().find_char(is_invisible);
            }
            Msg::IrPara => {
                self.menu = false;
                self.ir_para = Some(String::new());
                self.foco += 1;
            }
            Msg::Linha(t) => self.ir_para = Some(t.chars().filter(char::is_ascii_digit).collect()),
            Msg::IrParaLinha => {
                if let Some(n) = self.ir_para.take().and_then(|t| t.parse().ok()) {
                    self.conteudo().go_to_line(n);
                }
                self.foco += 1;
            }
            Msg::Propriedades => {
                self.menu = false;
                self.propriedades = !self.propriedades;
            }
            Msg::Codificacao(cod) => {
                self.menu_cod = None;
                let aba = &mut self.abas[self.ativa];
                if aba.cod != cod {
                    aba.cod = cod;
                    aba.salvo = None;
                    // Avisar já, não só ao salvar.
                    if let Some(Err(f)) = aba.conteudo.as_ref().map(|c| cod.gravar(&c.text())) {
                        self.aviso = Some(format!("{} caractere(s) não cabem em {}: o arquivo não salva assim", f.len(), cod.nome()));
                    }
                }
            }
            Msg::QuebraLinha(q) => {
                let aba = &mut self.abas[self.ativa];
                if aba.quebra != q || !aba.converter {
                    aba.quebra = q;
                    aba.converter = true;
                    aba.salvo = None;
                }
            }
            Msg::AbrirPasta => {
                if let Some(pasta) = self.abas[self.ativa].caminho.as_ref().and_then(|p| p.parent()) {
                    let programa = if cfg!(windows) { "explorer" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
                    if let Err(e) = std::process::Command::new(programa).arg(pasta).spawn() {
                        self.aviso = Some(format!("Não abriu a pasta: {e}"));
                    }
                }
            }
            Msg::Descartar => {
                self.menu = false;
                let aba = &mut self.abas[self.ativa];
                let _ = std::fs::remove_file(arquivo::recuperacao(aba.id));
                aba.conteudo = None;
                aba.converter = false;
                self.carregar(self.ativa);
                self.guardar_sessao();
            }
            Msg::TelaCheia => {
                self.menu = false;
                return Task::window(WindowCmd::ToggleFullscreen);
            }
            Msg::Copiar => {
                self.copia_agendada = false;
                self.guardar_sessao();
            }
            Msg::Aba(i) => {
                self.ativar(i);
                self.guardar_sessao();
            }
            Msg::ProximaAba => self.ativar((self.ativa + 1) % self.abas.len()),
            Msg::Nova => {
                self.menu = false;
                self.nova_aba(None);
                self.guardar_sessao();
            }
            Msg::FecharAba(id) => {
                let alterada = self.abas.iter().find(|a| a.id == id).is_some_and(Aba::alterada);
                if alterada {
                    // Precisa do texto na tela para salvar.
                    if let Some(i) = self.abas.iter().position(|a| a.id == id) {
                        self.ativar(i);
                    }
                    self.confirmar = Some(id);
                } else {
                    self.fechar_aba(id);
                }
            }
            Msg::Confirmar(r) => {
                let Some(id) = self.confirmar.take() else { return Task::none() };
                match r {
                    Resposta::Salvar => return self.salvar_aba(id, false, true),
                    Resposta::Descartar => self.fechar_aba(id),
                    Resposta::Cancelar => {}
                }
            }
            Msg::Abrir => {
                self.menu = false;
                let pasta = self.abas[self.ativa].caminho.as_ref().and_then(|p| p.parent()).map(|p| p.to_path_buf()).or_else(dirs::document_dir);
                return Task::perform(
                    async move {
                        let mut d = rfd::AsyncFileDialog::new().add_filter("Texto", &["txt", "md", "log", "csv", "ini", "conf"]).add_filter("Todos os arquivos", &["*"]);
                        if let Some(p) = pasta {
                            d = d.set_directory(p);
                        }
                        d.pick_files().await.unwrap_or_default().into_iter().map(|f| f.path().to_path_buf()).collect()
                    },
                    Msg::Abertos,
                );
            }
            Msg::Abertos(caminhos) => {
                for p in caminhos {
                    match self.abas.iter().position(|a| a.caminho.as_ref() == Some(&p)) {
                        Some(i) => self.ativar(i),
                        None => {
                            // Só a aba vazia de quando o app abriu: o arquivo toma o lugar dela.
                            let aba = &self.abas[self.ativa];
                            if self.abas.len() == 1 && aba.caminho.is_none() && !aba.alterada() && aba.conteudo.as_ref().is_some_and(|c| c.text().is_empty()) {
                                self.abas[0] = Aba::nova(aba.id, Some(p));
                                self.ativar(0);
                            } else {
                                self.nova_aba(Some(p));
                            }
                        }
                    }
                }
                self.guardar_sessao();
            }
            Msg::Salvar => {
                self.menu = false;
                return self.salvar_aba(self.abas[self.ativa].id, false, false);
            }
            Msg::SalvarComo => {
                self.menu = false;
                return self.salvar_aba(self.abas[self.ativa].id, true, false);
            }
            Msg::SalvarEm(id, Some(p), fechar) => {
                if self.salvar(id, p) && fechar {
                    self.fechar_aba(id);
                }
            }
            Msg::SalvarEm(_, None, _) => {}
            Msg::Menu => self.menu = !self.menu,
            Msg::FecharMenu => self.menu = false,
            Msg::Fonte(d) => self.mudar_prefs(|p| p.tamanho = if d == 0.0 { TAMANHO_PADRAO } else { (p.tamanho + d).clamp(8.0, 48.0) }),
            Msg::Quebra => self.mudar_prefs(|p| p.quebra = !p.quebra),
            Msg::Tema(t) => self.mudar_prefs(|p| p.tema = t),
            Msg::Localizar => {
                self.menu = false;
                if self.busca.is_none() {
                    self.busca = Some(Busca { termo: String::new(), troca: String::new(), maiusculas: false });
                }
                self.ir_para = None;
                self.foco += 1;
            }
            Msg::Termo(t) => {
                if let Some(b) = &mut self.busca {
                    b.termo = t;
                }
            }
            Msg::Troca(t) => {
                if let Some(b) = &mut self.busca {
                    b.troca = t;
                }
            }
            Msg::Maiusculas => {
                if let Some(b) = &mut self.busca {
                    b.maiusculas = !b.maiusculas;
                }
            }
            Msg::Proximo | Msg::Substituir | Msg::SubstituirTudo => {
                let Some(b) = &self.busca else { return Task::none() };
                if b.termo.is_empty() {
                    return Task::none();
                }
                let c = self.conteudo();
                let antes = c.version();
                let aviso = match msg {
                    Msg::SubstituirTudo => Some(match c.replace_all(&b.termo, &b.troca, b.maiusculas) {
                        0 => "Não encontrado".into(),
                        1 => "1 substituição".into(),
                        n => format!("{n} substituições"),
                    }),
                    Msg::Substituir => (!c.replace_next(&b.termo, &b.troca, b.maiusculas)).then(|| "Não encontrado".into()),
                    _ => (!c.find_next(&b.termo, b.maiusculas)).then(|| "Não encontrado".into()),
                };
                let mudou = c.version() != antes;
                let t = if mudou { self.mudou() } else { Task::none() };
                self.aviso = aviso;
                return t;
            }
            Msg::Esc => {
                if self.confirmar.take().is_none() && !std::mem::take(&mut self.menu) && self.menu_cod.take().is_none() && self.ir_para.take().is_none() {
                    self.busca = None;
                }
                self.foco += 1;
            }
            Msg::Arrastar => return Task::window(WindowCmd::Drag),
            Msg::Puxar(d) => return Task::window(WindowCmd::DragResize(d)),
            Msg::Minimizar => return Task::window(WindowCmd::Minimize),
            Msg::Maximizar => return Task::window(WindowCmd::ToggleMaximize),
            Msg::Pedido(p) => match p.as_str() {
                "postit" => return self.update(Msg::NovoPostIt),
                _ => return self.update(Msg::MostrarPrincipal),
            },
            Msg::IniciarComSistema(ligar) => {
                match arquivo::iniciar_com_sistema(ligar) {
                    Ok(()) => self.inicio_sistema = ligar,
                    Err(e) => self.aviso = Some(format!("Não deu para mudar o início com o sistema: {e}")),
                }
            }
            Msg::AtalhoPostIt(ligar) => match arquivo::atalho_postit(ligar) {
                Ok(()) => self.atalho_postit = ligar,
                Err(e) => self.aviso = Some(format!("Não deu para mudar o atalho: {e}")),
            },
            Msg::Configurado => self.mudar_prefs(|p| p.configurado = true),
            Msg::NovoPostIt => {
                self.menu = false;
                let id = self.postits.iter().map(|p| p.id + 1).max().unwrap_or(1);
                // A cor seguinte à do último: post-its novos não ficam todos iguais.
                let cor = self.postits.last().map_or(0, |p| (p.cor + 1) % CORES_POSTIT.len());
                self.postits.push(PostIt { id, conteudo: Content::new(), cor, aberto: true, fixo: false, posicao: None, excluido: false, confirmar: false, gravado: 0 });
                let t = self.abrir_postit(self.postits.len() - 1);
                self.guardar_postits();
                return t;
            }
            Msg::MostrarPostIts => {
                self.menu = false;
                let fechados: Vec<usize> = (0..self.postits.len()).filter(|&i| !self.postits[i].aberto && !self.postits[i].excluido).collect();
                let t = Task::batch(fechados.into_iter().map(|i| self.abrir_postit(i)).collect::<Vec<_>>());
                self.guardar_postits();
                return t;
            }
            Msg::PostItMudou => return self.mudou(),
            Msg::PostItCor(id) => {
                if let Some(p) = self.postits.iter_mut().find(|p| p.id == id) {
                    p.cor = (p.cor + 1) % CORES_POSTIT.len();
                }
                self.guardar_postits();
            }
            Msg::PostItFixar(id) => {
                let Some(p) = self.postits.iter_mut().find(|p| p.id == id) else { return Task::none() };
                p.fixo = !p.fixo;
                let nivel = if p.fixo { WindowLevel::AlwaysOnBottom } else { WindowLevel::Normal };
                self.guardar_postits();
                return Task::window_of(WindowId(id), WindowCmd::Level(nivel));
            }
            Msg::PostItArrastar(id) => return Task::window_of(WindowId(id), WindowCmd::Drag),
            Msg::PostItPuxar(id, d) => return Task::window_of(WindowId(id), WindowCmd::DragResize(d)),
            Msg::PostItFechar(id) => {
                let Some(p) = self.postits.iter_mut().find(|p| p.id == id) else { return Task::none() };
                if !p.conteudo.text().trim().is_empty() {
                    p.confirmar = true;
                    return Task::none();
                }
                self.postit_fechou(id, false);
                return Task::window_of(WindowId(id), WindowCmd::Close);
            }
            Msg::PostItResposta(id, r) => {
                if let Some(p) = self.postits.iter_mut().find(|p| p.id == id) {
                    p.confirmar = false;
                }
                let Some(excluir) = r else { return Task::none() };
                self.postit_fechou(id, excluir);
                return Task::window_of(WindowId(id), WindowCmd::Close);
            }
            Msg::PostItMoveu(id, ponto) => {
                if let Some(p) = self.postits.iter_mut().find(|p| p.id == id) {
                    p.posicao = Some(ponto);
                }
                return self.mudou();
            }
            Msg::PainelPostIts => {
                self.menu = false;
                self.painel_postits = !self.painel_postits;
            }
            Msg::AbrirPostIt(id) => {
                let Some(i) = self.postits.iter().position(|p| p.id == id) else { return Task::none() };
                if self.postits[i].aberto {
                    return Task::window_of(WindowId(id), WindowCmd::Focus);
                }
                let t = self.abrir_postit(i);
                self.guardar_postits();
                return t;
            }
            Msg::RestaurarPostIt(id) => {
                if let Some(p) = self.postits.iter_mut().find(|p| p.id == id) {
                    p.excluido = false;
                }
                self.guardar_postits();
            }
            Msg::ApagarPostIt(id) => {
                if let Some(i) = self.postits.iter().position(|p| p.id == id && p.excluido) {
                    self.apagar_postit(i);
                }
                self.guardar_postits();
            }
            Msg::MostrarPrincipal => return Task::batch([Task::open_window(WindowId::MAIN, janela_principal(false)), Task::window(WindowCmd::Focus)]),
            Msg::Sair => {
                self.guardar_sessao();
                return Task::window(WindowCmd::Exit);
            }
            Msg::Fechar => {
                // O que não foi salvo fica na recuperação e volta na próxima vez.
                self.guardar_sessao();
                return Task::window(WindowCmd::Close);
            }
        }
        Task::none()
    }

    fn theme(&self) -> Theme {
        let c = self.cores();
        let base = if self.escuro { Theme::dark() } else { Theme::light() };
        Theme { background: Color::TRANSPARENT, surface: c.papel, text: c.texto, muted: c.numeros, primary: c.acento, border: c.borda, ..base }
    }

    fn view_window(&self, id: WindowId) -> Element<'_, Msg> {
        match self.postits.iter().find(|p| WindowId(p.id) == id) {
            Some(p) if id != WindowId::MAIN => self.postit(p),
            _ if id == WindowId::MAIN => self.view(),
            _ => Space::new(Length::FILL, Length::FILL).into(),
        }
    }

    fn on_window_event(&self, id: WindowId, ev: &Event) -> Option<Msg> {
        if id == WindowId::MAIN {
            return self.on_event(ev);
        }
        match ev {
            Event::CloseRequested => Some(Msg::PostItFechar(id.0)),
            // O (0, 0) chega logo que a janela nasce, antes de o gerenciador pôr ela no lugar.
            Event::Moved(p) if *p != Point::ORIGIN => Some(Msg::PostItMoveu(id.0, *p)),
            Event::Key { key: Key::Character(k), modifiers, pressed: true, .. } if modifiers.control_key() && k.eq_ignore_ascii_case("n") => Some(Msg::NovoPostIt),
            _ => None,
        }
    }

    fn on_event(&self, ev: &Event) -> Option<Msg> {
        let Event::Key { key, modifiers, pressed: true, .. } = ev else {
            return match ev {
                Event::CloseRequested => Some(Msg::Fechar),
                Event::Resized(s) => Some(Msg::Tamanho(*s)),
                Event::Wheel { dy, modifiers, .. } if modifiers.control_key() => {
                    let soma = self.zoom.get() + dy;
                    self.zoom.set(if soma.abs() >= 40.0 { 0.0 } else { soma });
                    (soma.abs() >= 40.0).then(|| Msg::Fonte(soma.signum()))
                }
                _ => None,
            };
        };
        match key {
            Key::Named(NamedKey::Enter) if modifiers.alt_key() => return Some(Msg::Propriedades),
            Key::Named(NamedKey::Escape) => return Some(Msg::Esc),
            Key::Named(NamedKey::F11) => return Some(Msg::TelaCheia),
            _ => {}
        }
        if !(modifiers.control_key() || modifiers.super_key()) {
            return None;
        }
        let shift = modifiers.shift_key();
        match key {
            Key::Named(NamedKey::Tab) => Some(Msg::ProximaAba),
            Key::Character(k) => match k.to_lowercase().as_str() {
                "n" if shift => Some(Msg::NovoPostIt),
                "n" | "t" => Some(Msg::Nova),
                "o" => Some(Msg::Abrir),
                "q" => Some(Msg::Sair),
                "s" if shift => Some(Msg::SalvarComo),
                "b" => Some(Msg::Formatar(Formato::Negrito)),
                "i" => Some(Msg::Formatar(Formato::Italico)),
                "x" if shift => Some(Msg::Formatar(Formato::Tachado)),
                "e" => Some(Msg::Formatar(Formato::Codigo)),
                "k" => Some(Msg::Formatar(Formato::Link)),
                // Ctrl+Shift+7/8/9: no teclado ABNT o Shift muda a tecla para & * (, por isso os dois.
                "8" | "*" if shift => Some(Msg::Formatar(Formato::Lista)),
                "7" | "&" if shift => Some(Msg::Formatar(Formato::Numerada)),
                "9" | "(" if shift => Some(Msg::Formatar(Formato::Tarefa)),
                d @ ("1" | "2" | "3" | "4" | "5" | "6") => Some(Msg::Formatar(Formato::Titulo(d.parse().expect("dígito")))),
                "s" => Some(Msg::Salvar),
                "w" => Some(Msg::FecharAba(self.abas[self.ativa].id)),
                "f" | "h" => Some(Msg::Localizar),
                "g" | "l" => Some(Msg::IrPara),
                "=" | "+" => Some(Msg::Fonte(1.0)),
                "-" => Some(Msg::Fonte(-1.0)),
                "0" => Some(Msg::Fonte(0.0)),
                _ => None,
            },
            _ => None,
        }
    }

    fn view(&self) -> Element<'_, Msg> {
        let c = self.cores();
        let editor = text_editor(self.conteudo())
            .size(self.prefs.tamanho)
            .font(match self.prefs.fonte {
                Fonte::Mono => Font::MONO,
                Fonte::Sans => Font::DEFAULT,
                Fonte::Serif => Font::SERIF,
            })
            .wrap(self.prefs.quebra)
            .line_numbers(true)
            .padding(Padding { top: 14.0, right: 16.0, bottom: 8.0, left: 22.0 })
            .on_change(Msg::Mudou)
            .autofocus(self.foco)
            .invisibles(self.prefs.invisiveis)
            .markdown(self.abas[self.ativa].markdown)
            .style(move |_, _| Style::default().background(c.papel).text_color(c.texto));
        let topo = container(self.barra(c)).width(Length::FILL).style(move |_| Style::default().background(c.barra).backdrop_blur(20.0));
        let linha = || container(Space::new(Length::FILL, 1.0)).style(caixa(c.borda, Border::default()));
        let meio = row![column![].push_maybe(self.formatos(c)).push(editor).width(Length::FILL).height(Length::FILL)].push_maybe(if self.painel_postits { Some(self.lista_postits(c)) } else { self.propriedades.then(|| self.propriedades(c)) }).height(Length::FILL);
        let corpo = column![topo, meio, linha(), self.rodape(c)];
        let mut camadas = stack![corpo, self.alcas()];
        if let Some(b) = self.ir_para(c).or_else(|| self.busca(c)) {
            camadas = camadas.push(b);
        }
        if self.menu {
            camadas = camadas.push(self.menu(c));
        }
        if let Some(reabrir) = self.menu_cod {
            camadas = camadas.push(self.menu_codificacao(c, reabrir));
        }
        if let Some(id) = self.confirmar {
            camadas = camadas.push(self.confirmacao(c, id));
        }
        if !self.prefs.configurado {
            camadas = camadas.push(self.boas_vindas(c));
        }
        container(camadas).width(Length::FILL).height(Length::FILL).clip(true).style(move |_| Style::default().background(c.papel).border(Border::rounded(RAIO_JANELA).color(c.borda, 1.0)).text_color(c.texto)).into()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn prefixos_trocam_e_desligam() {
        let c = Content::with_text("item");
        capture(&text_editor::<Msg>(&c).into(), Size::new(300.0, 100.0), 1.0, &Theme::light(), &mut Renderer::new(), None);
        formatar(&c, Formato::Lista);
        assert_eq!(c.text(), "- item");
        formatar(&c, Formato::Tarefa);
        assert_eq!(c.text(), "- [ ] item");
        formatar(&c, Formato::Titulo(2));
        assert_eq!(c.text(), "## item");
        formatar(&c, Formato::Titulo(2));
        assert_eq!(c.text(), "item");
        formatar(&c, Formato::Numerada);
        assert_eq!(c.text(), "1. item");
        // Com recuo, o riscado entra depois do "##" mesmo pegando a linha toda.
        let c = Content::with_text(" ## oi");
        capture(&text_editor::<Msg>(&c).into(), Size::new(300.0, 100.0), 1.0, &Theme::light(), &mut Renderer::new(), None);
        c.find_next(" ## oi", true);
        formatar(&c, Formato::Tachado);
        assert_eq!(c.text(), " ## ~~oi~~");
    }

    /// Excluir manda para a lixeira (o texto fica); só "Apagar" da lixeira tira de vez; vazio some ao fechar.
    #[test]
    fn postit_excluido_vai_para_a_lixeira() {
        let mut n = Notas::novo();
        let _ = n.update(Msg::NovoPostIt);
        let id = n.postits.last().unwrap().id;
        let p = n.postits.last_mut().unwrap();
        // Como se tivesse digitado: a versão gravada fica para trás.
        p.conteudo = Content::with_text("lembrar");
        p.gravado = u64::MAX;
        let _ = n.update(Msg::PostItFechar(id));
        assert!(n.postits.iter().any(|p| p.id == id && p.confirmar), "com texto pergunta antes");
        let _ = n.update(Msg::PostItResposta(id, Some(true)));
        let p = n.postits.iter().find(|p| p.id == id).expect("continua na lista");
        assert!(p.excluido && !p.aberto);
        assert_eq!(std::fs::read_to_string(arquivo::texto_postit(id)).unwrap(), "lembrar");
        // Apagar só vale na lixeira: restaurado, o pedido não apaga.
        let _ = n.update(Msg::RestaurarPostIt(id));
        let _ = n.update(Msg::ApagarPostIt(id));
        assert!(n.postits.iter().any(|p| p.id == id && !p.excluido));
        let _ = n.update(Msg::PostItResposta(id, Some(true)));
        let _ = n.update(Msg::ApagarPostIt(id));
        assert!(!n.postits.iter().any(|p| p.id == id));
        assert!(!arquivo::texto_postit(id).exists());
        // Vazio fecha sem perguntar e não fica na lista.
        let _ = n.update(Msg::NovoPostIt);
        let vazio = n.postits.last().unwrap().id;
        let _ = n.update(Msg::PostItFechar(vazio));
        assert!(!n.postits.iter().any(|p| p.id == vazio));
    }

    /// A tela desenha sem GPU nos dois temas, com o menu, a busca e a confirmação abertos.
    #[test]
    fn tela_desenha() {
        for escuro in [false, true] {
            let mut n = Notas::novo();
            n.escuro = escuro;
            n.menu = true;
            n.busca = Some(Busca { termo: "a".into(), troca: String::new(), maiusculas: true });
            n.prefs.invisiveis = true;
            // Texto fixo: o teste não depende da sessão de quem roda.
            n.abas[n.ativa].markdown = true;
            n.abas[n.ativa].conteudo = Some(Content::with_text("Olá mundo\tcom tab\nZWSP aqui:\u{200b}x e NBSP:\u{a0}y\n# Título\n## Subtítulo\n**negrito**, *itálico*, ~~tachado~~ e `código`\n- [ ] tarefa\n- [x] feita\n1. item\n> citação\n[link](https://x.y)\n| a | b |\n|---|---|\n"));
            n.confirmar = Some(n.abas[0].id);
            n.propriedades = true;
            let png = capture(&n.view(), Size::new(980.0, 680.0), 1.0, &n.theme(), &mut Renderer::new(), None);
            assert!(png.len() > 10_000);
            // A configuração inicial, por cima de tudo na primeira vez.
            if let Some(d) = std::env::var_os("FEATHERNOTES_CAPTURAR") {
                let (antes, conf) = (n.prefs.configurado, n.confirmar.take());
                n.prefs.configurado = false;
                std::fs::write(PathBuf::from(&d).join(format!("inicio-{escuro}.png")), capture(&n.view(), Size::new(980.0, 680.0), 1.0, &n.theme(), &mut Renderer::new(), None)).unwrap();
                (n.prefs.configurado, n.confirmar) = (antes, conf);
            }
            // O post-it desenha na janela dele.
            n.postits.push(PostIt { id: 77, conteudo: Content::with_text("Comprar pão\n- [ ] leite\n- [x] **café**"), cor: 0, aberto: true, fixo: true, posicao: None, excluido: false, confirmar: true, gravado: 0 });
            let nota = capture(&n.view_window(WindowId(77)), Size::new(280.0, 280.0), 1.0, &n.theme(), &mut Renderer::new(), None);
            assert!(nota.len() > 2_000);
            if let Some(d) = std::env::var_os("FEATHERNOTES_CAPTURAR") {
                std::fs::write(PathBuf::from(&d).join(format!("postit-{escuro}.png")), &nota).unwrap();
                // A lista, com um na lixeira.
                n.postits.push(PostIt { id: 78, conteudo: Content::with_text("## Reunião às 15h"), cor: 2, aberto: false, fixo: false, posicao: None, excluido: true, confirmar: false, gravado: 0 });
                let (menu, conf, prop) = (std::mem::take(&mut n.menu), n.confirmar.take(), std::mem::take(&mut n.propriedades));
                n.painel_postits = true;
                std::fs::write(PathBuf::from(&d).join(format!("lista-{escuro}.png")), capture(&n.view(), Size::new(980.0, 680.0), 1.0, &n.theme(), &mut Renderer::new(), None)).unwrap();
                (n.menu, n.confirmar, n.propriedades, n.painel_postits) = (menu, conf, prop, false);
                n.postits.pop();
            }
            if let Some(d) = std::env::var_os("FEATHERNOTES_CAPTURAR") {
                std::fs::write(PathBuf::from(&d).join(format!("tela-{escuro}.png")), &png).unwrap();
                n.confirmar = None;
                n.propriedades = false;
                std::fs::write(PathBuf::from(&d).join(format!("menu-{escuro}.png")), capture(&n.view(), Size::new(980.0, 680.0), 1.0, &n.theme(), &mut Renderer::new(), None)).unwrap();
                n.menu = false;
                n.busca = None;
                n.ir_para = Some("12".into());
                n.menu_cod = Some(false);
                n.propriedades = true;
                std::fs::write(PathBuf::from(&d).join(format!("limpa-{escuro}.png")), capture(&n.view(), Size::new(980.0, 680.0), 1.0, &n.theme(), &mut Renderer::new(), None)).unwrap();
            }
        }
    }
}
