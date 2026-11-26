//! A ponte com a base jsonW. Dentro do programa o dado vai em jsonW: a entrada é conferida pela libjsonw
//! (`arquivo::texto` valida contra o esquema e devolve o texto canônico), os campos são lidos desse texto e a saída
//! é escrita em jsonW. JSON só sai na borda, com [`como_json`], para quem chama de fora ou pede `--json`.

use std::fmt::Write;

use wortic_base::{Formato, arquivo, texto as base_texto};

/// Um valor lido de um registro canônico: nulo, texto (entre aspas) ou o token cru (número, dinheiro, tempo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Valor {
    /// `null`.
    Nulo,
    /// Um texto, já sem aspas nem escapes.
    Texto(String),
    /// Um token que não é texto: `8080`, `12.50 BRL`, `t"2026-11-16T10:00:00Z"`.
    Cru(String),
}

impl Valor {
    /// O texto do valor, quando há (texto ou token cru). Nulo não tem texto.
    #[must_use]
    pub fn como_texto(&self) -> Option<&str> {
        match self {
            Valor::Nulo => None,
            Valor::Texto(s) | Valor::Cru(s) => Some(s),
        }
    }
}

/// A razão da libjsonw sem o trecho do texto que ela mostra (`perto de "..."`, `em "..."`). O trecho pode ser um
/// segredo (`RASTREIO_SEGREDO`) ou um dado do cliente, então nunca sai.
#[must_use]
pub fn sem_trecho(razao: &str) -> String {
    let corte = [" perto de", " em \""]
        .iter()
        .filter_map(|marca| razao.find(marca))
        .min();
    match corte {
        Some(i) => razao[..i].trim_end().to_string(),
        None => razao.to_string(),
    }
}

/// Confere um registro jsonW (ou JSON, que também é jsonW) contra o esquema, pela libjsonw. Devolve o texto canônico
/// (uma linha, sem a quebra final).
///
/// # Errors
/// O registro não casa com o esquema, com a razão sem trecho do texto.
pub fn conferir(esquema: &str, registro: &str) -> Result<String, String> {
    let linhas = wortic_base::canonico::canonico(esquema, registro).map_err(|r| sem_trecho(&r))?;
    Ok(linhas.trim_end_matches('\n').to_string())
}

/// Lê um registro canônico (`{chave:valor,...}`, sem aninhamento) em pares. Campo aninhado é recusado: as entradas
/// da ferramenta são planas.
///
/// # Errors
/// O texto não está na forma canônica de um registro plano.
pub fn campos(canonico: &str) -> Result<Vec<(String, Valor)>, String> {
    let corpo = canonico
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| "o registro não é um objeto jsonW".to_string())?;
    let mut campos = Vec::new();
    let mut resto = corpo.trim_start();
    while !resto.is_empty() {
        let (chave, depois) = ler_chave(resto)?;
        let depois = depois
            .trim_start()
            .strip_prefix(':')
            .ok_or_else(|| "falta ':' depois de uma chave".to_string())?
            .trim_start();
        let (valor, depois) = ler_valor(depois)?;
        campos.push((chave, valor));
        let depois = depois.trim_start();
        resto = if depois.is_empty() {
            depois
        } else {
            depois
                .strip_prefix(',')
                .ok_or_else(|| "falta ',' entre os campos".to_string())?
                .trim_start()
        };
    }
    Ok(campos)
}

/// Os pares `(chave, texto)` de um registro lido: texto e token cru viram o próprio texto; nulo some.
#[must_use]
pub fn pares(campos: &[(String, Valor)]) -> Vec<(String, String)> {
    campos
        .iter()
        .filter_map(|(chave, valor)| Some((chave.clone(), valor.como_texto()?.to_string())))
        .collect()
}

