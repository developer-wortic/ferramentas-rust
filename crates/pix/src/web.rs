//! As rotas do servidor, sem sockets: cada pedido vira uma `Resposta`. Assim a lógica é testável sem subir a porta.
//! O `servidor` só liga isso ao `tiny_http`.
//!
//! O formato da resposta sai do cabeçalho `Accept` (`text/jsonw`) ou do parâmetro `formato=jsonw` (o parâmetro vence).
//! Sem pedido, é JSON: é o que a tela embutida usa, e a borda com o navegador continua igual.

use wortic_base::Formato;
use wortic_base::formato::{TIPO_JSON, parametro};

use crate::cobranca::Cobranca;
use crate::erro::Erro;
use crate::{jsonw, leitura, qr, registro};

/// A tela, embutida no binário.
pub const TELA: &str = include_str!("../static/index.html");

/// Status, tipo do conteúdo e corpo de uma resposta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resposta {
    pub status: u16,
    pub tipo: &'static str,
    pub corpo: String,
}

const HTML: &str = "text/html; charset=utf-8";

/// Decide a resposta de um caminho. `consulta` é o que vem depois do `?`, sem o `?`; `accept` é o cabeçalho `Accept`.
/// `tem_chave` diz só se `WORTIC_API_KEY` está definida: a chave em si nunca sai daqui.
#[must_use]
pub fn rota(caminho: &str, consulta: &str, accept: Option<&str>, tem_chave: bool) -> Resposta {
    let formato = Formato::do_pedido(accept, parametro(consulta, "formato"));
    match caminho {
        "/" => Resposta {
            status: 200,
            tipo: HTML,
            corpo: TELA.to_string(),
        },
        "/api/config" => api(
            formato,
            200,
            jsonw::ESQUEMA_CONFIG,
            &registro::config(tem_chave),
        ),
        "/api/gerar" => resposta_de(
            formato,
            jsonw::ESQUEMA_COBRANCA,
            gerar(&parametros(consulta)),
        ),
        "/api/ler" => resposta_de(
            formato,
            jsonw::ESQUEMA_LEITURA,
            ler_codigo(&parametros(consulta)),
        ),
        _ => api(
            formato,
            404,
            jsonw::ESQUEMA_ERRO,
            &registro::erro(None, "Rota não encontrada."),
        ),
    }
}

fn resposta_de(formato: Formato, esquema: &str, resultado: Result<String, Erro>) -> Resposta {
    match resultado {
        Ok(registro) => api(formato, 200, esquema, &registro),
        Err(erro) => api(
            formato,
            400,
            jsonw::ESQUEMA_ERRO,
            &registro::erro(None, &erro.to_string()),
        ),
    }
}

/// A resposta da API no formato pedido. Se o registro não passar na libjsonw (erro de programa, não de quem pediu),
/// sai 500 em JSON, que é o formato que não depende de conferência.
fn api(formato: Formato, status: u16, esquema: &str, registro: &str) -> Resposta {
    match jsonw::registro(formato, esquema, registro) {
        Ok(corpo) => Resposta {
            status,
            tipo: formato.tipo_do_corpo(),
            corpo,
        },
        Err(razao) => Resposta {
            status: 500,
            tipo: TIPO_JSON,
            corpo: registro::erro(None, &razao),
        },
    }
}

/// Monta a cobrança estática: devolve o registro com o copia e cola, o QR em SVG e os campos.
fn gerar(p: &[(String, String)]) -> Result<String, Erro> {
    let mut cobranca = Cobranca::nova(
        obrigatorio(p, "chave", "chave Pix")?,
        obrigatorio(p, "nome", "nome do recebedor")?,
        obrigatorio(p, "cidade", "cidade")?,
    )?;
    if let Some(valor) = opcional(p, "valor") {
        cobranca = cobranca.com_valor(valor)?;
    }
    if let Some(txid) = opcional(p, "txid") {
        cobranca = cobranca.com_txid(txid)?;
    }
    if let Some(descricao) = opcional(p, "descricao") {
        cobranca = cobranca.com_descricao(descricao)?;
    }
    let svg = qr::svg(&cobranca.codigo()?)?;
    registro::cobranca(&cobranca, Some(&svg))
}

/// Lê um copia e cola e devolve os campos.
fn ler_codigo(p: &[(String, String)]) -> Result<String, Erro> {
    let l = leitura::ler(obrigatorio(p, "codigo", "código Pix")?)?;
    Ok(registro::leitura(&l))
}

fn obrigatorio<'a>(
    p: &'a [(String, String)],
    nome: &str,
    campo: &'static str,
) -> Result<&'a str, Erro> {
    opcional(p, nome).ok_or(Erro::Obrigatorio(campo))
}

