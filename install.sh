#!/bin/sh
# Instala o azura a partir do último release.
#   curl -fsSL https://raw.githubusercontent.com/edivanteixeira/azura/main/install.sh | sh
# Variáveis: AZURA_VERSION (tag), AZURA_INSTALL_DIR (destino)
set -eu

REPO="edivanteixeira/azura"
DEST="${AZURA_INSTALL_DIR:-/usr/local/bin}"

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)              TARGET=aarch64-apple-darwin ;;
  Darwin-x86_64)             TARGET=x86_64-apple-darwin ;;
  Linux-x86_64)              TARGET=x86_64-unknown-linux-gnu ;;
  Linux-aarch64|Linux-arm64) TARGET=aarch64-unknown-linux-gnu ;;
  *)
    echo "plataforma não suportada: $(uname -s) $(uname -m)"
    echo "baixe o binário em https://github.com/$REPO/releases"
    exit 1
    ;;
esac

TAG="${AZURA_VERSION:-}"
if [ -z "$TAG" ]; then
  TAG=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
    sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')
fi
[ -n "$TAG" ] || { echo "não achei nenhum release em $REPO"; exit 1; }

FILE="azura-$TARGET.tar.gz"
URL="https://github.com/$REPO/releases/download/$TAG/$FILE"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
echo "baixando azura $TAG ($TARGET)…"
curl -fsSL "$URL" -o "$TMP/$FILE"

# confere o checksum quando ele existe no release
if curl -fsSL "$URL.sha256" -o "$TMP/$FILE.sha256" 2>/dev/null; then
  ESPERADO=$(cut -d' ' -f1 < "$TMP/$FILE.sha256")
  if command -v sha256sum >/dev/null 2>&1; then
    OBTIDO=$(sha256sum "$TMP/$FILE" | cut -d' ' -f1)
  else
    OBTIDO=$(shasum -a 256 "$TMP/$FILE" | cut -d' ' -f1)
  fi
  [ "$ESPERADO" = "$OBTIDO" ] || { echo "checksum não bate — abortando"; exit 1; }
fi

tar -xzf "$TMP/$FILE" -C "$TMP"

if [ -w "$DEST" ]; then
  install -m 755 "$TMP/azura" "$DEST/azura"
elif command -v sudo >/dev/null 2>&1; then
  echo "escrevendo em $DEST (sudo)…"
  sudo install -m 755 "$TMP/azura" "$DEST/azura"
else
  echo "sem permissão para escrever em $DEST"
  echo "rode de novo com AZURA_INSTALL_DIR=\$HOME/.local/bin"
  exit 1
fi

echo "azura instalado em $DEST/azura"
echo "rode 'azura' para configurar org, projeto e PAT"
