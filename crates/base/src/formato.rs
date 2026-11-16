//! Formato de saída de uma resposta. O navegador (a tela e o widget) recebe JSON, que é a borda; quem pede jsonW
//! recebe jsonW-texto (`text/jsonw`) ou, numa lista, o arquivo coluna `.jsw` (`application/jsonw`).
//!
//! Pedido: cabeçalho `Accept` ou o parâmetro `formato=json|jsonw|jsw` na consulta (o parâmetro vence).

/// Tipo de conteúdo do jsonW-texto.
pub const TIPO_TEXTO: &str = "text/jsonw; charset=utf-8";
/// Tipo de conteúdo do arquivo coluna `.jsw`.
pub const TIPO_ARQUIVO: &str = "application/jsonw";
/// Tipo de conteúdo do JSON.
pub const TIPO_JSON: &str = "application/json; charset=utf-8";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Formato {
    /// JSON (padrão no HTTP, a borda com o navegador).
    #[default]
    Json,
    /// jsonW-texto (padrão na linha de comando).
    Texto,
    /// Arquivo coluna `.jsw` (só em listas).
    Arquivo,
}

impl Formato {
    /// Escolhe pelo parâmetro `formato` (se veio) ou pelo `Accept`. Sem nenhum dos dois, JSON.
    #[must_use]
    pub fn do_pedido(accept: Option<&str>, parametro: Option<&str>) -> Self {
        if let Some(p) = parametro.and_then(Self::do_nome) {
            return p;
        }
        let Some(accept) = accept else {
            return Self::Json;
        };
        let accept = accept.to_ascii_lowercase();
        if accept.contains("application/jsonw") {
            Self::Arquivo
        } else if accept.contains("text/jsonw") {
            Self::Texto
        } else {
            Self::Json
        }
    }

    /// `json`, `jsonw` (ou `texto`) e `jsw` (ou `arquivo`).
    #[must_use]
    pub fn do_nome(nome: &str) -> Option<Self> {
        match nome.trim().to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "jsonw" | "texto" | "jsw.txt" => Some(Self::Texto),
            "jsw" | "arquivo" => Some(Self::Arquivo),
            _ => None,
        }
    }

    /// O `content-type` da resposta.
    #[must_use]
    pub fn tipo(self) -> &'static str {
        match self {
            Self::Json => TIPO_JSON,
            Self::Texto => TIPO_TEXTO,
            Self::Arquivo => TIPO_ARQUIVO,
        }
    }

    /// Um corpo JSON no formato pedido. `Arquivo` num registro só cai para jsonW-texto (o `.jsw` é para listas).
    #[must_use]
    pub fn corpo(self, json: String) -> String {
        match self {
            Self::Json => json,
            Self::Texto | Self::Arquivo => crate::texto::de_json(&json),
        }
    }

    /// O tipo que vai junto de [`Formato::corpo`].
    #[must_use]
    pub fn tipo_do_corpo(self) -> &'static str {
        match self {
            Self::Json => TIPO_JSON,
            Self::Texto | Self::Arquivo => TIPO_TEXTO,
        }
    }
}

/// O valor de um parâmetro numa consulta `a=1&formato=jsonw` (sem decodificar: os nomes de formato são ASCII).
#[must_use]
pub fn parametro<'a>(consulta: &'a str, nome: &str) -> Option<&'a str> {
    consulta
        .trim_start_matches('?')
        .split('&')
        .filter_map(|par| par.split_once('='))
        .find(|(chave, _)| *chave == nome)
        .map(|(_, valor)| valor)
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn padrao_e_json_e_o_parametro_vence_o_accept() {
        assert_eq!(Formato::do_pedido(None, None), Formato::Json);
        assert_eq!(Formato::do_pedido(Some("text/jsonw"), None), Formato::Texto);
        assert_eq!(
            Formato::do_pedido(Some("application/jsonw, */*"), None),
            Formato::Arquivo
        );
        assert_eq!(
            Formato::do_pedido(Some("text/jsonw"), Some("json")),
            Formato::Json
        );
        assert_eq!(Formato::do_pedido(None, Some("jsonw")), Formato::Texto);
        assert_eq!(Formato::do_pedido(None, Some("xml")), Formato::Json);
    }

    #[test]
    fn corpo_em_texto_tira_as_aspas_das_chaves() {
        assert_eq!(
            Formato::Texto.corpo(r#"{"a":"b"}"#.into()),
            r#"{a:"b"}"#
        );
        assert_eq!(Formato::Json.corpo(r#"{"a":"b"}"#.into()), r#"{"a":"b"}"#);
    }

    #[test]
    fn parametro_da_consulta() {
        assert_eq!(parametro("?x=1&formato=jsonw", "formato"), Some("jsonw"));
        assert_eq!(parametro("x=1", "formato"), None);
    }
}
