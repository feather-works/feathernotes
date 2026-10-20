//! Ler e gravar os .txt, as preferências e a sessão (abas abertas + cópias de recuperação).
use std::io;
use std::path::{Path, PathBuf};

/// Grava num temporário ao lado e troca: queda de energia no meio não corta o arquivo.
// shortcut: o rename troca um link simbólico pelo arquivo e não copia as permissões; gravar por cima quando alguém precisar.
pub fn gravar(p: &Path, bytes: &[u8]) -> io::Result<()> {
    let nome = p.file_name().ok_or_else(|| io::Error::other("caminho sem nome de arquivo"))?;
    let tmp = p.with_file_name(format!(".{}.feathernotes", nome.to_string_lossy()));
    let mut f = std::fs::File::create(&tmp)?;
    io::Write::write_all(&mut f, bytes)?;
    // Para o disco antes de trocar: sem isso, no XFS a queda pode deixar o arquivo vazio.
    f.sync_all()?;
    std::fs::rename(&tmp, p)
}

fn pasta_dados() -> PathBuf {
    // Os testes nunca mexem na sessão e nos post-its de quem roda.
    if cfg!(test) {
        return std::env::temp_dir().join(format!("feathernotes-teste-{}", std::process::id()));
    }
    dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("feathernotes")
}

/// A porta onde o FeatherNotes aberto escuta (veja `instancia`).
pub fn arquivo_porta() -> PathBuf {
    pasta_dados().join("porta")
}

pub fn recuperacao(id: u64) -> PathBuf {
    pasta_dados().join("recuperar").join(format!("{id}.txt"))
}

/// Uma aba como fica na sessão: só a referência, o texto é lido quando a aba abre.
pub struct AbaSalva {
    pub id: u64,
    pub caminho: Option<PathBuf>,
    pub markdown: bool,
    /// A de salvar; `None` nas sessões antigas (aí detecta pelo arquivo).
    pub cod: Option<crate::codificacao::Cod>,
}

// shortcut: uma aba por linha separada por tab; um caminho com tab ou quebra de linha no nome se perde na sessão.
pub fn gravar_sessao(ativa: usize, abas: &[AbaSalva]) -> io::Result<()> {
    let mut s = format!("ativa\t{ativa}\n");
    for a in abas {
        s.push_str(&format!("aba\t{}\t{}\n", a.id, a.caminho.as_ref().map_or(String::new(), |p| p.to_string_lossy().into_owned())));
        if a.markdown {
            s.push_str(&format!("md\t{}\n", a.id));
        }
        if let Some(c) = a.cod {
            s.push_str(&format!("cod\t{}\t{}\n", a.id, c.nome()));
        }
    }
    let pasta = pasta_dados();
    std::fs::create_dir_all(pasta.join("recuperar"))?;
    gravar(&pasta.join("sessao"), s.as_bytes())
}

