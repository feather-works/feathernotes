# FeatherNotes

Bloco de notas leve da suíte FeatherWorks, feito em Rust com o [pelicano](https://github.com/feather-works/pelicano).

- Abas que voltam como estavam, com o que não foi salvo
- Localizar e substituir, ir para a linha, zoom com Ctrl+roda
- Markdown com os sinais escondidos, barra de formatação e tarefas clicáveis
- Mais de 30 codificações, detecção automática e conserto de acentos quebrados
- Caracteres invisíveis à mostra
- Post-its soltos na área de trabalho: cores, alfinete no fundo da tela, lista com lixeira
- Atalho do sistema para novo post-it (Super+Alt+N, no GNOME) e abrir junto com o sistema

## Instalar

Baixe da [última versão](https://github.com/feather-works/feathernotes/releases/latest) ou instale pelo FeatherWorks Launcher:

- Linux: `feathernotes-linux-amd64.deb`, ou o `.tar.gz` e rode `instalar.sh`
- Windows: `FeatherNotes-windows-x64.exe`

## Compilar

```bash
cargo run --release
cargo test
pacote/gerar.sh   # os instaladores, em pacote/saida/
```

Licença GPL-3.0.