/// O valor de um parâmetro, sem espaços nas pontas. Vazio conta como ausente.
fn opcional<'a>(p: &'a [(String, String)], nome: &str) -> Option<&'a str> {
    p.iter()
        .find(|(k, _)| k == nome)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

/// Os pares `chave=valor` da consulta, já decodificados.
#[must_use]
pub fn parametros(consulta: &str) -> Vec<(String, String)> {
    consulta
        .split('&')
        .filter(|par| !par.is_empty())
        .map(|par| {
            let (k, v) = par.split_once('=').unwrap_or((par, ""));
            (decodificar(k), decodificar(v))
        })
        .collect()
}

/// Desfaz o `+` (espaço) e os `%XX` da URL. `%` sem dois dígitos hexadecimais passa como veio.
#[must_use]
pub fn decodificar(texto: &str) -> String {
    let b = texto.as_bytes();
    let mut saida = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'+' {
            saida.push(b' ');
            i += 1;
        } else if b[i] == b'%'
            && i + 2 < b.len()
            && b[i + 1].is_ascii_hexdigit()
            && b[i + 2].is_ascii_hexdigit()
        {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("0");
            saida.push(u8::from_str_radix(hex, 16).unwrap_or(b'%'));
            i += 3;
        } else {
            saida.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&saida).into_owned()
}

#[cfg(test)]
mod testes {
    use super::*;

    const CHAVE: &str = "chave=123.456.789-09&nome=Loja+Exemplo&cidade=Sao+Paulo";

    fn corpo(r: &Resposta) -> &str {
        &r.corpo
    }

    fn json(r: &Resposta) -> &str {
        assert_eq!(r.tipo, TIPO_JSON);
        &r.corpo
    }

    #[test]
    fn a_raiz_entrega_a_tela() {
        let r = rota("/", "", None, false);
        assert_eq!(r.status, 200);
        assert_eq!(r.tipo, HTML);
        assert!(corpo(&r).contains("<title>"));
    }

    #[test]
    fn config_diz_so_se_ha_chave() {
        assert_eq!(
            rota("/api/config", "", None, true).corpo,
            "{\"chave\":true}"
        );
        assert_eq!(
            rota("/api/config", "", None, false).corpo,
            "{\"chave\":false}"
        );
    }

    #[test]
    fn gerar_devolve_codigo_qr_e_tipo() {
        let r = rota("/api/gerar", &format!("{CHAVE}&valor=10%2C50"), None, false);
        assert_eq!(r.status, 200);
        assert_ne!(json(&r), "");
        assert!(corpo(&r).starts_with("{\"codigo\":\"000201"));
        assert!(corpo(&r).contains("\"tipo_chave\":\"CPF\""));
        assert!(corpo(&r).contains("\"qr_svg\":\"<svg "));
        assert!(corpo(&r).contains("\"valor\":\"10.50 BRL\""));
    }

    #[test]
    fn gerar_com_campo_faltando_da_400_com_a_mensagem() {
        let r = rota("/api/gerar", "nome=Ana&cidade=Recife", None, false);
        assert_eq!(r.status, 400);
        assert!(corpo(&r).contains("chave Pix é obrigatório."));
        let r = rota(
            "/api/gerar",
            "chave=123.456.789-09&nome=&cidade=Recife",
            None,
            false,
        );
        assert!(corpo(&r).contains("nome do recebedor é obrigatório."));
        let r = rota("/api/gerar", "chave=123.456.789-09&nome=Ana", None, false);
        assert!(corpo(&r).contains("cidade é obrigatório."));
    }

    #[test]
    fn gerar_com_valor_txid_descricao_e_erro_de_cada_um() {
        let base = CHAVE;
        let r = rota("/api/gerar", &format!("{base}&valor=abc"), None, false);
        assert_eq!(r.status, 400);
        assert!(corpo(&r).contains("valor inválido"));
        let r = rota("/api/gerar", &format!("{base}&txid=a%20b"), None, false);
        assert!(corpo(&r).contains("txid inválido"));
        let r = rota(
            "/api/gerar",
            &format!("{base}&txid=PED1&descricao=Pedido+1"),
            None,
            false,
        );
        assert_eq!(r.status, 200);
        assert!(corpo(&r).contains("\"txid\":\"PED1\""));
        assert!(corpo(&r).contains("\"descricao\":\"Pedido 1\""));
        let longa = "d".repeat(80);
        let r = rota(
            "/api/gerar",
            &format!("{base}&descricao={longa}"),
            None,
            false,
        );
        assert!(corpo(&r).contains("descrição passa de 72"));
    }

