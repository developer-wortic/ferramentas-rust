# Cadastro pelo CNPJ: modelo em Rust

<p>
  <a href="https://cnpj.wortic.com"><img alt="API" src="https://img.shields.io/badge/API-cnpj.wortic.com-0049ff?style=flat-square"></a>
  <img alt="Rust" src="https://img.shields.io/badge/Rust-2024-1d1d1f?style=flat-square">
  <img alt="licença" src="https://img.shields.io/badge/licen%C3%A7a-MIT-1d1d1f?style=flat-square">
  <a href="#english"><img alt="English" src="https://img.shields.io/badge/English-summary-2ac2f3?style=flat-square"></a>
</p>

**Consulta de CNPJ em Rust, pronta para usar:** um cadastro de clientes que se preenche sozinho a partir do CNPJ ou do nome
da empresa, com os dados abertos da Receita Federal (73,8 milhões de estabelecimentos) pela API da Wortic.

Um cadastro de clientes que se preenche sozinho. A pessoa digita o CNPJ ou o nome da empresa e a tela traz razão social,
situação na Receita, atividade, endereço, telefone e e-mail. Backend em Rust, tela em HTML puro, dados da API aberta
[cnpj.wortic.com](https://cnpj.wortic.com).

## Rodar

```sh
cargo run --release
```

Abra http://127.0.0.1:8080. Não precisa de cadastro nem de chave.

## O que vem pronto

- **Um binário só.** A tela vai embutida no executável (`static/index.html`).
- **CNPJ conferido antes de consultar.** Os dígitos verificadores são checados no seu servidor, inclusive no CNPJ
  alfanumérico. Número errado não gasta consulta (`src/cnpj.rs`, com testes).
- **Guarda em memória por um dia.** A segunda consulta da mesma empresa não sai do seu servidor (`src/api.rs`).
- **Limite tratado.** No 429 a tela diz quantos segundos esperar, lendo o `retry-after`.
- **Busca por nome.** Quem não sabe o CNPJ digita o nome e escolhe na lista.

## Rotas do seu backend

| Rota | O que faz |
|---|---|
| `GET /` | A tela do cadastro |
| `GET /api/empresa/{cnpj}` | Ficha da empresa |
| `GET /api/socios/{cnpj}` | Sócios da empresa |
| `GET /api/empresas?nome=&uf=` | Empresas pelo nome, 4 letras ou mais |

## Ajustes

| Variável | Para quê | Padrão |
|---|---|---|
| `PORTA` | Porta do servidor | `8080` |
| `WORTIC_API_KEY` | Chave do seu plano, para volume acima do uso aberto | sem chave |

Sem chave, a API aceita 60 consultas por minuto por IP. Quem passa do limite repetidas vezes é bloqueado: a regra está em
[cnpj.wortic.com/docs](https://cnpj.wortic.com/docs).

## Onde mexer

- **Salvar de verdade:** o botão "Salvar cliente" guarda só na sessão do navegador. Troque o trecho do `submit` em
  `static/index.html` por um `POST` para o seu banco.
- **Mais campos:** a ficha traz também CNAEs secundários, natureza jurídica, porte, capital social, Simples e MEI.
  A lista completa está em [openapi.json](https://cnpj.wortic.com/openapi.json).
- **Publicar:** `cargo build --release` gera `target/release/cadastro-cnpj`. Ponha atrás do seu proxy com HTTPS.

## Testes

```sh
cargo test
```

Fonte dos dados: Cadastro Nacional da Pessoa Jurídica, dados abertos da Receita Federal, pela Wortic.

## Perguntas frequentes

**Como consultar um CNPJ em Rust?**
Rode este modelo e chame `GET /api/empresa/{cnpj}` no seu backend, ou copie o `src/api.rs`: ele faz a chamada à
`https://cnpj.wortic.com`, confere os dígitos antes e guarda a resposta por um dia.

**A API de CNPJ é gratuita?**
Sim, até 60 consultas por minuto por IP, sem cadastro. Quem assina a Wortic tem 1 milhão de consultas por mês, busca
avançada por atividade, cidade, porte e capital, e exportação em planilha ou jsonW.

**Aceita o CNPJ alfanumérico?**
Sim. A conferência dos dígitos em `src/cnpj.rs` já segue a regra nova da Receita Federal.

**De onde vêm os dados?**
Dos dados abertos do Cadastro Nacional da Pessoa Jurídica, publicados pela Receita Federal.

**Tem em outras linguagens?**
A documentação traz exemplos prontos em 10 linguagens: [cnpj.wortic.com/docs](https://cnpj.wortic.com/docs).

## English

**Brazilian company registry (CNPJ) lookup in Rust.** A ready-to-run customer form that fills itself from a CNPJ or a company
name, backed by the free [cnpj.wortic.com](https://cnpj.wortic.com) API (73.8 million establishments from Brazil's Federal
Revenue open data). One binary with the page embedded, check digits validated locally (including the new alphanumeric
CNPJ), one-day in-memory cache and `retry-after` handling. `cargo run --release`, then open http://127.0.0.1:8080. MIT
licensed.

## Mais da Wortic

- Outras APIs abertas, guias e conectores para IA: [wortic.com.br/desenvolvedores](https://wortic.com.br/desenvolvedores).
- Volume maior (1 milhão de consultas por mês), busca avançada de empresas e os outros produtos da Wortic numa assinatura
  só: [wortic.com.br/planos](https://wortic.com.br/planos).

## Licença

MIT. Use, mude e venda o seu sistema feito a partir deste modelo.
