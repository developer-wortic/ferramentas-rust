# Cadastro pelo CNPJ: modelo em Rust

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

## Mais da Wortic

- Outras APIs abertas, guias e conectores para IA: [wortic.com.br/desenvolvedores](https://wortic.com.br/desenvolvedores).
- Volume maior, busca avançada de empresas e os outros produtos da Wortic numa assinatura só: [wortic.com.br](https://wortic.com.br).

## Licença

MIT. Use, mude e venda o seu sistema feito a partir deste modelo.
