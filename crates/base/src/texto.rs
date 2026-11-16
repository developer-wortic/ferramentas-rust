//! jsonW-texto: a forma para gente ler e escrever. Todo JSON válido já é jsonW-texto válido; o canônico tira as aspas
//! das chaves, aceita comentário `//`, vírgula sobrando, número com vírgula (`12,50d`), dinheiro (`12,50 BRL`), tempo
//! (`t"2026-11-16T10:00:00Z"`) e medida (`3,5 km`).
//!
//! Ler passa pela libjsonw oficial ([`para_json`]), que confere o texto contra o esquema. Escrever ([`de_json`]) é
//! uma troca de forma sem perda: o valor continua o mesmo, só as chaves perdem as aspas.

use std::ffi::{CString, c_char};

unsafe extern "C" {
    // jsonw.h: texto -> JSON, validado pelo esquema. Saída curta devolve -2; erro, -1 com a razão.
    fn jsonw_texto_para_json(
        esquema: *const c_char,
        texto: *const c_char,
        n: usize,
        saida: *mut c_char,
        cap: usize,
        erro: *mut c_char,
        nerro: usize,
    ) -> i64;
}

const TAMANHO_ERRO: usize = 320;

/// Lê um registro em jsonW-texto, confere contra o esquema e devolve o mesmo registro em JSON (para o serde).
///
/// # Errors
/// O texto não casa com o esquema (tipo errado, campo obrigatório faltando, sintaxe), com a razão da libjsonw.
pub fn para_json(esquema: &str, texto: &str) -> Result<String, String> {
    let esquema = CString::new(esquema).map_err(|_| "esquema com byte nulo".to_string())?;
    let mut cap = (texto.len() * 2).max(4096);
    loop {
        let mut saida = vec![0u8; cap];
        let mut erro = [0u8; TAMANHO_ERRO];
        // SAFETY: os ponteiros apontam para buffers vivos com os tamanhos passados; a libjsonw não guarda nenhum.
        let n = unsafe {
            jsonw_texto_para_json(
                esquema.as_ptr(),
                texto.as_ptr().cast(),
                texto.len(),
                saida.as_mut_ptr().cast(),
                cap,
                erro.as_mut_ptr().cast(),
                TAMANHO_ERRO,
            )
        };
        match n {
            -2 => cap *= 4,
            ..0 => {
                let fim = erro.iter().position(|&b| b == 0).unwrap_or(erro.len());
                let razao = String::from_utf8_lossy(&erro[..fim]).into_owned();
                return Err(if razao.is_empty() {
                    "jsonW-texto inválido".to_string()
                } else {
                    razao
                });
            }
            n => {
                saida.truncate(usize::try_from(n).unwrap_or(0));
                return String::from_utf8(saida).map_err(|_| "saída fora de UTF-8".to_string());
            }
        }
    }
}

/// Escreve um JSON como jsonW-texto canônico de forma: chave que é identificador perde as aspas. Valores, ordem e
/// espaços ficam como vieram. Entrada que não é JSON bem formado volta igual (quem chama já validou).
#[must_use]
pub fn de_json(json: &str) -> String {
    let b = json.as_bytes();
    let mut saida = Vec::with_capacity(b.len());
    // true = dentro de objeto; a chave vem depois de `{` ou de `,` num objeto
    let mut pilha: Vec<bool> = Vec::new();
    let mut espera_chave = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            let ini = i;
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            let fim = (i + 1).min(b.len());
            let dentro = &json[ini + 1..fim.saturating_sub(1).max(ini + 1)];
            if espera_chave && identificador(dentro) {
                saida.extend_from_slice(dentro.as_bytes());
            } else {
                saida.extend_from_slice(&b[ini..fim]);
            }
            espera_chave = false;
            i = fim;
            continue;
        }
        match c {
            b'{' => {
                pilha.push(true);
                espera_chave = true;
            }
            b'[' => {
                pilha.push(false);
                espera_chave = false;
            }
            b'}' | b']' => {
                pilha.pop();
                espera_chave = false;
            }
            b',' => espera_chave = pilha.last() == Some(&true),
            b' ' | b'\n' | b'\r' | b'\t' => {}
            _ => espera_chave = false,
        }
        saida.push(c);
        i += 1;
    }
    // só tiramos aspas ASCII de volta de chaves ASCII: o resto dos bytes é o UTF-8 de entrada
    String::from_utf8(saida).unwrap_or_else(|_| json.to_string())
}

