#!/bin/bash
# Mede as ferramentas: tamanho, partida (até a 1a resposta), memória e pedidos por segundo em rotas sem rede.
# uso: bancada/medir.sh <rotulo> <modo> [binario]
#   modo antes: binários separados em $ANTES/<repo>/target/release/<nome>
#   modo wortic: o binário único (wortic <ferramenta>)
# Saída em jsonW-texto, uma linha por medida.
set -u
ROT=$1; MODO=$2; WORTIC=${3:-}
ANTES=/srv/obra/rust-unico/antes
B=$(dirname "$0"); WB=/tmp/wbanca
[ -x $WB ] || gcc -O2 -pthread -o $WB $B/wbanca.c
N=${N:-5000}; C=${C:-4}
PIX='/api/gerar?chave=teste%40exemplo.com.br&nome=Loja%20Exemplo&cidade=Porto%20Alegre&valor=12.50&txid=PEDIDO42'
cmd() { # ferramenta -> comando
  if [ $MODO = antes ]; then case $1 in
    cnpj) echo $ANTES/modelo-cadastro-cnpj-rust/target/release/cadastro-cnpj;;
    chatbot) echo $ANTES/modelo-chatbot-prospeccao-rust/target/release/chatbot-prospeccao;;
    pix) echo $ANTES/modelo-pix-brcode-rust/target/release/pix-brcode;;
    rastreio) echo $ANTES/modelo-rastreio-capi-rust/target/release/rastreio-capi-modelo;; esac
  else echo "$WORTIC $1"; fi
}
porta=18300
for f in cnpj chatbot pix rastreio; do
  porta=$((porta+1)); d=$(mktemp -d)
  t0=$(date +%s%N)
  ( cd $d; PORTA=$porta ENDERECO=127.0.0.1 CHATBOT_DADOS=$d RASTREIO_SEGREDO=0123456789abcdef0123456789abcdef exec $(cmd $f) >/dev/null 2>&1 ) & pid=$!
  until curl -s -o /dev/null http://127.0.0.1:$porta/; do sleep 0.002; done
  t1=$(date +%s%N)
  case $f in
    cnpj) rotas="/ /api/config /api/empresa/11111111111111";;
    chatbot) rotas="/ /api/config /widget.js";;
    pix) rotas="/ /api/config $PIX";;
    rastreio) rotas="/ /r?utm_source=facebook&utm_medium=cpc&utm_campaign=bf&fbclid=AbC123";;
  esac
  for r in $rotas; do
    $WB $porta "$r" $N $C | sed "s/^{/{rotulo:\"$ROT\",ferramenta:\"$f\",/"
  done
  rss=$(awk '/VmHWM/{print $2}' /proc/$pid/status)
  echo "{rotulo:\"$ROT\",ferramenta:\"$f\",partida_ms:$(( (t1-t0)/1000000 )),pico_memoria_kb:$rss}"
  kill $pid; wait $pid 2>/dev/null; rm -rf $d
done
