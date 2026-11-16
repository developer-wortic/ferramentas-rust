//! As listas guardadas (origens e eventos) como linhas de jsonW-texto, na ordem em que entraram. Cada linha passa pelo
//! esquema da lista ao sair ([`crate::jsonw::lista`]). O clid sai em hash, marcado `+pessoal` no esquema: o valor não sai.

use crate::captura::Plataforma;
use crate::dados::sha256_hex;
use crate::jsonw::{dinheiro, obj, opcional, tempo, texto};
use crate::modelo::{Origem, Registro};

/// Uma linha por origem (`tempo`, página, UTM e os clids em hash). Segue o esquema `ORIGEM`.
#[must_use]
pub fn origens(origens: &[Origem]) -> Vec<String> {
    origens.iter().map(linha_origem).collect()
}

/// Uma linha por evento (`tempo`, evento, pedido, valor em dinheiro, origem e situação). Segue o esquema `REGISTRO`.
#[must_use]
pub fn eventos(registros: &[Registro]) -> Vec<String> {
    registros.iter().map(linha_evento).collect()
}

fn linha_origem(origem: &Origem) -> String {
    let utm = &origem.captura.utm;
    let mut campos: Vec<(String, String)> = vec![
        ("tempo".into(), tempo(origem.quando_ms)),
        ("pagina".into(), texto(&origem.pagina)),
        ("fonte".into(), opcional(utm.source.as_deref())),
        ("meio".into(), opcional(utm.medium.as_deref())),
        ("campanha".into(), opcional(utm.campaign.as_deref())),
        ("conteudo".into(), opcional(utm.content.as_deref())),
        ("termo".into(), opcional(utm.term.as_deref())),
    ];
    for plataforma in Plataforma::TODAS {
        let hash = origem.captura.clid(plataforma).map(sha256_hex);
        campos.push((
            format!("{}_hash", plataforma.parametro()),
            opcional(hash.as_deref()),
        ));
    }
    let escritos: Vec<(&str, String)> = campos
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect();
    obj(&escritos)
}

fn linha_evento(registro: &Registro) -> String {
    let valor = match (registro.valor_centavos, registro.moeda.as_deref()) {
        (Some(centavos), Some(moeda)) => dinheiro(centavos, moeda),
        _ => "null".to_string(),
    };
    obj(&[
        ("tempo", tempo(registro.quando_ms)),
        ("evento", texto(registro.evento)),
        ("pedido", texto(&registro.event_id)),
        ("valor", valor),
        ("origem", texto(&registro.origem)),
        ("situacao", texto(&registro.situacao)),
    ])
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::captura;
    use crate::esquemas;
    use crate::jsonw;

    fn origem_com_clid() -> Origem {
        Origem {
            quando_ms: 1_700_000_000_000,
            pagina: "https://loja.com/".to_string(),
            captura: captura::de_url(
                "https://loja.com/?utm_source=facebook&utm_campaign=verao&fbclid=CLIQUE123",
            ),
        }
    }

    #[test]
    fn origem_sai_com_clid_em_hash_e_passa_pelo_esquema() {
        let linhas = origens(&[origem_com_clid()]);
        let canonico = jsonw::conferir(esquemas::ORIGEM, &linhas[0]).expect("esquema");
        assert!(
            !canonico.contains("CLIQUE123"),
            "o valor do clid não sai: {canonico}"
        );
        let hash = sha256_hex("CLIQUE123");
        assert!(
            canonico.contains(&format!("fbclid_hash:\"{hash}\"")),
            "{canonico}"
        );
        assert!(canonico.contains("fonte:\"facebook\""), "{canonico}");
        assert!(canonico.contains("gclid_hash:null"), "{canonico}");
        assert!(
            canonico.contains("tempo:t\"2023-11-14T22:13:20"),
            "{canonico}"
        );
    }

    #[test]
    fn evento_sai_com_dinheiro_na_moeda_e_passa_pelo_esquema() {
        let registro = Registro {
            quando_ms: 1_700_000_000_000,
            evento: "Purchase",
            event_id: "pedido-1001".to_string(),
            valor: Some("99,90 BRL".to_string()),
            valor_centavos: Some(9990),
            moeda: Some("BRL".to_string()),
            origem: "facebook / cpc / verao".to_string(),
            situacao: "demonstração: nada foi enviado".to_string(),
        };
        let linhas = eventos(&[registro]);
        let canonico = jsonw::conferir(esquemas::REGISTRO, &linhas[0]).expect("esquema");
        assert!(canonico.contains("valor:99.90 BRL"), "{canonico}");
        assert!(canonico.contains("pedido:\"pedido-1001\""), "{canonico}");
    }

    #[test]
    fn evento_sem_valor_sai_com_valor_nulo() {
        let registro = Registro {
            quando_ms: 1,
            evento: "Lead",
            event_id: "l1".to_string(),
            valor: None,
            valor_centavos: None,
            moeda: None,
            origem: "sem origem".to_string(),
            situacao: "recusado: sem_consentimento".to_string(),
        };
        let canonico =
            jsonw::conferir(esquemas::REGISTRO, &eventos(&[registro])[0]).expect("esquema");
        assert!(canonico.contains("valor:null"), "{canonico}");
    }

    #[test]
    fn origem_sem_utm_guarda_so_o_clid() {
        let sem_utm = Origem {
            quando_ms: 2,
            pagina: "https://loja.com/".to_string(),
            captura: captura::de_url("https://loja.com/?gclid=G1"),
        };
        let canonico = jsonw::conferir(esquemas::ORIGEM, &origens(&[sem_utm])[0]).expect("esquema");
        assert!(canonico.contains("fonte:null"), "{canonico}");
        assert!(
            canonico.contains(&format!("gclid_hash:\"{}\"", sha256_hex("G1"))),
            "{canonico}"
        );
    }
}