/// Confere e lê um registro de uma vez: os pares já prontos para as regras de negócio.
///
/// # Errors
/// Como em [`conferir`] e [`campos`].
pub fn ler_registro(esquema: &str, registro: &str) -> Result<Vec<(String, String)>, String> {
    let canonico = conferir(esquema, registro)?;
    Ok(pares(&campos(&canonico)?))
}

/// Um registro jsonW já conferido em JSON, para quem chama de fora ou pede `--json`. A borda.
/// A libjsonw põe nulo em todo opcional que falta; na borda o campo sem valor some, como no JSON que a Meta recebe.
///
/// # Errors
/// O registro não casa com o esquema (não deve acontecer para um registro que a ferramenta montou).
pub fn como_json(esquema: &str, registro: &str) -> Result<String, String> {
    let json = base_texto::para_json(esquema, registro).map_err(|r| sem_trecho(&r))?; // borda-json: API de fora ou saída --json
    let mut valor: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?; // borda-json: API de fora ou saída --json
    sem_nulos(&mut valor);
    Ok(valor.to_string()) // borda-json: API de fora ou saída --json
}

/// Tira do JSON as chaves de valor nulo, em todos os níveis.
fn sem_nulos(valor: &mut serde_json::Value) {
    // borda-json: API de fora ou saída --json
    match valor {
        serde_json::Value::Object(mapa) => {
            // borda-json: API de fora ou saída --json
            mapa.retain(|_, v| !v.is_null());
            mapa.values_mut().for_each(sem_nulos);
        }
        serde_json::Value::Array(itens) => itens.iter_mut().for_each(sem_nulos), // borda-json: API de fora ou saída --json
        _ => {}
    }
}

/// Uma lista de registros jsonW no formato pedido, com o tipo da resposta e o corpo.
pub struct Corpo {
    /// O `content-type` da resposta.
    pub tipo: &'static str,
    /// O corpo em bytes.
    pub bytes: Vec<u8>,
}

/// Uma lista de registros (cada um em jsonW-texto, já montado) no formato pedido: JSON (um array, na borda),
/// jsonW-texto (um registro por linha, com os tipos canônicos) ou o arquivo coluna `.jsw`.
///
/// # Errors
/// Algum registro não casa com o esquema, com a razão sem trecho.
pub fn lista(formato: Formato, esquema: &str, linhas: &[String]) -> Result<Corpo, String> {
    let bytes = match formato {
        Formato::Json => {
            // borda-json: lista em JSON para quem chama de fora
            let mut itens = Vec::with_capacity(linhas.len());
            for linha in linhas {
                itens.push(como_json(esquema, linha)?);
            }
            format!("[{}]", itens.join(",")).into_bytes() // borda-json: lista em JSON para quem chama de fora
        }
        Formato::Texto => arquivo::texto(esquema, linhas.iter().map(String::as_str))
            .map_err(|r| sem_trecho(&r))?
            .into_bytes(),
        Formato::Arquivo => arquivo::bytes(esquema, linhas.iter().map(String::as_str))
            .map_err(|r| sem_trecho(&r))?,
    };
    Ok(Corpo {
        tipo: formato.tipo(),
        bytes,
    })
}

/// Um texto em jsonW: entre aspas, com as mesmas escapes do JSON (válidas nos dois).
#[must_use]
pub fn texto(valor: &str) -> String {
    let mut saida = String::with_capacity(valor.len() + 2);
    saida.push('"');
    for c in valor.chars() {
        match c {
            '"' => saida.push_str("\\\""),
            '\\' => saida.push_str("\\\\"),
            '\n' => saida.push_str("\\n"),
            '\r' => saida.push_str("\\r"),
            '\t' => saida.push_str("\\t"),
            c if c < ' ' => {
                let _ = write!(saida, "\\u{:04x}", u32::from(c));
            }
            outro => saida.push(outro),
        }
    }
    saida.push('"');
    saida
}

/// Um texto opcional: o texto entre aspas, ou `null`.
#[must_use]
pub fn opcional(valor: Option<&str>) -> String {
    valor.map_or_else(|| "null".to_string(), texto)
}

