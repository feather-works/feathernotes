# Auditoria do FeatherNotes 0.1.0 (2026-10-20)

**O que o repositório faz:** bloco de notas leve em Rust sobre o pelicano: abas que voltam com o
que não foi salvo, mais de 30 codificações, Markdown e post-its soltos na área de trabalho. Uma
cópia só por vez: a segunda entrega o pedido à primeira por uma porta local e sai.
**Carga assumida:** uma pessoa num desktop, uma cópia do app, arquivos de texto pequenos ou médios.

Os 8 testes passam (`cargo test`).

Estado: ✅ corrigido · ⏳ pendente

## Precisa corrigir

1. ✅ **Arquivo que não abre vira arquivo vazio** (`src/main.rs`, `carregar`)
   - Se a leitura falha (pendrive fora, sem permissão), a aba abre vazia e marcada como "alterada".
     A cópia de recuperação guarda o vazio; nas próximas vezes a aba abre o vazio em vez do
     arquivo, e "Salvar" grava o vazio por cima do arquivo de verdade.
   - Conserto: no erro, marcar a aba como não alterada (`Some(0)`), sem cópia de recuperação.

2. ✅ **O .deb não instala na maioria das distros** (`pacote/gerar.sh`)
   - Pede `libc6 (>= 2.43)`, a glibc da máquina que compila. Ubuntu 24.04 tem 2.39, Debian 12 tem
     2.36, Mint 22 tem 2.39. O binário precisa mesmo da 2.43 (`acosf`, `atan2f`), então só trocar
     o número não basta.
   - Conserto: compilar num sistema mais antigo (contêiner `ubuntu:22.04`, glibc 2.35) ou com
     `cargo zigbuild --target x86_64-unknown-linux-gnu.2.31`, e pôr essa versão no `Depends`.
   - Feito: `pacote/gerar.sh` compila num contêiner Debian 12 (glibc 2.36) e o `Depends` sai do binário.

## Deveria corrigir

3. ✅ **A codificação escolhida some quando o app reabre** (`src/arquivo.rs`, `src/main.rs`)
   - Quem escolhe "Converter para Windows-1252" e fecha sem salvar volta com a codificação
     detectada: ao salvar, o arquivo sai na codificação errada sem aviso.
   - Conserto: a sessão guarda a codificação de cada aba (uma linha `cod`, como a `md`).
   - Ainda falta: a quebra de linha escolhida (LF/CRLF) também some do mesmo jeito.

4. ✅ **O painel de propriedades mostra números velhos** (`src/main.rs`)
   - As contagens ficam guardadas pela chave (aba, versão do texto), e todo texto novo começa
     na versão 0. Com a aba vazia e o painel aberto, Ctrl+O abre um arquivo no lugar dela e o
     painel continua dizendo "0 linhas". O mesmo vale para "Reabrir com".
   - Conserto: esquecer as contagens quando o texto da aba é trocado.

5. ✅ **Uma porta velha faz o app fechar sem abrir** (`src/instancia.rs`)
   - Depois de uma queda, o arquivo `porta` fica com a porta antiga. Se outro programa escuta
     nela, a conexão dá certo, o pedido "foi entregue" e o FeatherNotes sai sem mostrar nada.
   - Conserto: o app aberto responde `ok`; quem chega só sai se receber a resposta, ou se o outro
     fechar sem responder (é o que a 0.1.0 faz; sem isso, atualizar com o app antigo aberto em
     segundo plano abria duas cópias, e elas gravavam a sessão uma por cima da outra).

6. ✅ **Gravar não garante o arquivo depois de uma queda de energia** (`src/arquivo.rs`, `gravar`)
   - Grava num temporário e troca, mas sem mandar os dados para o disco antes. No XFS (e no ext4
     com algumas opções) a queda pode deixar o arquivo vazio.
   - Conserto: `sync_all` antes do `rename`.

7. ✅ **Uma entrada velha fica no menu depois de desinstalar o .deb** (`src/main.rs`)
   - O app escreve um `.desktop` em `~/.local/share/applications` toda vez que abre. Com o .deb,
     essa cópia esconde a do sistema e fica para trás depois do `apt remove`, apontando para um
     executável que não existe mais.
   - Conserto: não registrar quando o executável está em `/usr/`.