pub fn ler_sessao() -> (usize, Vec<AbaSalva>) {
    let Ok(s) = std::fs::read_to_string(pasta_dados().join("sessao")) else { return (0, Vec::new()) };
    let mut ativa = 0;
    let mut abas = Vec::new();
    for l in s.lines() {
        let campos: Vec<&str> = l.splitn(3, '\t').collect();
        match campos.as_slice() {
            ["ativa", n] => ativa = n.parse().unwrap_or(0),
            ["aba", id, caminho] => {
                if let Ok(id) = id.parse() {
                    abas.push(AbaSalva { id, caminho: (!caminho.is_empty()).then(|| PathBuf::from(caminho)), markdown: false, cod: None });
                }
            }
            // Numa linha à parte: sessões antigas, sem ela, continuam lendo.
            ["md", id] => {
                if let Some(a) = abas.iter_mut().find(|a| id.parse() == Ok(a.id)) {
                    a.markdown = true;
                }
            }
            ["cod", id, nome] => {
                if let Some(a) = abas.iter_mut().find(|a| id.parse() == Ok(a.id)) {
                    a.cod = crate::codificacao::pelo_nome(nome);
                }
            }
            _ => {}
        }
    }
    // Cópias que nenhuma aba usa (de uma sessão que caiu no meio da gravação) vão embora.
    if let Ok(dir) = std::fs::read_dir(pasta_dados().join("recuperar")) {
        for e in dir.flatten() {
            let id = e.path().file_stem().and_then(|s| s.to_str()?.parse::<u64>().ok());
            if !id.is_some_and(|id| abas.iter().any(|a| a.id == id)) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    (ativa, abas)
}

/// Um post-it como fica no disco: o texto em `postits/<id>.txt`, cor e se estava aberto na `lista`.
pub struct PostItSalvo {
    pub id: u64,
    pub cor: usize,
    pub aberto: bool,
    /// Com o alfinete: preso no fundo da tela.
    pub fixo: bool,
    pub posicao: Option<pelicano::Point>,
    /// Na lixeira da lista de post-its: some da tela mas dá para restaurar.
    pub excluido: bool,
}

fn pasta_postits() -> PathBuf {
    pasta_dados().join("postits")
}

pub fn texto_postit(id: u64) -> PathBuf {
    pasta_postits().join(format!("{id}.txt"))
}

pub fn ler_postits() -> Vec<(PostItSalvo, String)> {
    let lista = std::fs::read_to_string(pasta_postits().join("lista")).unwrap_or_default();
    lista
        .lines()
        .filter_map(|l| {
            let mut c = l.split('\t');
            let id = c.next()?.parse().ok()?;
            let cor = c.next()?.parse().unwrap_or(0);
            let aberto = c.next() == Some("1");
            let fixo = c.next() == Some("1");
            let excluido = c.next() == Some("1");
            let posicao = c.next().zip(c.next()).and_then(|(x, y)| Some(pelicano::Point::new(x.parse().ok()?, y.parse().ok()?)));
            Some((PostItSalvo { id, cor, aberto, fixo, posicao, excluido }, std::fs::read_to_string(texto_postit(id)).unwrap_or_default()))
        })
        .collect()
}

/// A lista; o texto de cada um é gravado à parte, só quando muda.
pub fn gravar_postits(lista: &[PostItSalvo]) -> io::Result<()> {
    let s: String = lista.iter().map(|p| format!("{}\t{}\t{}\t{}\t{}{}\n", p.id, p.cor, u8::from(p.aberto), u8::from(p.fixo), u8::from(p.excluido), p.posicao.map_or(String::new(), |q| format!("\t{}\t{}", q.x, q.y)))).collect();
    std::fs::create_dir_all(pasta_postits())?;
    gravar(&pasta_postits().join("lista"), s.as_bytes())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tema {
    Claro,
    Escuro,
    Automatico,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fonte {
    Mono,
    Sans,
    Serif,
}

pub struct Preferencias {
    pub tema: Tema,
    pub fonte: Fonte,
    pub tamanho: f32,
    pub quebra: bool,
    pub invisiveis: bool,
    /// Já respondeu a configuração inicial (a pergunta da primeira vez).
    pub configurado: bool,
}

fn arquivo_preferencias() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("feathernotes").join("preferencias")
}

pub fn ler_preferencias() -> Preferencias {
    let mut p = Preferencias { tema: Tema::Automatico, fonte: Fonte::Mono, tamanho: 15.0, quebra: true, invisiveis: false, configurado: false };
    for l in std::fs::read_to_string(arquivo_preferencias()).unwrap_or_default().lines() {
        match l.split_once('=') {
            Some(("tema", "claro")) => p.tema = Tema::Claro,
            Some(("tema", "escuro")) => p.tema = Tema::Escuro,
            Some(("tamanho", v)) => p.tamanho = v.parse().unwrap_or(15.0_f32).clamp(8.0, 48.0),
            Some(("quebra", v)) => p.quebra = v != "0",
            Some(("invisiveis", v)) => p.invisiveis = v == "1",
            Some(("configurado", v)) => p.configurado = v == "1",
            Some(("fonte", "sans")) => p.fonte = Fonte::Sans,
            Some(("fonte", "serif")) => p.fonte = Fonte::Serif,
            _ => {}
        }
    }
    p
}

pub fn gravar_preferencias(p: &Preferencias) {
    let tema = match p.tema {
        Tema::Claro => "claro",
        Tema::Escuro => "escuro",
        Tema::Automatico => "auto",
    };
    let fonte = match p.fonte {
        Fonte::Mono => "mono",
        Fonte::Sans => "sans",
        Fonte::Serif => "serif",
    };
    let caminho = arquivo_preferencias();
    let _ = std::fs::create_dir_all(caminho.parent().expect("tem pasta"));
    let _ = std::fs::write(caminho, format!("tema={tema}\nfonte={fonte}\ntamanho={}\nquebra={}\ninvisiveis={}\nconfigurado={}\n", p.tamanho, u8::from(p.quebra), u8::from(p.invisiveis), u8::from(p.configurado)));
}

/// O GNOME guarda o modo escuro em `color-scheme`.
// shortcut: fora do GNOME o automático fica claro; ler o portal (org.freedesktop.appearance) e o registro do Windows quando alguém pedir.
pub fn sistema_escuro() -> bool {
    cfg!(target_os = "linux")
        && std::process::Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "color-scheme"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("dark"))
}

/// Abrir ao entrar no sistema: no Linux um .desktop em `~/.config/autostart`, no Windows a chave `Run`.
// shortcut: no macOS não faz nada; um LaunchAgent em ~/Library/LaunchAgents quando houver build de Mac.
pub fn iniciar_com_sistema(ligar: bool) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CHAVE: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let mut c = std::process::Command::new("reg");
        if ligar {
            c.args(["add", CHAVE, "/v", "FeatherNotes", "/t", "REG_SZ", "/d", &format!("\"{}\" --fundo", exe.display()), "/f"]);
        } else {
            c.args(["delete", CHAVE, "/v", "FeatherNotes", "/f"]);
        }
        // Sem a janelinha do console piscando.
        let ok = c.creation_flags(0x0800_0000).status()?.success();
        return if ok || !ligar { Ok(()) } else { Err(io::Error::other("o reg add falhou")) };
    }
    #[cfg(not(windows))]
    {
        let arquivo = arquivo_autostart();
        if !ligar {
            return std::fs::remove_file(arquivo).or_else(|e| if e.kind() == io::ErrorKind::NotFound { Ok(()) } else { Err(e) });
        }
        std::fs::create_dir_all(arquivo.parent().expect("tem pasta"))?;
        // Exec entre aspas: o caminho pode ter espaço.
        let desktop = format!("[Desktop Entry]\nType=Application\nName=FeatherNotes\nExec=\"{}\" --fundo\nIcon=feathernotes\nX-GNOME-Autostart-enabled=true\n", exe.display());
        gravar(&arquivo, desktop.as_bytes())
    }
}