    #[test]
    fn gerar_com_chave_ruim_da_400() {
        let r = rota(
            "/api/gerar",
            "chave=abc&nome=Ana&cidade=Recife",
            None,
            false,
        );
        assert_eq!(r.status, 400);
        assert!(corpo(&r).contains("chave Pix inválida"));
    }

    #[test]
    fn ler_devolve_os_campos_do_copia_e_cola() {
        let codigo = {
            let r = rota("/api/gerar", CHAVE, None, false);
            let inicio = r.corpo.find("\"codigo\":\"").unwrap() + 10;
            let fim = r.corpo[inicio..].find('"').unwrap() + inicio;
            r.corpo[inicio..fim].to_string()
        };
        let r = rota(
            "/api/ler",
            &format!("codigo={}", codigo.replace(' ', "+")),
            None,
            false,
        );
        assert_eq!(r.status, 200, "{}", r.corpo);
        assert!(corpo(&r).contains("\"chave\":\"12345678909\""));
        assert!(corpo(&r).contains("\"nome\":\"Loja Exemplo\""));
        assert!(corpo(&r).contains("\"valor\":null"));
        assert!(corpo(&r).contains("\"campos\":[{\"id\":\"00\""));
    }

    #[test]
    fn ler_codigo_ruim_da_400_e_sem_codigo_tambem() {
        let r = rota("/api/ler", "codigo=000201", None, false);
        assert_eq!(r.status, 400);
        assert!(corpo(&r).contains("não li o código"));
        let r = rota("/api/ler", "", None, false);
        assert_eq!(r.status, 400);
        assert!(corpo(&r).contains("código Pix é obrigatório."));
    }

    #[test]
    fn rota_que_nao_existe_da_404_em_json() {
        let r = rota("/nao_existe", "", None, false);
        assert_eq!(r.status, 404);
        assert_eq!(
            json(&r),
            "{\"erro\":{\"linha\":null,\"mensagem\":\"Rota não encontrada.\"}}"
        );
    }

    #[test]
    fn jsonw_pelo_accept_ou_pelo_parametro() {
        let r = rota("/api/config", "", Some("text/jsonw"), true);
        assert_eq!(r.tipo, "text/jsonw; charset=utf-8");
        assert_eq!(r.corpo, "{chave:true}");
        let r = rota("/api/config", "formato=jsonw", None, false);
        assert_eq!(r.corpo, "{chave:false}");
        let r = rota("/api/config", "formato=json", Some("text/jsonw"), true);
        assert_eq!(r.corpo, "{\"chave\":true}");
    }

    #[test]
    fn gerar_em_jsonw_tira_as_aspas_e_tipa_o_valor() {
        let r = rota(
            "/api/gerar",
            &format!("{CHAVE}&valor=10%2C50"),
            Some("text/jsonw"),
            false,
        );
        assert_eq!(r.status, 200);
        assert!(r.corpo.starts_with("{codigo:\"000201"), "{}", r.corpo);
        assert!(r.corpo.contains("valor:10.50 BRL"), "{}", r.corpo);
        assert!(r.corpo.contains("tipo_chave:\"CPF\""));
    }

    #[test]
    fn erro_em_jsonw_segue_o_formato_pedido() {
        let r = rota("/api/gerar", "nome=Ana", Some("text/jsonw"), false);
        assert_eq!(r.status, 400);
        assert_eq!(r.tipo, "text/jsonw; charset=utf-8");
        assert!(
            r.corpo.contains("mensagem:\"chave Pix é obrigatório.\""),
            "{}",
            r.corpo
        );
    }

    #[test]
    fn decodifica_mais_e_percentual() {
        assert_eq!(decodificar("a+b%20c"), "a b c");
        assert_eq!(decodificar("%41%42"), "AB");
        assert_eq!(decodificar("%2F"), "/");
        assert_eq!(decodificar("%+1"), "% 1");
        assert_eq!(decodificar("abc%"), "abc%");
        assert_eq!(decodificar("%4"), "%4");
        assert_eq!(decodificar("%zz"), "%zz");
        assert_eq!(decodificar("%é1"), "%é1");
        assert_eq!(decodificar("%ff"), "\u{fffd}");
    }

    #[test]
    fn parametros_aceita_chave_sem_valor_e_vazio() {
        assert_eq!(
            parametros("a=1&&b&c=x%3Dy"),
            vec![
                ("a".to_string(), "1".to_string()),
                ("b".to_string(), String::new()),
                ("c".to_string(), "x=y".to_string()),
            ]
        );
        assert_eq!(parametros(""), Vec::new());
    }
}