/// Chave que pode ir sem aspas: letra ou `_`, depois letras, dígitos ou `_`; nunca `true`, `false` ou `null`.
fn identificador(chave: &str) -> bool {
    let mut letras = chave.bytes();
    matches!(letras.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && letras.all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !matches!(chave, "true" | "false" | "null")
}

/// Uma lista de registros JSON (`[{..},{..}]` já separada) em jsonW-texto, um registro por linha.
#[must_use]
pub fn linhas_de_json<'a>(registros: impl IntoIterator<Item = &'a str>) -> String {
    let mut saida = String::new();
    for registro in registros {
        saida.push_str(&de_json(registro));
        saida.push('\n');
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn chave_perde_aspas_e_valor_fica() {
        assert_eq!(
            de_json(r#"{"nome":"Loja \"X\"","n":3,"lista":[{"a":1},"b"],"ok":true}"#),
            r#"{nome:"Loja \"X\"",n:3,lista:[{a:1},"b"],ok:true}"#
        );
    }

    #[test]
    fn chave_que_nao_e_identificador_mantem_aspas() {
        assert_eq!(
            de_json(r#"{"com espaço":1,"1a":2,"null":3,"ação":4}"#),
            r#"{"com espaço":1,"1a":2,"null":3,"ação":4}"#
        );
    }

    #[test]
    fn texto_que_parece_chave_dentro_de_lista_fica_com_aspas() {
        assert_eq!(de_json(r#"["a","b"]"#), r#"["a","b"]"#);
        assert_eq!(de_json(r#"{"a":["x",{"y":"z"}]}"#), r#"{a:["x",{y:"z"}]}"#);
    }

    #[test]
    fn ida_e_volta_pela_libjsonw() {
        let json = r#"{"empresa":{"nome":"Loja \"X\"","tom":"cordial"},"perguntas":["Quantas?","Quem decide?"],"nota":70}"#;
        let esquema = "empresa:obj{nome:texto,tom:texto},perguntas:lista<texto>,nota:i64";
        let texto = de_json(json);
        assert_eq!(para_json(esquema, &texto).unwrap(), json);
    }

    #[test]
    fn le_comentario_virgula_sobrando_e_dinheiro() {
        let texto = "// comentário\n{\n  nome: \"Loja\",\n  preco: 12,50 BRL,\n  perguntas: [\"a\", \"b\",],\n}\n";
        let json = para_json("nome:texto,preco:dinheiro,perguntas:lista<texto>", texto).unwrap();
        assert_eq!(json, r#"{"nome":"Loja","preco":"12.50 BRL","perguntas":["a","b"]}"#);
    }

    #[test]
    fn esquema_recusa_tipo_errado_com_a_razao() {
        let erro = para_json("nota:i64", "{nota:\"setenta\"}").unwrap_err();
        assert!(erro.contains("nao e texto") || !erro.is_empty(), "{erro}");
    }

    #[test]
    fn campo_opcional_ausente_vira_nulo() {
        assert_eq!(
            para_json("a:i64,b:texto?", "{a:1}").unwrap(),
            r#"{"a":1,"b":null}"#
        );
    }

    #[test]
    fn texto_grande_cresce_o_buffer() {
        let grande = "x".repeat(300_000);
        let json = para_json("t:texto", &format!("{{t:\"{grande}\"}}")).unwrap();
        assert_eq!(json.len(), grande.len() + 8);
    }
}
