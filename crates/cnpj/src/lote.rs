//! `wortic cnpj lote`: uma ficha por CNPJ, num arquivo coluna `.jsw` (esquema, custódia e leitura pela libjsonw). A
//! entrada é um registro jsonW por linha, `{cnpj:"..."}`. Linha que falha (dígito errado, erro da API, limite) não
//! derruba o lote: vai com o motivo na coluna `erro`.

use std::path::Path;

use serde_json::{Map, Value, json}; // borda-json: API cnpj.wortic.com (a ficha chega em JSON; o .jsw sai em jsonW)
use wortic_base::{arquivo, texto};

use crate::{api, cnpj};

/// O esquema da ficha no `.jsw`. Os campos principais são tipados; o resto da ficha da API vai na coluna `resto`.
/// - `cnpj`: código exato, 14 caracteres (o alfanumérico também), sem pontuação;
/// - `uf` e `situacao`: dicionário (`dic`);
/// - `capital_social`: dinheiro em reais (`BRL`);
/// - `abertura`: texto `AAAA-MM-DD`, como a Receita dá. O `tempo` do jsonW é um instante em UTC, e uma data sem hora
///   nele mudaria de dia conforme o fuso;
/// - `resto`: qualquer JSON, a ficha inteira menos o que já está nas colunas tipadas;
/// - `erro`: o motivo da falha da linha, ou nulo quando a ficha veio.
pub const ESQUEMA_FICHA: &str = "cnpj:codigo,razao_social:texto?,uf:dic?,situacao:dic?,capital_social:dinheiro?,abertura:texto?,resto:qualquer?,erro:texto?";

/// Campos da ficha que viram colunas tipadas (saem do `resto`).
const TIPADOS: [&str; 4] = ["cnpj", "razao_social", "capital_social", "abertura"];
const MSG_DIGITOS: &str = "CNPJ inválido: confira os dígitos.";
const MSG_INVALIDA: &str = "A API respondeu algo que não consegui ler.";

/// Quantas linhas o lote consultou e quantas falharam (as que falharam também estão no arquivo, com o motivo).
pub struct Resumo {
    pub total: usize,
    pub com_erro: usize,
}

/// Os CNPJs da entrada, já limpos (sem pontuação, em maiúsculas). Linha em branco e comentário `//` não contam.
///
/// # Errors
/// Uma linha que não é `{cnpj:"..."}` (ou que tem o CNPJ vazio). A entrada inteira é recusada antes de consultar.
pub fn ler_entrada(entrada: &str) -> Result<Vec<String>, String> {
    let mut cnpjs = Vec::new();
    for (n, linha) in entrada.lines().enumerate() {
        let linha = linha.trim();
        if linha.is_empty() || linha.starts_with("//") {
            continue;
        }
        let registro = texto::para_json("cnpj:texto", linha)
            .map_err(|razao| format!("linha {}: {razao}", n + 1))?;
        // borda-json: API cnpj.wortic.com (o registro vem da libjsonw já conferido, e só o CNPJ é lido dele)
        let valor: Value =
            serde_json::from_str(&registro).map_err(|erro| format!("linha {}: {erro}", n + 1))?;
        let Some(bruto) = valor.get("cnpj").and_then(Value::as_str) else {
            return Err(format!("linha {}: falta o campo cnpj", n + 1));
        };
        let limpo = cnpj::limpar(bruto);
        if limpo.is_empty() {
            return Err(format!("linha {}: CNPJ vazio", n + 1));
        }
        cnpjs.push(limpo);
    }
    Ok(cnpjs)
}

/// Roda o lote: consulta cada CNPJ pela `api` e grava uma linha por CNPJ no arquivo `saida`.
///
/// # Errors
/// A entrada está fora do formato (nada é consultado), ou o arquivo não pôde ser gravado (o destino fica como estava).
pub fn executar(api: &api::Api, entrada: &str, saida: &Path) -> Result<Resumo, String> {
    let cnpjs = ler_entrada(entrada)?;
    let linhas: Vec<Value> = cnpjs.iter().map(|c| consultar(c, api)).collect();
    let registros: Vec<String> = linhas.iter().map(Value::to_string).collect();
    arquivo::gravar(saida, ESQUEMA_FICHA, registros.iter().map(String::as_str))?;
    Ok(Resumo {
        total: linhas.len(),
        com_erro: linhas.iter().filter(|l| !l["erro"].is_null()).count(),
    })
}

