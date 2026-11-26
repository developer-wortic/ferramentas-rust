//! A saída das rotas e da linha de comando. JSON só na borda: o navegador (a tela) recebe JSON; quem pede jsonW recebe
//! jsonW-texto, que é o mesmo registro com a forma das chaves sem aspas, sem perda.

use serde_json::Value;
use wortic_base::Formato;

use crate::api::RESPOSTA_INVALIDA;

/// O corpo no formato pedido. JSON sai como veio. Os outros formatos trocam só a forma (`texto::de_json`).
/// Corpo que não é JSON não sai rotulado como jsonW: vira o erro `resposta_invalida`, como já faz a tela.
#[must_use]
pub fn corpo(formato: Formato, json: &str) -> String {
    if formato == Formato::Json {
        return json.to_string();
    }
    // borda-json: API cnpj.wortic.com (o corpo ainda é o da API: só se confere que é JSON antes de trocar a forma)
    if serde_json::from_str::<Value>(json).is_ok() {
        formato.corpo(json.to_string())
    } else {
        formato.corpo(RESPOSTA_INVALIDA.to_string())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn json_sai_como_veio() {
        let json = r#"{"razao":"x", "n": 1}"#;
        assert_eq!(corpo(Formato::Json, json), json);
    }

    #[test]
    fn jsonw_tira_as_aspas_das_chaves_sem_mexer_no_valor() {
        assert_eq!(
            corpo(Formato::Texto, r#"{"razao":"A \"B\"","lista":[1,2]}"#),
            r#"{razao:"A \"B\"",lista:[1,2]}"#
        );
    }

    #[test]
    fn lista_de_socios_tambem_vira_jsonw() {
        assert_eq!(
            corpo(Formato::Texto, r#"[{"nome":"ANA"}]"#),
            r#"[{nome:"ANA"}]"#
        );
    }

    #[test]
    fn arquivo_num_registro_so_cai_para_jsonw_texto() {
        assert_eq!(corpo(Formato::Arquivo, r#"{"a":1}"#), "{a:1}");
    }

    #[test]
    fn corpo_que_nao_e_json_vira_erro_jsonw_com_o_mesmo_codigo_da_tela() {
        let texto = corpo(Formato::Texto, "manutencao");
        assert!(
            texto.starts_with("{erro:{codigo:\"resposta_invalida\""),
            "{texto}"
        );
        // o JSON cru continua passando como veio, como antes
        assert_eq!(corpo(Formato::Json, "manutencao"), "manutencao");
    }
}