/// Um objeto jsonW a partir de campos já escritos em jsonW (`chave: valor`). As chaves são identificadores fixos.
#[must_use]
pub fn obj(campos: &[(&str, String)]) -> String {
    let dentro: Vec<String> = campos.iter().map(|(k, v)| format!("{k}:{v}")).collect();
    format!("{{{}}}", dentro.join(","))
}

/// Uma lista jsonW a partir de itens já escritos.
#[must_use]
pub fn lista_de(itens: &[String]) -> String {
    format!("[{}]", itens.join(","))
}

/// O instante de `quando_ms` (milissegundos desde a época) como `tempo`: microssegundos UTC, número cru.
#[must_use]
pub fn tempo(quando_ms: u64) -> String {
    quando_ms.saturating_mul(1000).to_string()
}

/// Um dinheiro na moeda do evento: `"12.50 BRL"` (escala 2).
#[must_use]
pub fn dinheiro(centavos: u32, moeda: &str) -> String {
    texto(&format!("{}.{:02} {moeda}", centavos / 100, centavos % 100))
}

/// Uma chave de campo: texto entre aspas, ou um identificador (letra, dígito ou `_`).
fn ler_chave(s: &str) -> Result<(String, &str), String> {
    if s.starts_with('"') {
        return ler_texto(s);
    }
    let fim = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    if fim == 0 {
        return Err("chave vazia".to_string());
    }
    Ok((s[..fim].to_string(), &s[fim..]))
}

/// Um valor plano: texto, `null`, ou um token cru até a vírgula (número, dinheiro, `t"..."`).
fn ler_valor(s: &str) -> Result<(Valor, &str), String> {
    if s.starts_with('"') {
        let (texto, resto) = ler_texto(s)?;
        return Ok((Valor::Texto(texto), resto));
    }
    if s.starts_with('[') || s.starts_with('{') {
        return Err("valor aninhado: as entradas da ferramenta são planas".to_string());
    }
    let fim = match s.strip_prefix("t\"") {
        Some(resto) => resto
            .find('"')
            .map(|i| i + 3)
            .ok_or_else(|| "tempo sem a aspa de fechamento".to_string())?,
        None => s.find(',').unwrap_or(s.len()),
    };
    let token = s[..fim].trim();
    let resto = &s[fim..];
    let valor = if token == "null" {
        Valor::Nulo
    } else if token.is_empty() {
        return Err("valor vazio".to_string());
    } else {
        Valor::Cru(token.to_string())
    };
    Ok((valor, resto))
}

/// Um texto entre aspas, com as escapes do JSON. Devolve o texto e o que sobrou depois da aspa de fechamento.
fn ler_texto(s: &str) -> Result<(String, &str), String> {
    let mut saida = String::new();
    let mut it = s[1..].char_indices();
    while let Some((i, c)) = it.next() {
        match c {
            '"' => return Ok((saida, &s[i + 2..])),
            '\\' => {
                let (_, e) = it.next().ok_or_else(|| "escape cortado".to_string())?;
                match e {
                    '"' => saida.push('"'),
                    '\\' => saida.push('\\'),
                    '/' => saida.push('/'),
                    'b' => saida.push('\u{8}'),
                    'f' => saida.push('\u{c}'),
                    'n' => saida.push('\n'),
                    'r' => saida.push('\r'),
                    't' => saida.push('\t'),
                    'u' => saida.push(ler_unicode(&mut it)?),
                    _ => return Err("escape desconhecido".to_string()),
                }
            }
            outro => saida.push(outro),
        }
    }
    Err("texto sem a aspa de fechamento".to_string())
}