/// A linha de um CNPJ: a ficha, ou a falha com o motivo. Nunca interrompe o lote.
fn consultar(cnpj: &str, api: &api::Api) -> Value {
    if !cnpj::valido(cnpj) {
        return falha(cnpj, MSG_DIGITOS);
    }
    let r = api.buscar(&format!("/cnpj/{cnpj}"));
    if r.status != 200 {
        return falha(cnpj, &api::descrever_erro(&r));
    }
    // borda-json: API cnpj.wortic.com (a ficha da API entra aqui)
    match serde_json::from_str::<Value>(&r.corpo) {
        Ok(Value::Object(ficha)) => ficha_ok(cnpj, &ficha),
        _ => falha(cnpj, MSG_INVALIDA),
    }
}

/// A ficha da API nas colunas do esquema. Campo que a API não manda sai nulo, nunca inventado.
fn ficha_ok(cnpj: &str, ficha: &Map<String, Value>) -> Value {
    let razao = ficha.get("razao_social").and_then(Value::as_str);
    let uf = ficha
        .get("endereco")
        .and_then(|e| e.get("uf"))
        .and_then(Value::as_str);
    let situacao = ficha
        .get("situacao")
        .and_then(|s| s.get("descricao"))
        .and_then(Value::as_str);
    let abertura = ficha.get("abertura").and_then(Value::as_str);
    // o capital vem como número (ou como texto com ponto); reais com duas casas, sem ponto flutuante na coluna
    let capital = ficha
        .get("capital_social")
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
        .map(|reais| format!("{reais:.2} BRL"));
    let mut resto = ficha.clone();
    for chave in TIPADOS {
        resto.remove(chave);
    }
    json!({
        "cnpj": cnpj,
        "razao_social": razao,
        "uf": uf,
        "situacao": situacao,
        "capital_social": capital,
        "abertura": abertura,
        "resto": resto,
        "erro": null,
    })
}

