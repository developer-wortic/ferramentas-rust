# Ferramentas abertas da Wortic, em Rust

Quatro ferramentas num projeto só e num binário só, o `wortic`, com **jsonW em tudo**: a configuração é jsonW-texto
com esquema, as listas e os dados vão em arquivo `.jsw`, e a linha de comando lê e escreve jsonW. JSON só sai onde o outro
lado só fala JSON: o navegador, a API do provedor de IA, a Conversions API da Meta e a API de CNPJ.

[![Testes](https://github.com/developer-wortic/ferramentas-rust/actions/workflows/ci.yml/badge.svg)](https://github.com/developer-wortic/ferramentas-rust/actions/workflows/ci.yml)

| Subcomando | O que faz | Antes era |
|---|---|---|
| [`wortic cnpj`](crates/cnpj/README.md) | Cadastro de clientes que se preenche pelo CNPJ, com a API aberta cnpj.wortic.com (tela, API, consulta e lote na linha de comando) | `modelo-cadastro-cnpj-rust` |
| [`wortic chatbot`](crates/chatbot/README.md) | Chatbot de prospecção com a sua chave (OpenAI, Anthropic, Gemini, Groq ou OpenRouter): roteiro em jsonW, resumo do lead com nota, widget para o site | `modelo-chatbot-prospeccao-rust` |
| [`wortic pix`](crates/pix/README.md) | Pix estático: gera e lê o BR Code, confere chaves, desenha o QR em SVG, gera em lote | `modelo-pix-brcode-rust` |
| [`wortic rastreio`](crates/rastreio/README.md) | Rastreio de UTM e cliques e envio de conversões à Conversions API da Meta, com consentimento LGPD | `modelo-rastreio-capi-rust` |

## Começar

```sh
git clone https://github.com/developer-wortic/ferramentas-rust
cd ferramentas-rust
cargo build --release          # um binário: target/release/wortic
./target/release/wortic pix    # sobe a tela do Pix em http://127.0.0.1:8080
```

Precisa do Rust 1.88 ou mais novo. Roda em Linux (x86_64 e arm64) e macOS (arm64); a libjsonw dessas plataformas já vem
em `terceiros/jsonw`. Sem argumento, cada ferramenta sobe o servidor dela; `wortic <ferramenta> ajuda` mostra os
subcomandos de linha de comando.

```text
wortic [--config wortic.jsw.txt] <ferramenta> [argumentos]

  cnpj       consultar, socios, buscar, validar, lote          (sem argumento: tela + API)
  chatbot    roteiro exemplo|conferir, migrar, leads           (sem argumento: painel + widget)
  pix        gerar, ler, chave, lote                           (sem argumento: tela + API)
  rastreio   captura, dados, evento, registros, --config       (sem argumento: painel + pixel)
  config     mostra o arquivo de configuração de exemplo
  versao     versão do wortic e da libjsonw
```

Quem usava os binários separados não precisa mudar nada: um link com o nome antigo (`pix-brcode`, `cadastro-cnpj`,
`chatbot-prospeccao`, `rastreio-capi-modelo`) apontando para `wortic` sobe a ferramenta daquele nome, com as mesmas
variáveis (`PORTA`, `ENDERECO`, `WORTIC_API_KEY`, `CHATBOT_*`, `RASTREIO_*`).

## jsonW em tudo

[jsonW](https://jsonw.org) é um formato de dados com três formas que se convertem sem perda: texto (para gente escrever;
todo JSON válido já é jsonW-texto), linha e arquivo coluna `.jsw` (lote, busca, custódia por hash). Aqui:

- **Configuração.** `wortic --config wortic.jsw.txt` (ou `WORTIC_CONFIG`) lê um arquivo jsonW-texto com uma seção por
  ferramenta, conferido contra o esquema: seção que não existe ou valor de tipo errado para com a razão. O roteiro do
  chatbot é `roteiro.jsw.txt` e a configuração dele, `configuracao.jsw.txt` (600); quem tinha os `.toml` antigos tem a
  conversão feita sozinha na partida (o original fica em `.toml.antes`). O rastreio aceita `RASTREIO_CONFIG`.
- **Dados.** Listas saem em `.jsw` com esquema tipado: leads do chatbot (`/api/leads?formato=jsw`), origens e eventos do
  rastreio (`/registros/origens`, `/registros/eventos`, ou gravados em disco com `RASTREIO_ARQUIVO`), lotes do Pix e do
  CNPJ. Dinheiro é `dinheiro` (`12.50 BRL`), tempo é `tempo`, identificador é `codigo`, dado pessoal só em hash.
- **Entrada e saída.** A linha de comando escreve jsonW-texto canônico (`--json` para JSON) e lê jsonW-texto. Toda rota
  JSON das telas responde em jsonW-texto com `Accept: text/jsonw` ou `?formato=jsonw`, e as listas em `.jsw` com
  `Accept: application/jsonw` ou `?formato=jsw`. O padrão das rotas continua JSON, porque quem lê é o navegador.

```text
$ wortic pix gerar --chave 123.456.789-09 --nome "Loja Exemplo" --cidade "Sao Paulo" --valor 10,50 --txid PED42
{codigo:"00020101021126330014br.gov.bcb.pix011112345678909520400005303986540510.505802BR5912Loja Exemplo6009Sao Paulo62090505PED426304D15C",qr_svg:null,tipo_chave:"CPF",chave:"12345678909",nome:"Loja Exemplo",cidade:"Sao Paulo",valor:10.50 BRL,txid:"PED42",descricao:null}

$ wortic cnpj validar 12.ABC.345/01DE-35
{cnpj:"12ABC34501DE35",valido:true}
```

Quem lê, confere e grava jsonW é a libjsonw oficial, ligada estática no binário, sem modificação (veja
[terceiros/jsonw](terceiros/jsonw/README.md)). O jsonW-texto canônico de uma resposta sai em memória, igual byte a byte ao
da libjsonw (provado nos testes de `crates/base/src/canonico.rs`) e com o mesmo custo do JSON.

## Antes e depois

Medido no mesmo servidor (contêiner Linux x86_64, 4 núcleos, Rust 1.100, perfil release), com `bancada/medir.sh`: 2.000
pedidos por rota em 4 conexões mantidas, rotas sem rede. "Antes" são os quatro repositórios separados; "depois" é o
`wortic`. Os números brutos estão em `bancada/`.

| | Antes (4 projetos) | Depois (`wortic`) |
|---|---|---|
| Binários | 4, somando 7.151.072 bytes | 1, com 3.174.976 bytes (−56%), já com a libjsonw dentro |
| Build release do zero | 78,3 s (os quatro em sequência) | 39,2 s |
| Testes | 466 | 685, nenhuma falha |
| Cobertura de linhas | por projeto | 98,15% do workspace (`cargo llvm-cov`) |
| Formato de configuração e dados | TOML, JSON e memória | jsonW-texto e `.jsw`, JSON só na borda |

Tempo de resposta (mediana, ida e volta):

| Rota | Antes | Depois |
|---|---|---|
| `wortic cnpj` tela `/` | 41,0 ms | 38 µs |
| `wortic chatbot` painel `/` | 41,9 ms | 62 µs |
| `wortic chatbot` `/widget.js` | 42,0 ms | 60 µs |
| `wortic pix` tela `/` | 42,0 ms | 31 µs |
| `wortic pix` `/api/gerar` (BR Code + QR em SVG) | 41,1 ms | 0,70 ms |
| `wortic rastreio` painel `/` | 41,0 ms | 50 µs |
| rotas pequenas (`/api/config`, `/r`, CNPJ inválido) | 13 a 74 µs | 23 a 65 µs |

Os 41 ms de antes eram o algoritmo de Nagle somado ao ACK atrasado: o `tiny_http` escreve o cabeçalho e o corpo em duas
escritas, e toda resposta maior que o buffer esperava até 40 ms. O servidor comum (`crates/base/src/http.rs`) liga
`TCP_NODELAY`. Nas rotas pequenas, que já cabiam numa escrita, a mediana subiu alguns microssegundos (agora são dois
pacotes), e a vazão subiu porque a cauda de milissegundos sumiu. Memória de pico por processo: de 3,4 a 4,4 MB antes, de
4,6 a 6,0 MB depois (o binário único carrega as quatro ferramentas e a libjsonw). jsonW-texto contra JSON na mesma rota:
26 µs contra 25 µs em `/api/config`, 0,75 ms contra 0,70 ms em `/api/gerar`.

## Como o projeto está organizado

```text
crates/
  base/       jsonW (ABI da libjsonw, texto, canônico, configuração, arquivo .jsw), formato da resposta, servidor HTTP
  cnpj/       biblioteca do cadastro pelo CNPJ (tela em static/)
  chatbot/    biblioteca do chatbot (painel e widget em static/, roteiro.jsw.txt)
  pix/        biblioteca do Pix (tela em static/)
  rastreio/   biblioteca do rastreio e da Conversions API (painel em src/painel.rs)
  wortic/     o binário único e os testes de cada ferramenta rodando de verdade
terceiros/jsonw/   libjsonw por plataforma, jsonw.h e a licença dela (sem modificação)
bancada/           medidor HTTP em C (wbanca.c) e o roteiro de medida
docs/<ferramenta>/ prints e gifs de cada tela
```

Cada ferramenta continua sendo uma biblioteca que dá para usar sozinha (`cadastro-cnpj`, `chatbot-prospeccao`,
`pix-brcode`, `rastreio-capi`). O histórico dos quatro repositórios antigos está neste, cada um em `crates/<ferramenta>`.

## Contribuir, segurança e licença

Veja [CONTRIBUTING.md](CONTRIBUTING.md) e [SECURITY.md](SECURITY.md). O código deste repositório segue a [licença AAL](LICENSE), de uso livre com créditos à Wortic. A
libjsonw em `terceiros/jsonw` segue a [Licença de Uso Sustentável do jsonW](terceiros/jsonw/LICENCA.md), que permite
usá-la e distribuí-la sem modificação.

## English

**Wortic open-source tools in Rust, one project, one binary.** `wortic cnpj` (customer registration autofilled from the
Brazilian company registry), `wortic chatbot` (bring-your-own-key lead qualification chatbot), `wortic pix` (Pix static
BR Code generator and reader with QR) and `wortic rastreio` (UTM/click tracking and Meta Conversions API). Everything
speaks [jsonW](https://jsonw.org): configuration is schema-checked jsonW-text, lists and data are `.jsw` columnar files,
and the CLI reads and writes jsonW (`--json` for JSON). HTTP routes keep JSON as the browser default and answer jsonW with
`Accept: text/jsonw` or `application/jsonw`. Compared with the four former repositories: one 3.2 MB binary instead of four
totalling 7.2 MB, 685 tests instead of 466, and large responses down from ~41 ms to tens of microseconds (TCP_NODELAY).
Build with `cargo build --release`; Attribution Assurance License (free to use with credit to Wortic), with the official libjsonw vendored unmodified under its own license.
