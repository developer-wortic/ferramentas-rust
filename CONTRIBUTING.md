# Como contribuir

Obrigado por querer ajudar. São quatro ferramentas pequenas num projeto só, feitas para serem fáceis de ler e de adaptar.

## Antes de começar

- Para mudança grande, abra uma issue antes do código.
- Não ponha dado pessoal nem segredo em issue ou pull request: nada de chave de API, token, `RASTREIO_SEGREDO`, chave Pix,
  CPF, CNPJ, e-mail, telefone, IP ou dado de visitante. Use dados de exemplo (`contato@exemplo.com.br`, `LOJA EXEMPLO`,
  a "Loja Exemplo" do roteiro).
- Pedido de correção ou remoção de dado pessoal segue o modelo de LGPD da ferramenta. Evento recusado pela Meta vai pelo
  modelo "Evento recusado", sem os dados do cliente. Divergência do manual do Pix diz a seção ou a versão do manual.

## Rodar as checagens

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings -W clippy::pedantic
cargo test --workspace
cargo llvm-cov --workspace --summary-only --ignore-filename-regex '(tests/|examples/|/rustc/|\.cargo/)'
```

O CI roda formatação, clippy (pedantic), os testes e o build do binário em Linux (x86_64 e arm64) e macOS (arm64), em cada
push e em cada pull request. A cobertura de linhas do `src/` de cada ferramenta não pode cair.

## Regras do projeto

- Um projeto só: cada ferramenta é uma biblioteca em `crates/<ferramenta>`, a base comum fica em `crates/base` e o
  binário único, em `crates/wortic`. Ferramenta nova entra como mais um crate e mais um subcomando do `wortic`.
- **jsonW em tudo, JSON só na borda.** Configuração em jsonW-texto (`.jsw.txt`) com esquema declarado; listas e dados
  gravados em `.jsw`; a linha de comando fala jsonW-texto (com `--json` quando alguém de fora precisa). JSON só onde o
  outro lado só fala JSON (o navegador, a API do provedor de IA, a Conversions API, a API de CNPJ), marcado no código com
  `// borda-json: <para quem>`.
- A única dependência em C é a libjsonw, que vem pronta em `terceiros/jsonw` (sem modificação, com a licença dela).
  Dependência nova de fora precisa de motivo claro e entra uma vez só no `Cargo.toml` da raiz.
- Mudança de código vem com teste. Provedor novo do chatbot vem com teste contra o servidor falso, conferindo o formato
  da requisição com a documentação oficial. Vetor de hash do rastreio vem da documentação da Meta.
- Nenhum teste fala com serviço real: nem provedor de IA, nem Meta, nem a API de CNPJ. Nenhum teste gasta crédito de
  ninguém.
- Chave e token nunca aparecem em log, em resposta de rota, em erro nem na `Debug`.
- As telas continuam em HTML (e JavaScript puro onde já havia), em `crates/<ferramenta>/static`, sem build de front-end.
- Documentação e interface em português do Brasil, sem travessão, com um resumo em inglês no README.
- O código deste repositório é MIT. Ao contribuir, você concorda que sua contribuição entra sob a mesma licença.

## Pull request

- Diga o que muda e por quê, e em qual ferramenta.
- Inclua a saída de `cargo test --workspace`.
- Se mexer no evento da Conversions API, mostre o corpo gerado (com dados de exemplo da Meta) e os campos que mudaram.
