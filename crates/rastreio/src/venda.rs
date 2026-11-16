//! A venda que chega ao `/evento` (formulário, jsonW-texto ou JSON) e a que a linha de comando lê. Os dois caminhos
//! passam pelas mesmas conferências, na mesma ordem. Nada daqui toca a rede.

use crate::evento::{self, Nome};
use crate::jsonw;

/// Um erro de entrada, com o status HTTP, o código e a frase que o `/evento` responde.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Erro {
    /// O status HTTP da resposta.
    pub status: u16,
    /// O código curto (`evento_invalido`, `jsonw_invalido`...).
    pub codigo: &'static str,
    /// A frase para quem chamou.
    pub mensagem: String,
}

fn erro(status: u16, codigo: &'static str, mensagem: impl Into<String>) -> Erro {
    Erro {
        status,
        codigo,
        mensagem: mensagem.into(),
    }
}

/// A venda já conferida, pronta para montar o evento.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pedido<'a> {
    /// Qual evento é.
    pub nome: Nome,
    /// O número do pedido (identificador do evento).
    pub pedido: &'a str,
    /// A página da venda (com a consulta; a consulta sai depois).
    pub pagina: &'a str,
    /// O valor em centavos, se a venda trouxe.
    pub valor: Option<u32>,
    /// A moeda (`BRL` por padrão).
    pub moeda: &'a str,
}

/// Confere a venda na ordem do `/evento`: evento, pedido, página e valor.
///
/// # Errors
/// O primeiro campo que não cumpre, com o status e o código de resposta.
pub fn pedido(campos: &[(String, String)]) -> Result<Pedido<'_>, Erro> {
    let nome = buscar(campos, "evento").unwrap_or("Purchase");
    let nome = Nome::de_texto(nome).ok_or_else(|| {
        erro(
            400,
            "evento_invalido",
            "evento precisa ser Purchase, Lead, InitiateCheckout ou PageView",
        )
    })?;
    let pedido = buscar(campos, "pedido").ok_or_else(|| {
        erro(
            400,
            "pedido_faltando",
            "informe o número do pedido em pedido",
        )
    })?;
    let pagina = buscar(campos, "u")
        .ok_or_else(|| erro(400, "pagina_faltando", "informe a página da venda em u"))?;
    let valor = buscar(campos, "valor")
        .map(|texto| {
            evento::centavos(texto)
                .ok_or_else(|| erro(400, "valor_invalido", "valor precisa ser como 99,90"))
        })
        .transpose()?;
    let moeda = buscar(campos, "moeda").unwrap_or("BRL");
    Ok(Pedido {
        nome,
        pedido,
        pagina,
        valor,
        moeda,
    })
}

/// Os campos de um registro jsonW da venda (`VENDA` ou `EVENTO`), conferido pela libjsonw. O `valor` em dinheiro
/// (`12.50 BRL`) vira o número e a moeda, do jeito que o formulário manda.
///
/// # Errors
/// O texto não casa com o esquema: `jsonw_invalido`, com a razão sem trecho do texto.
pub fn campos_de(esquema: &str, registro: &str) -> Result<Vec<(String, String)>, Erro> {
    let pares = jsonw::ler_registro(esquema, registro)
        .map_err(|razao| erro(400, "jsonw_invalido", razao))?;
    let mut campos = Vec::with_capacity(pares.len() + 1);
    for (chave, valor) in pares {
        match (chave.as_str(), valor.rsplit_once(' ')) {
            ("valor", Some((numero, moeda))) => {
                campos.push(("valor".to_string(), numero.to_string()));
                campos.push(("moeda".to_string(), moeda.to_string()));
            }
            _ => campos.push((chave, valor)),
        }
    }
    Ok(campos)
}

/// O valor do primeiro par com esse nome. Valor vazio ou só com espaço conta como ausente.
#[must_use]
pub fn buscar<'a>(pares: &'a [(String, String)], nome: &str) -> Option<&'a str> {
    pares
        .iter()
        .find(|(chave, _)| chave == nome)
        .map(|(_, valor)| valor.as_str())
        .filter(|valor| !valor.trim().is_empty())
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::esquemas;

    fn pares(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(k, x)| ((*k).to_string(), (*x).to_string()))
            .collect()
    }

    #[test]
    fn venda_boa_passa_com_padroes() {
        let campos = pares(&[("pedido", "1001"), ("u", "https://loja.com/o")]);
        let p = pedido(&campos).expect("venda");
        assert_eq!(p.nome, Nome::Purchase);
        assert_eq!(p.valor, None);
        assert_eq!(p.moeda, "BRL");
    }

    #[test]
    fn erros_saem_na_ordem_do_evento() {
        let casos = [
            (
                pares(&[("evento", "Nada"), ("pedido", "1"), ("u", "x")]),
                "evento_invalido",
            ),
            (pares(&[("u", "https://a.com/")]), "pedido_faltando"),
            (pares(&[("pedido", "1")]), "pagina_faltando"),
            (
                pares(&[("pedido", "1"), ("u", "https://a.com/"), ("valor", "1,234")]),
                "valor_invalido",
            ),
        ];
        for (campos, codigo) in casos {
            let e = pedido(&campos).expect_err("erro");
            assert_eq!(e.codigo, codigo);
            assert_eq!(e.status, 400);
        }
    }

    #[test]
    fn jsonw_da_venda_traz_o_dinheiro_como_valor_e_moeda() {
        let campos = campos_de(
            esquemas::VENDA,
            r#"{evento:"Purchase",pedido:"1001",u:"https://loja.com/obrigado",valor:99.90 BRL,email:"ana@exemplo.com"}"#,
        )
        .expect("campos");
        assert_eq!(buscar(&campos, "valor"), Some("99.90"));
        assert_eq!(buscar(&campos, "moeda"), Some("BRL"));
        let p = pedido(&campos).expect("pedido");
        assert_eq!(p.valor, Some(9990));
        assert_eq!(p.moeda, "BRL");
    }

    #[test]
    fn jsonw_sem_pedido_e_jsonw_invalido() {
        let e = campos_de(esquemas::VENDA, r#"{u:"https://loja.com/"}"#).expect_err("erro");
        assert_eq!(e.codigo, "jsonw_invalido");
        assert_eq!(e.status, 400);
    }

    #[test]
    fn jsonw_com_chave_que_nao_existe_e_recusado_sem_trecho() {
        let e = campos_de(
            esquemas::VENDA,
            r#"{pedido:"1",u:"https://a.com/",cartao:"4111"}"#,
        )
        .expect_err("erro");
        assert_eq!(e.codigo, "jsonw_invalido");
        assert!(!e.mensagem.contains("4111"), "{}", e.mensagem);
    }

    #[test]
    fn buscar_ignora_vazio() {
        let campos = pares(&[("a", "  "), ("b", "1")]);
        assert_eq!(buscar(&campos, "a"), None);
        assert_eq!(buscar(&campos, "b"), Some("1"));
    }
}