/// A linha de um CNPJ que falhou: as colunas da ficha vazias e o motivo na coluna `erro`.
fn falha(cnpj: &str, motivo: &str) -> Value {
    json!({
        "cnpj": cnpj,
        "razao_social": null,
        "uf": null,
        "situacao": null,
        "capital_social": null,
        "abertura": null,
        "resto": null,
        "erro": motivo,
    })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::suporte::falso;
    use std::time::Duration;

    const FICHA: &str = r#"{"cnpj":"37853407000171","razao_social":"EMPRESA DE TESTE LTDA","endereco":{"uf":"SC","cidade":"Cidade Teste"},"situacao":{"descricao":"Ativa","desde":"2020-07-24"},"capital_social":1234.5,"abertura":"2020-07-24","telefone":"0000-0000"}"#;

    fn api_para(base: &str) -> api::Api {
        api::Api::com(base, None, Duration::from_secs(5), Duration::from_secs(60))
    }

    fn pasta(nome: &str) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("cadastro-cnpj-lote-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn entrada_aceita_registros_e_pula_branco_e_comentario() {
        let entrada =
            "// cabeçalho\n\n{cnpj:\"37.853.407/0001-71\"}\n  {cnpj:\"37853407000172\"}  \n";
        assert_eq!(
            ler_entrada(entrada).unwrap(),
            vec!["37853407000171".to_string(), "37853407000172".to_string()]
        );
    }

    #[test]
    fn entrada_fora_do_formato_recusa_com_o_numero_da_linha() {
        let erro = ler_entrada("{cnpj:\"37853407000171\"}\n{cnpj:123}\n").unwrap_err();
        assert!(erro.starts_with("linha 2:"), "{erro}");
    }

    #[test]
    fn entrada_sem_campo_cnpj_ou_com_cnpj_vazio_recusa() {
        assert!(
            ler_entrada("{nome:\"x\"}")
                .unwrap_err()
                .starts_with("linha 1:")
        );
        let vazio = ler_entrada("{cnpj:\"  .-/ \"}").unwrap_err();
        assert!(vazio.contains("CNPJ vazio"), "{vazio}");
    }

    #[test]
    fn ficha_da_api_vira_as_colunas_e_o_resto() {
        let ficha: Map<String, Value> = serde_json::from_str(FICHA).unwrap();
        let linha = ficha_ok("37853407000171", &ficha);
        assert_eq!(linha["razao_social"], "EMPRESA DE TESTE LTDA");
        assert_eq!(linha["uf"], "SC");
        assert_eq!(linha["situacao"], "Ativa");
        assert_eq!(linha["capital_social"], "1234.50 BRL");
        assert_eq!(linha["abertura"], "2020-07-24");
        assert!(linha["erro"].is_null());
        // o resto leva o que não virou coluna, e tira o que virou
        let resto = linha["resto"].as_object().unwrap();
        assert!(resto.contains_key("endereco"));
        assert!(resto.contains_key("telefone"));
        assert!(!resto.contains_key("razao_social"));
        assert!(!resto.contains_key("capital_social"));
    }

    #[test]
    fn capital_que_nao_e_numero_fica_nulo_e_o_texto_com_ponto_passa() {
        let com_texto: Map<String, Value> =
            serde_json::from_str(r#"{"capital_social":"20000.5"}"#).unwrap();
        assert_eq!(ficha_ok("1", &com_texto)["capital_social"], "20000.50 BRL");
        let ruim: Map<String, Value> =
            serde_json::from_str(r#"{"capital_social":"vinte mil"}"#).unwrap();
        assert!(ficha_ok("1", &ruim)["capital_social"].is_null());
        let vazia: Map<String, Value> = serde_json::from_str("{}").unwrap();
        let linha = ficha_ok("1", &vazia);
        assert!(linha["razao_social"].is_null());
        assert!(linha["uf"].is_null());
    }

    #[test]
    fn cnpj_com_digito_errado_nao_consulta_e_vai_com_o_motivo() {
        let f = falso(|_| (200, FICHA.as_bytes().to_vec(), vec![]));
        let linha = consultar("37853407000172", &api_para(&f.base));
        assert_eq!(linha["erro"], MSG_DIGITOS);
        assert!(linha["resto"].is_null());
        assert!(f.pedidos.lock().unwrap().is_empty());
    }

    #[test]
    fn erro_da_api_vai_na_linha_com_a_mensagem_dela() {
        let f = falso(|_| (404, b"{\"erro\":\"nao achei\"}".to_vec(), vec![]));
        let linha = consultar("37853407000171", &api_para(&f.base));
        assert_eq!(linha["erro"], "A API respondeu com erro 404.");
        assert_eq!(linha["cnpj"], "37853407000171");
    }

    #[test]
    fn resposta_da_api_que_nao_e_ficha_vai_como_erro() {
        let f = falso(|_| (200, b"[1,2]".to_vec(), vec![]));
        let linha = consultar("37853407000171", &api_para(&f.base));
        assert_eq!(linha["erro"], MSG_INVALIDA);
    }

    #[test]
    fn lote_grava_o_jsw_e_a_libjsonw_le_de_volta() {
        let f = falso(|caminho| {
            if caminho == "/cnpj/37853407000171" {
                (200, FICHA.as_bytes().to_vec(), vec![])
            } else {
                (404, b"{\"erro\":\"nao achei\"}".to_vec(), vec![])
            }
        });
        let pasta = pasta("ida");
        let destino = pasta.join("fichas.jsw");
        let entrada =
            "{cnpj:\"37853407000171\"}\n{cnpj:\"37853407000172\"}\n{cnpj:\"11444777000161\"}\n";
        let resumo = executar(&api_para(&f.base), entrada, &destino).unwrap();
        assert_eq!(resumo.total, 3);
        assert_eq!(resumo.com_erro, 2);
        let linhas = arquivo::ler(&destino, true).unwrap();
        assert_eq!(linhas.len(), 3);
        assert!(
            linhas[0]
                .starts_with("{cnpj:\"37853407000171\",razao_social:\"EMPRESA DE TESTE LTDA\"")
        );
        assert!(
            linhas[0].contains("capital_social:1234.50 BRL"),
            "{}",
            linhas[0]
        );
        assert!(linhas[1].contains("erro:\"CNPJ inválido"), "{}", linhas[1]);
        assert!(
            linhas[2].contains("erro:\"A API respondeu com erro 404.\""),
            "{}",
            linhas[2]
        );
        // a mesma custódia lê em JSON, com as três linhas
        assert_eq!(arquivo::ler(&destino, false).unwrap().len(), 3);
        let _ = std::fs::remove_dir_all(&pasta);
    }

    #[test]
    fn entrada_ruim_nao_grava_nada_nem_consulta() {
        let f = falso(|_| (200, FICHA.as_bytes().to_vec(), vec![]));
        let pasta = pasta("ruim");
        let destino = pasta.join("fichas.jsw");
        assert!(executar(&api_para(&f.base), "{cnpj:123", &destino).is_err());
        assert!(!destino.exists());
        assert!(f.pedidos.lock().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&pasta);
    }
}
