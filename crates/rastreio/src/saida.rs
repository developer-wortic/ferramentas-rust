//! As saídas em jsonW dos objetos que a ferramenta monta: o evento da Conversions API e o `user_data` em hash. Cada uma
//! sai como jsonW-texto, no mesmo formato de [`crate::esquemas::EVENTO_MONTADO`] e [`crate::esquemas::DADOS_HASH`];
//! quem pede JSON recebe a conversão feita na borda ([`crate::jsonw::como_json`]).

use crate::evento::{Evento, UserData};
use crate::jsonw::{lista_de, obj, texto};

/// O evento montado, como jsonW-texto (sem o envelope `data` do lote). Campo sem valor não sai, como no corpo da Meta.
#[must_use]
pub fn evento(evento: &Evento) -> String {
    let custom = evento.custom_data.as_ref().map(|c| {
        obj(&[
            ("value", c.value.to_string()),
            ("currency", texto(&c.currency)),
        ])
    });
    presentes(vec![
        ("event_name", Some(texto(evento.event_name))),
        ("event_time", Some(evento.event_time.to_string())),
        ("event_id", Some(texto(&evento.event_id))),
        ("action_source", Some(texto(evento.action_source))),
        ("event_source_url", Some(texto(&evento.event_source_url))),
        ("user_data", Some(user_data(&evento.user_data))),
        ("custom_data", custom),
    ])
}

/// O `user_data` em hash, como jsonW-texto. Cada dado hasheado é uma lista com um hash; IP, user agent, fbc e fbp
/// vão em claro, como a Meta pede. Dado que não veio não sai.
#[must_use]
pub fn user_data(u: &UserData) -> String {
    presentes(vec![
        ("em", lista(u.em.as_deref())),
        ("ph", lista(u.ph.as_deref())),
        ("fn", lista(u.fn_.as_deref())),
        ("ln", lista(u.ln.as_deref())),
        ("db", lista(u.db.as_deref())),
        ("ct", lista(u.ct.as_deref())),
        ("st", lista(u.st.as_deref())),
        ("zp", lista(u.zp.as_deref())),
        ("country", lista(u.country.as_deref())),
        ("external_id", lista(u.external_id.as_deref())),
        (
            "client_ip_address",
            u.client_ip_address.as_deref().map(texto),
        ),
        (
            "client_user_agent",
            u.client_user_agent.as_deref().map(texto),
        ),
        ("fbc", u.fbc.as_deref().map(texto)),
        ("fbp", u.fbp.as_deref().map(texto)),
    ])
}

/// Uma lista de hashes, quando o dado veio.
fn lista(itens: Option<&[String]>) -> Option<String> {
    itens.map(|v| lista_de(&v.iter().map(|s| texto(s)).collect::<Vec<_>>()))
}

/// Só os campos que têm valor entram no objeto.
fn presentes(campos: Vec<(&str, Option<String>)>) -> String {
    let escritos: Vec<(&str, String)> = campos
        .into_iter()
        .filter_map(|(chave, valor)| valor.map(|v| (chave, v)))
        .collect();
    obj(&escritos)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::esquemas;
    use crate::evento::{CustomData, Evento};
    use crate::jsonw;

    fn evento_de_teste(com_valor: bool) -> Evento {
        Evento {
            event_name: "Purchase",
            event_time: 1_700_000_000,
            event_id: "pedido-1001".to_string(),
            action_source: "website",
            event_source_url: "https://loja.com/obrigado".to_string(),
            user_data: UserData {
                em: Some(vec![
                    "62a14e44f765419d10fea99367361a727c12365e2520f32218d505ed9aa0f62f".to_string(),
                ]),
                fbc: Some("fb.1.1700000000000.AbC".to_string()),
                ..UserData::default()
            },
            custom_data: com_valor.then(|| CustomData {
                value: 99.9,
                currency: "BRL".to_string(),
            }),
        }
    }

    #[test]
    fn evento_montado_passa_pelo_esquema_e_traz_os_campos() {
        for com_valor in [true, false] {
            let texto_jsonw = evento(&evento_de_teste(com_valor));
            let canonico = jsonw::conferir(esquemas::EVENTO_MONTADO, &texto_jsonw)
                .unwrap_or_else(|r| panic!("{r}\n{texto_jsonw}"));
            assert!(canonico.contains("event_name:\"Purchase\""), "{canonico}");
            assert!(
                canonico.contains("fbc:\"fb.1.1700000000000.AbC\""),
                "{canonico}"
            );
            assert_eq!(
                canonico.contains("currency:\"BRL\""),
                com_valor,
                "{canonico}"
            );
        }
    }

    #[test]
    fn user_data_sem_dado_fica_nulo_no_canonico_e_passa_pelo_esquema() {
        let texto_jsonw = user_data(&UserData {
            ph: Some(vec!["aa".to_string()]),
            ..UserData::default()
        });
        assert!(
            !texto_jsonw.contains("em:"),
            "o que não veio não sai: {texto_jsonw}"
        );
        let canonico = jsonw::conferir(esquemas::DADOS_HASH, &texto_jsonw).expect("esquema");
        assert!(canonico.contains("em:null"), "{canonico}");
        assert!(canonico.contains("ph:[\"aa\"]"), "{canonico}");
    }
}
