# Segurança

Se você achar uma falha de segurança, não abra issue pública. Relate em privado pelo
[aviso de segurança do GitHub](https://github.com/developer-wortic/ferramentas-rust/security/advisories/new).

Inclua a ferramenta, o que você encontrou, como reproduzir e o impacto que você vê.

Não envie chave de API, token, senha, `RASTREIO_SEGREDO`, chave Pix, CPF, e-mail, telefone ou dado de visitante no relato.
Se uma chave vazou, revogue-a no painel do provedor (ou na sua conta Wortic) e crie outra. Se um token da Conversions API
vazou, revogue-o no Events Manager. Se o segredo do cookie do rastreio vazou, troque `RASTREIO_SEGREDO`: os cookies
antigos param de valer.

Áreas que mais importam:

- **chatbot:** vazamento da chave (log, resposta, tela, arquivo de configuração fora do 600), escape de instrução pela fala
  do visitante (injeção), administração exposta fora da máquina local e CORS além de `CHATBOT_ORIGENS`.
- **rastreio:** a verificação do cookie de atribuição (`src/cookie.rs`), a captura de URL (`src/captura.rs`), o envio com
  token (`src/envio.rs`) e a tela, que escapa todo texto vindo de fora (`src/painel.rs`).
- **cnpj e pix:** a validação do que chega pela rota antes de consultar a API ou montar o BR Code.
- **base:** a leitura de jsonW-texto e de `.jsw` de fora (a libjsonw confere índice, limite e custódia; relate qualquer
  arquivo que a faça cair).

Só a versão mais recente da branch principal recebe correção. Vamos responder pelo mesmo canal.