8. ✅ **Nenhum teste cobre a sessão e a recuperação, que apagam arquivos** (`src/arquivo.rs`)
   - `ler_sessao` apaga toda cópia de recuperação que nenhuma aba da sessão usa. Se gravar e ler
     a sessão discordarem, o texto não salvo some na próxima vez que o app abre.
   - Conserto: um teste de ida e volta da sessão, que confere que a cópia continua lá.

## Bom ter

9. ✅ **Caracteres que não cabem contados em dobro** (`src/codificacao.rs`)
   - "ã ç ã" em ASCII avisa "3 caractere(s): ã ç ã".
   - Conserto: guardar cada caractere uma vez só.

10. ✅ **A troca de cor da pena está escrita duas vezes** (`src/main.rs`)
    - A mesma troca de cores aparece em dois lugares; mudar só um deixa os dois ícones diferentes.
    - Conserto: uma função só.

11. ⏳ **A cópia de recuperação grava o texto inteiro na thread da tela a cada 2 s** (`src/main.rs`)
    - Para notas normais, tudo bem. Com um log de 50 MB aberto, a digitação engasga.
    - Conserto, só se alguém reclamar: gravar numa thread.

## O travamento ao redimensionar

✅ Resolvido. Medido numa RX 7600, arrastando a borda da janela:

- **A GPU ia a 1300–1900 MHz por causa do Xwayland**, não do FeatherNotes. O pelicano desenha na
  CPU; com `x11: true` cada tamanho novo passa pelo Xwayland, que usava 20–66% da GPU. O
  FeatherNotes em si ficava perto de 0%. Conserto: saiu o `x11: true`; o app roda direto no Wayland.
  Em troca, no Ubuntu os post-its abrem onde o GNOME quiser.
- **A VRAM subia (até 128 MB) e só caía depois**: o pelicano criava uma textura a cada tamanho e
  não pedia para soltar as velhas. Conserto no pelicano 0.1.5: textura em degraus de 256 px e
  `device.poll` depois de mostrar o quadro.
- Correção de uma sugestão anterior: o blur (`backdrop_blur`) do pelicano roda na CPU e não tem
  nada a ver com a memória de vídeo.
- ⏳ Sem o X11, o alfinete do post-it (`WindowLevel::AlwaysOnBottom`) não faz nada no Wayland: o
  botão continua à mostra. Esconder no Linux ou tirar a promessa do README.

## Não verificado

- O resto do código do pelicano (só o desenho e o redimensionamento foram lidos).
- O build e o instalador do Windows (NSIS).
- As partes de `main.rs` que montam a tela e a formatação de Markdown (`formatar`, `prefixo`), só
  passei o olho.
- O travamento não foi reproduzido.

# Auditoria do FeatherNotes 0.1.14 (2026-11-02)

Todos corrigidos; 27 testes passam (`cargo test`).

1. ✅ **Atualizar no Windows com o app aberto deixava a versão velha rodando** (`src/instancia.rs`):
   só o Linux percebia o executável trocado. Agora compara a data do arquivo no caminho de quando abriu.
2. ✅ **Tailandês e russo do DOS abriam embaralhados** (`src/codificacao.rs`): o palpite do chardetng
   virava Windows-1252. Fica o palpite; IBM866 e Windows-874 entraram em "Outras codificações".
3. ✅ **Salvar tirava a permissão de execução de um script** (`src/arquivo.rs`, `gravar`).
4. ✅ **Sem teste de salvar e carregar arquivos**: `salvar_e_carregar_arquivos`.
5. ✅ **O `instalar.sh` prometia não mudar o app padrão**, mas o app vira o padrão na primeira vez
   (de propósito, commit `89cf73b`): o comentário foi corrigido.
6. ✅ **Quatro caches escritos à mão e limpos em dois lugares** (`src/main.rs`): `lembrar` e `esquecer_contagens`.
7. ✅ **Disco consultado a cada quadro** para as abas fechadas, e a lista de suspeitas copiada duas vezes.
8. ✅ **O modo JSON não voltava com a sessão**: linha `json`, como a `md`.
9. ✅ **Preferências gravadas sem o temporário**: agora pelo `gravar`.
10. ✅ **O mesmo estilo de cartão em seis lugares**: `flutuante`.
11. ✅ **Caminho que não é UTF-8 se perdia no pedido ao app aberto**: vai em hexadecimal (`abrirx`);
    o `abrir` em texto das versões antigas continua aceito.
12. ✅ **Ctrl+Tab não guardava a aba ativa na sessão.**

Não verificado: a atualização com o app aberto num Windows de verdade (o build do Windows só foi checado com `cargo check`).
