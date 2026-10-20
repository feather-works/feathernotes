#!/usr/bin/env bash
# Gera os instaladores do FeatherNotes em pacote/saida/, com os nomes fixos que o
# launcher procura na release (releases/latest/download/<nome>):
#   feathernotes-linux-x86_64.tar.gz   Linux portátil (instalar.sh na pasta do usuário)
#   feathernotes-linux-amd64.deb       Debian, Ubuntu, Mint…
#   FeatherNotes-windows-x64.exe       Windows 10/11 (instalador por usuário, sem administrador)
#   SHA256SUMS
#
# Uso: pacote/gerar.sh [linux|windows ...]   (sem argumentos = todos)
# Precisa de: cargo, dpkg-deb; para Windows, o alvo x86_64-pc-windows-gnu,
# mingw-w64 e makensis.
set -euo pipefail

pacote="$(cd "$(dirname "$0")" && pwd)"
raiz="$(dirname "$pacote")"
alvo="${CARGO_TARGET_DIR:-$raiz/target}"
versao="$(sed -n 's/^version = "\(.*\)"/\1/p' "$raiz/Cargo.toml" | head -1)"
saida="$pacote/saida"
alvos=("$@")
[[ ${#alvos[@]} -eq 0 ]] && alvos=(linux windows)
rm -rf "$saida" && mkdir -p "$saida"
echo "FeatherNotes $versao → $saida"

linux() {
  cargo build --release --manifest-path "$raiz/Cargo.toml"
  local bin="$alvo/release/feathernotes"

  # .deb
  local d; d="$(mktemp -d)"
  install -Dm755 "$bin" "$d/usr/bin/feathernotes"
  install -Dm644 "$pacote/feathernotes.desktop" "$d/usr/share/applications/feathernotes.desktop"
  for t in 16 24 32 48 64 128 256 512; do
    install -Dm644 "$pacote/icones/feathernotes-$t.png" "$d/usr/share/icons/hicolor/${t}x${t}/apps/feathernotes.png"
  done
  install -Dm644 "$pacote/icones/feathernotes.svg" "$d/usr/share/icons/hicolor/scalable/apps/feathernotes.svg"
  install -Dm644 "$raiz/LICENSE" "$d/usr/share/doc/feathernotes/copyright"
  local tamanho; tamanho="$(du -sk "$d/usr" | cut -f1)"
  mkdir -p "$d/DEBIAN"
  cat > "$d/DEBIAN/control" <<CTL
Package: feathernotes
Version: $versao
Section: editors
Priority: optional
Architecture: amd64
Installed-Size: $tamanho
Depends: libc6 (>= 2.43), libxkbcommon0, libwayland-client0 | libx11-6, libfontconfig1
Recommends: libvulkan1 | libgl1
Maintainer: FeatherWorks <https://github.com/feather-works>
Homepage: https://github.com/feather-works/feathernotes
Description: Bloco de notas e post-its da suíte FeatherWorks
 Bloco de notas leve em Rust, com abas, localizar e substituir, Markdown,
 mais de 30 codificações e post-its soltos na área de trabalho.
CTL
  cat > "$d/DEBIAN/postinst" <<'SH'
#!/bin/sh
set -e
command -v update-desktop-database >/dev/null && update-desktop-database -q /usr/share/applications || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
SH
  cp "$d/DEBIAN/postinst" "$d/DEBIAN/postrm"
  chmod 755 "$d/DEBIAN/postinst" "$d/DEBIAN/postrm"
  dpkg-deb --root-owner-group --build "$d" "$saida/feathernotes-linux-amd64.deb" >/dev/null
  rm -rf "$d"

  # .tar.gz portátil: "<app>-<versão>/instalar.sh", o mesmo que o launcher roda.
  local t; t="$(mktemp -d)/feathernotes-$versao"
  mkdir -p "$t"
  cp "$bin" "$t/feathernotes"
  cp "$pacote/feathernotes.desktop" "$raiz/LICENSE" "$t/"
  cp -r "$pacote/icones" "$t/"
  cat > "$t/instalar.sh" <<'SH'
#!/bin/sh
# Instala o FeatherNotes só para este usuário (~/.local), sem administrador.
set -e
cd "$(dirname "$0")"
mkdir -p ~/.local/bin ~/.local/share/applications
install -m755 feathernotes ~/.local/bin/feathernotes
sed "s#^Exec=feathernotes#Exec=$HOME/.local/bin/feathernotes#" feathernotes.desktop > ~/.local/share/applications/feathernotes.desktop
for t in 16 24 32 48 64 128 256 512; do
  mkdir -p ~/.local/share/icons/hicolor/${t}x${t}/apps
  cp icones/feathernotes-$t.png ~/.local/share/icons/hicolor/${t}x${t}/apps/feathernotes.png
done
update-desktop-database ~/.local/share/applications 2>/dev/null || true
xdg-mime default feathernotes.desktop text/plain text/markdown 2>/dev/null || true
echo "FeatherNotes instalado. Procure por \"FeatherNotes\" no menu de aplicativos."
SH
  chmod 755 "$t/instalar.sh"
  tar -C "$(dirname "$t")" -czf "$saida/feathernotes-linux-x86_64.tar.gz" "feathernotes-$versao"
  rm -rf "$(dirname "$t")"
}

windows() {
  cargo build --release --target x86_64-pc-windows-gnu --manifest-path "$raiz/Cargo.toml"
  local exe="$alvo/x86_64-pc-windows-gnu/release/feathernotes.exe"
  x86_64-w64-mingw32-strip "$exe" 2>/dev/null || true
  (cd "$pacote" && makensis -V2 -DVERSAO="$versao" -DEXE="$exe" -DSAIDA="$saida/FeatherNotes-windows-x64.exe" feathernotes.nsi)
}

for a in "${alvos[@]}"; do
  echo "== $a"
  "$a"
done
(cd "$saida" && sha256sum -- * | grep -v SHA256SUMS > SHA256SUMS)
ls -lh "$saida"