pub fn inicia_com_sistema() -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        return std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "FeatherNotes"])
            .creation_flags(0x0800_0000)
            .output()
            .is_ok_and(|o| o.status.success());
    }
    #[cfg(not(windows))]
    arquivo_autostart().exists()
}

#[cfg(not(windows))]
fn arquivo_autostart() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("autostart").join("feathernotes.desktop")
}

/// A tecla do atalho do sistema que cria um post-it.
pub const ATALHO_POSTIT: &str = "Super+Alt+N";
const CHAVES: &str = "org.gnome.settings-daemon.plugins.media-keys";
const CAMINHO_ATALHO: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/feathernotes-postit/";

fn gsettings(args: &[&str]) -> io::Result<String> {
    let o = std::process::Command::new("gsettings").args(args).output()?;
    if !o.status.success() {
        return Err(io::Error::other(String::from_utf8_lossy(&o.stderr).trim().to_owned()));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_owned())
}

/// O gsettings lê o valor como GVariant: um texto que começa com aspas viraria outra coisa.
fn texto_gvariant(t: &str) -> String {
    format!("'{}'", t.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// A lista de atalhos personalizados do GNOME com o nosso posto ou tirado (os do usuário ficam).
fn trocar_na_lista(lista: &str, ligar: bool) -> String {
    let mut itens: Vec<String> = lista.trim_start_matches("@as").trim().trim_matches(['[', ']']).split(',').map(|i| i.trim().trim_matches('\'').to_owned()).filter(|i| !i.is_empty() && i != CAMINHO_ATALHO).collect();
    if ligar {
        itens.push(CAMINHO_ATALHO.to_owned());
    }
    format!("[{}]", itens.iter().map(|i| format!("'{i}'")).collect::<Vec<_>>().join(", "))
}

/// Liga ou desliga o atalho do sistema (Super+Alt+N) que roda `feathernotes --postit`.
// shortcut: só no GNOME (gsettings); KDE e Windows precisam do jeito deles (kglobalaccel, RegisterHotKey) quando alguém pedir.
pub fn atalho_postit(ligar: bool) -> io::Result<()> {
    let lista = gsettings(&["get", CHAVES, "custom-keybindings"])?;
    if ligar {
        let exe = std::env::current_exe()?;
        let esquema = format!("{CHAVES}.custom-keybinding:{CAMINHO_ATALHO}");
        gsettings(&["set", &esquema, "name", &texto_gvariant("FeatherNotes: novo post-it")])?;
        gsettings(&["set", &esquema, "command", &texto_gvariant(&format!("\"{}\" --postit", exe.display()))])?;
        gsettings(&["set", &esquema, "binding", &texto_gvariant("<Super><Alt>n")])?;
    }
    gsettings(&["set", CHAVES, "custom-keybindings", &trocar_na_lista(&lista, ligar)]).map(drop)
}

pub fn tem_atalho_postit() -> bool {
    gsettings(&["get", CHAVES, "custom-keybindings"]).is_ok_and(|l| l.contains(CAMINHO_ATALHO))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn atalho_entra_e_sai_sem_mexer_nos_outros() {
        let c0 = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/custom0/";
        assert_eq!(trocar_na_lista("@as []", true), format!("['{CAMINHO_ATALHO}']"));
        let dois = trocar_na_lista(&format!("['{c0}']"), true);
        assert_eq!(dois, format!("['{c0}', '{CAMINHO_ATALHO}']"));
        // Ligar de novo não duplica; desligar tira só o nosso.
        assert_eq!(trocar_na_lista(&dois, true), dois);
        assert_eq!(trocar_na_lista(&dois, false), format!("['{c0}']"));
        assert_eq!(texto_gvariant("\"/a b/it's\" --postit"), "'\"/a b/it\\'s\" --postit'");
    }

    /// Ida e volta da sessão: o que foi gravado volta, e a cópia de uma aba da sessão não é apagada.
    #[test]
    fn sessao_volta_e_guarda_a_copia() {
        use crate::codificacao::Cod;
        let abas = [
            AbaSalva { id: 9001, caminho: Some("/tmp/a b.md".into()), markdown: true, cod: Some(Cod::Legado("windows-1252")) },
            AbaSalva { id: 9002, caminho: None, markdown: false, cod: None },
        ];
        gravar_sessao(1, &abas).unwrap();
        gravar(&recuperacao(9001), b"nao salvo").unwrap();
        let orfa = recuperacao(9999);
        gravar(&orfa, b"de ninguem").unwrap();
        let (ativa, lidas) = ler_sessao();
        assert_eq!(ativa, 1);
        let a = lidas.iter().find(|a| a.id == 9001).unwrap();
        assert_eq!((a.caminho.as_deref(), a.markdown, a.cod), (Some(Path::new("/tmp/a b.md")), true, Some(Cod::Legado("windows-1252"))));
        let b = lidas.iter().find(|a| a.id == 9002).unwrap();
        assert_eq!((b.caminho.as_deref(), b.markdown, b.cod), (None, false, None));
        assert!(recuperacao(9001).exists(), "a cópia de uma aba da sessão fica");
        assert!(!orfa.exists(), "a de ninguém vai embora");
    }
}

/// Na primeira vez, o FeatherNotes vira o app padrão de .txt e .md; depois a escolha do usuário manda.
#[cfg(target_os = "linux")]
pub fn tornar_padrao() {
    let marca = pasta_dados().join("padrao");
    if marca.exists() {
        return;
    }
    let feito = std::process::Command::new("xdg-mime").args(["default", "feathernotes.desktop", "text/plain", "text/markdown"]).status().is_ok_and(|s| s.success());
    if feito {
        let _ = std::fs::create_dir_all(pasta_dados());
        let _ = gravar(&marca, b"");
    }
}

/// Põe o FeatherNotes no menu de aplicativos e na dock (com "Novo post-it" no clique direito do
/// ícone). Só reescreve quando muda: o executável pode ter ido para outra pasta.
// shortcut: só Linux (freedesktop); no Windows o atalho do Menu Iniciar vem do instalador.
#[cfg(target_os = "linux")]
pub fn registrar_no_sistema(icone_svg: &str) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    let dados = dirs::data_dir().ok_or_else(|| io::Error::other("sem pasta de dados"))?;
    let icone = dados.join("icons/hicolor/scalable/apps/feathernotes.svg");
    let desktop = dados.join("applications/feathernotes.desktop");
    let exe = exe.display();
    // StartupWMClass casa com o `app_id` das janelas: é assim que a dock junta as janelas no ícone.
    let entrada = format!(
        "[Desktop Entry]\nType=Application\nName=FeatherNotes\nComment=Bloco de notas e post-its\nExec=\"{exe}\" %F\nMimeType=text/plain;text/markdown;\nIcon=feathernotes\nTerminal=false\nCategories=Utility;TextEditor;\nStartupWMClass=feathernotes\nActions=postit;\n\n[Desktop Action postit]\nName=Novo post-it\nExec=\"{exe}\" --postit\n"
    );
    for (caminho, conteudo) in [(&icone, icone_svg), (&desktop, entrada.as_str())] {
        if std::fs::read_to_string(caminho).ok().as_deref() != Some(conteudo) {
            std::fs::create_dir_all(caminho.parent().expect("tem pasta"))?;
            gravar(caminho, conteudo.as_bytes())?;
        }
    }
    Ok(())
}
