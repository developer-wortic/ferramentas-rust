//! Base comum das ferramentas Wortic: jsonW em tudo, JSON só na borda.
//!
//! - [`texto`]: jsonW-texto. Lê com esquema (validado pela libjsonw oficial) e escreve a partir de JSON.
//! - [`config`]: arquivo de configuração em jsonW-texto, com esquema, comentário e vírgula sobrando.
//! - [`formato`]: escolhe a saída de uma resposta (JSON para o navegador, jsonW-texto ou arquivo `.jsw` para quem pede).
//! - [`http`]: o servidor HTTP comum (`tiny_http` com `TCP_NODELAY`).
//! - [`canonico`]: jsonW-texto canônico em memória (o mesmo texto da libjsonw, sem arquivo).
//! - [`arquivo`]: grava e lê linhas no arquivo coluna `.jsw` (o formato de dados), pela ABI oficial.
//!
//! A libjsonw entra sem modificação (terceiros/jsonw, Licença de Uso Sustentável do jsonW); o código daqui segue a licença AAL (uso livre com créditos à Wortic).

pub mod abi;
pub mod arquivo;
pub mod canonico;
pub mod config;
pub mod formato;
pub mod http;
pub mod texto;

pub use formato::Formato;

/// Versão da libjsonw ligada neste binário.
#[must_use]
pub fn versao_jsonw() -> String {
    abi::versao()
}
