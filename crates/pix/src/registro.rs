//! Os registros do Pix em jsonW-texto, montados uma vez e usados pela linha de comando, pela API e pelo `.jsw`. A ordem
//! das chaves é a que a tela já recebia (os campos novos vêm no fim), e o esquema de cada um está em `jsonw`. Quem
//! escolhe o formato de saída é `jsonw::registro`.

use crate::chave::Chave;
use crate::cobranca::Cobranca;
use crate::erro::Erro;
use crate::leitura::Leitura;
use crate::valor;

/// Uma cobrança: o copia e cola, o QR (só quando foi pedido), o tipo e o valor da chave, o recebedor, o valor e o txid.
///
/// # Errors
///
/// Devolve o erro de `Cobranca::codigo` se a chave e a descrição juntas passarem do limite do campo 26.
pub fn cobranca(cobranca: &Cobranca, qr_svg: Option<&str>) -> Result<String, Erro> {
    let codigo = cobranca.codigo()?;
    Ok(format!(
        "{{codigo:{},qr_svg:{},tipo_chave:{},chave:{},nome:{},cidade:{},valor:{},txid:{},descricao:{}}}",
        cadeia(&codigo),
        opcional(qr_svg.map(cadeia)),
        cadeia(cobranca.chave.tipo().nome()),
        cadeia(cobranca.chave.valor()),
        cadeia(&cobranca.nome),
        cadeia(&cobranca.cidade),
        opcional(
            cobranca
                .valor
                .map(|centavos| format!("{} BRL", valor::texto(centavos)))
        ),
        opcional(cobranca.txid.as_deref().map(cadeia)),
        opcional(cobranca.descricao.as_deref().map(cadeia)),
    ))
}

/// Os campos de um copia e cola lido.
#[must_use]
pub fn leitura(l: &Leitura) -> String {
    let campos: Vec<String> = l
        .campos
        .iter()
        .map(|c| format!("{{id:{},valor:{}}}", cadeia(&c.id), cadeia(&c.valor)))
        .collect();
    format!(
        "{{chave:{},descricao:{},valor:{},moeda:{},pais:{},nome:{},cidade:{},categoria:{},txid:{},campos:[{}]}}",
        cadeia(&l.chave),
        opcional(l.descricao.as_deref().map(cadeia)),
        opcional(l.valor.as_deref().map(cadeia)),
        cadeia(&l.moeda),
        cadeia(&l.pais),
        cadeia(&l.nome),
        cadeia(&l.cidade),
        cadeia(&l.categoria),
        opcional(l.txid.as_deref().map(cadeia)),
        campos.join(",")
    )
}

/// Uma chave conferida: o valor normalizado e o tipo.
#[must_use]
pub fn chave(chave: &Chave) -> String {
    format!(
        "{{chave:{},tipo_chave:{}}}",
        cadeia(chave.valor()),
        cadeia(chave.tipo().nome())
    )
}

/// A configuração da API: só se há chave (o valor da chave nunca sai).
#[must_use]
pub fn config(tem_chave: bool) -> String {
    format!("{{chave:{tem_chave}}}")
}

/// Um erro, com a linha do lote quando for de uma linha só.
#[must_use]
pub fn erro(linha: Option<usize>, mensagem: &str) -> String {
    let linha = linha.map_or_else(|| "null".to_string(), |n| n.to_string());
    format!("{{erro:{{linha:{linha},mensagem:{}}}}}", cadeia(mensagem))
}

/// O resumo de um lote gravado.
#[must_use]
pub fn lote(registros: u64, arquivo: &str) -> String {
    format!("{{registros:{registros},arquivo:{}}}", cadeia(arquivo))
}

/// Um texto entre aspas no jsonW. As aspas e as barras saem com escape, e o controle vira `\uXXXX`: é a mesma
/// forma de texto que o jsonW lê (todo JSON válido já é jsonW-texto válido).
#[must_use]
pub fn cadeia(texto: &str) -> String {
    use std::fmt::Write;
    let mut saida = String::with_capacity(texto.len() + 2);
    saida.push('"');
    for c in texto.chars() {
        match c {
            '"' => saida.push_str("\\\""),
            '\\' => saida.push_str("\\\\"),
            '\n' => saida.push_str("\\n"),
            '\r' => saida.push_str("\\r"),
            '\t' => saida.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(saida, "\\u{:04x}", u32::from(c));
            }
            c => saida.push(c),
        }
    }
    saida.push('"');
    saida
}

/// O campo opcional em jsonW: o valor, ou `null` quando não há.
fn opcional(valor: Option<String>) -> String {
    valor.unwrap_or_else(|| "null".to_string())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cadeia_escapa_aspas_barras_e_controle() {
        assert_eq!(cadeia("a\"b\\c\nd\re\tf"), "\"a\\\"b\\\\c\\nd\\re\\tf\"");
        assert_eq!(cadeia("\u{1}"), "\"\\u0001\"");
        assert_eq!(cadeia("São"), "\"São\"");
    }

    #[test]
    fn opcional_vira_null_quando_nao_ha_valor() {
        assert_eq!(opcional(None), "null");
        assert_eq!(opcional(Some("x".into())), "x");
    }

    #[test]
    fn cobranca_mantem_a_ordem_antiga_e_acrescenta_os_campos_novos() {
        let c = Cobranca::nova("123.456.789-09", "Ana", "Recife")
            .unwrap()
            .com_valor("10,5")
            .unwrap();
        let registro = cobranca(&c, None).unwrap();
        assert!(registro.starts_with("{codigo:\"000201"));
        assert!(registro.contains("qr_svg:null"));
        assert!(registro.contains("tipo_chave:\"CPF\""));
        assert!(registro.contains("valor:10.50 BRL"));
        assert!(registro.contains("txid:null"));
        assert!(registro.ends_with("descricao:null}"));
    }

    #[test]
    fn cobranca_com_qr_leva_o_svg() {
        let c = Cobranca::nova("123.456.789-09", "Ana", "Recife").unwrap();
        let registro = cobranca(&c, Some("<svg/>")).unwrap();
        assert!(registro.contains("qr_svg:\"<svg/>\""));
    }

    #[test]
    fn erro_com_e_sem_linha() {
        assert_eq!(erro(Some(3), "x"), "{erro:{linha:3,mensagem:\"x\"}}");
        assert_eq!(erro(None, "x"), "{erro:{linha:null,mensagem:\"x\"}}");
    }

    #[test]
    fn config_so_diz_se_ha_chave() {
        assert_eq!(config(true), "{chave:true}");
        assert_eq!(config(false), "{chave:false}");
    }

    #[test]
    fn lote_diz_quantos_e_onde() {
        assert_eq!(lote(2, "saida.jsw"), "{registros:2,arquivo:\"saida.jsw\"}");
    }
}