/// `\uXXXX`, com par substituto (`😀`) quando a escape é um substituto alto.
fn ler_unicode(it: &mut std::str::CharIndices<'_>) -> Result<char, String> {
    let alto = quatro_hex(it)?;
    let codigo = if (0xD800..0xDC00).contains(&alto) {
        let (_, barra) = it
            .next()
            .ok_or_else(|| "par substituto cortado".to_string())?;
        let (_, u) = it
            .next()
            .ok_or_else(|| "par substituto cortado".to_string())?;
        if barra != '\\' || u != 'u' {
            return Err("par substituto sem a segunda metade".to_string());
        }
        let baixo = quatro_hex(it)?;
        0x10000 + ((alto - 0xD800) << 10) + (baixo - 0xDC00)
    } else {
        alto
    };
    char::from_u32(codigo).ok_or_else(|| "escape \\u fora de caractere".to_string())
}

/// Quatro dígitos hexadecimais depois de um `\u`.
fn quatro_hex(it: &mut std::str::CharIndices<'_>) -> Result<u32, String> {
    let hex: String = (0..4)
        .map(|_| it.next().map(|(_, h)| h))
        .collect::<Option<String>>()
        .ok_or_else(|| "escape \\u cortada".to_string())?;
    u32::from_str_radix(&hex, 16).map_err(|_| "escape \\u inválida".to_string())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn campos_leem_texto_nulo_cru_e_tempo() {
        let campos = campos(
            r#"{evento:"Purchase",pedido:null,valor:99.90 BRL,momento:1700000000,quando:t"2026-11-16T10:00:00.000000Z"}"#,
        )
        .expect("campos");
        assert_eq!(
            campos[0],
            ("evento".to_string(), Valor::Texto("Purchase".to_string()))
        );
        assert_eq!(campos[1].1, Valor::Nulo);
        assert_eq!(campos[2].1, Valor::Cru("99.90 BRL".to_string()));
        assert_eq!(campos[3].1, Valor::Cru("1700000000".to_string()));
        assert_eq!(
            campos[4].1,
            Valor::Cru("t\"2026-11-16T10:00:00.000000Z\"".to_string())
        );
    }

    #[test]
    fn texto_com_escapes_volta_igual_nos_dois_sentidos() {
        let original = "aspas \" barra \\ quebra\nfim \u{1}";
        let escrito = texto(original);
        let lido = campos(&format!("{{a:{escrito}}}")).expect("campos");
        assert_eq!(lido[0].1, Valor::Texto(original.to_string()));
    }

    #[test]
    fn escape_unicode_com_par_substituto_le_o_emoji() {
        let lido = campos(r#"{a:"😀 ok"}"#).expect("campos");
        assert_eq!(lido[0].1, Valor::Texto("😀 ok".to_string()));
    }

    #[test]
    fn valor_aninhado_e_recusado_na_leitura_plana() {
        assert!(campos("{a:{b:1}}").is_err());
        assert!(campos("{a:[1]}").is_err());
    }

    #[test]
    fn sem_trecho_corta_o_pedaco_do_texto_e_fica_a_razao() {
        assert_eq!(
            sem_trecho("numero num campo que nao e numero perto de \"}segredo{RAST\""),
            "numero num campo que nao e numero"
        );
        assert_eq!(
            sem_trecho("esperava , ou fim em \"+pessoal?,v\""),
            "esperava , ou fim"
        );
        assert_eq!(sem_trecho("sem trecho nenhum"), "sem trecho nenhum");
    }

    #[test]
    fn como_json_da_borda_nao_leva_nulo() {
        let json = como_json(
            crate::esquemas::RESPOSTA,
            r#"{resultado:"enviado",tentativas:2,recebidos:null}"#,
        )
        .expect("json");
        assert_eq!(json, r#"{"resultado":"enviado","tentativas":2}"#);
    }

    #[test]
    fn pares_tiram_o_nulo_e_mantem_o_texto() {
        let c = campos(r#"{a:"x",b:null,c:7}"#).expect("campos");
        assert_eq!(
            pares(&c),
            vec![
                ("a".to_string(), "x".to_string()),
                ("c".to_string(), "7".to_string())
            ]
        );
    }
}
