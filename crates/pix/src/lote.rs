//! O lote: cada linha de um arquivo jsonW-texto (um registro por linha) vira uma cobrança. Linha em branco e linha que
//! começa com `//` não contam, mas o número das linhas conta desde o começo do arquivo. Uma linha ruim para o lote
//! inteiro: quem chama não grava nada, e o erro diz a linha e a razão.

use wortic_base::canonico;

use crate::cobranca::Cobranca;
use crate::jsonw::{self, razao};
use crate::registro;

/// Monta o registro de cada cobrança do arquivo, na ordem em que aparecem (jsonW-texto, pronto para gravar).
///
/// # Errors
///
/// Devolve a linha (a partir de 1) e a razão da primeira linha que não passa. `None` na linha quer dizer que o
/// problema não é de uma linha só, como um arquivo sem nenhum registro.
pub fn montar(entrada: &str) -> Result<Vec<String>, (Option<usize>, String)> {
    let mut registros = Vec::new();
    for (i, linha) in entrada.lines().enumerate() {
        let t = linha.trim();
        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        let registro = uma(t).map_err(|razao| (Some(i + 1), razao))?;
        registros.push(registro);
    }
    if registros.is_empty() {
        return Err((None, "o arquivo de entrada não tem nenhum registro".into()));
    }
    Ok(registros)
}

/// Uma linha: a libjsonw confere e tipa pelo esquema da entrada; depois a cobrança sai com as mesmas regras de `gerar`.
fn uma(linha: &str) -> Result<String, String> {
    let tipada = canonico::canonico(jsonw::ESQUEMA_ENTRADA, linha).map_err(razao)?;
    let campos = campos(tipada.trim_end())?;
    let mut cobranca = Cobranca::nova(
        &obrigatorio(&campos, "chave")?,
        &obrigatorio(&campos, "nome")?,
        &obrigatorio(&campos, "cidade")?,
    )
    .map_err(|erro| erro.to_string())?;
    if let Some(valor) = opcional(&campos, "valor") {
        cobranca = cobranca
            .com_valor(&valor)
            .map_err(|erro| erro.to_string())?;
    }
    if let Some(txid) = opcional(&campos, "txid") {
        cobranca = cobranca.com_txid(&txid).map_err(|erro| erro.to_string())?;
    }
    if let Some(descricao) = opcional(&campos, "descricao") {
        cobranca = cobranca
            .com_descricao(&descricao)
            .map_err(|erro| erro.to_string())?;
    }
    registro::cobranca(&cobranca, None).map_err(|erro| erro.to_string())
}

type Campos = [(String, Option<String>)];

fn obrigatorio(campos: &Campos, nome: &str) -> Result<String, String> {
    opcional(campos, nome).ok_or_else(|| format!("falta o campo {nome}"))
}

/// Um campo com valor (`None` quando é `null` ou quando não existe na linha).
fn opcional(campos: &Campos, nome: &str) -> Option<String> {
    campos
        .iter()
        .find(|(k, _)| k == nome)
        .and_then(|(_, v)| v.clone())
        .filter(|v| !v.is_empty())
}

/// Os campos de uma linha já tipada pela libjsonw, na forma canônica: `{chave:"...",valor:10.50 BRL,txid:null}`.
/// Texto vem entre aspas (com escape), `null` é vazio e o resto é o valor como está. O dinheiro sai com o ` BRL` no
/// fim, que aqui é tirado, para o valor voltar como texto de reais ("10.50").
fn campos(linha: &str) -> Result<Vec<(String, Option<String>)>, String> {
    let interior = linha
        .strip_prefix('{')
        .and_then(|r| r.strip_suffix('}'))
        .ok_or("linha fora da forma jsonW")?;
    let mut saida = Vec::new();
    if interior.is_empty() {
        return Ok(saida);
    }
    let mut it = interior.chars().peekable();
    loop {
        let mut nome = String::new();
        while let Some(&c) = it.peek() {
            if c == ':' {
                break;
            }
            nome.push(c);
            it.next();
        }
        if it.next() != Some(':') {
            return Err("campo sem valor na linha".into());
        }
        let valor = if it.peek() == Some(&'"') {
            it.next();
            Some(texto_lido(&mut it)?)
        } else {
            let mut bruto = String::new();
            while let Some(&c) = it.peek() {
                if c == ',' {
                    break;
                }
                bruto.push(c);
                it.next();
            }
            if bruto == "null" {
                None
            } else {
                Some(bruto.strip_suffix(" BRL").unwrap_or(&bruto).to_string())
            }
        };
        saida.push((nome, valor));
        match it.next() {
            Some(',') => {}
            None => break,
            Some(_) => return Err("separador inesperado na linha".into()),
        }
    }
    Ok(saida)
}

/// Lê um texto até a aspa de fechamento, desfazendo o escape (`\"`, `\\`, `\n`, `\t`, `\r` e `\uXXXX`).
fn texto_lido(it: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Result<String, String> {
    let mut saida = String::new();
    loop {
        match it.next().ok_or("texto sem fechamento na linha")? {
            '"' => return Ok(saida),
            '\\' => match it.next().ok_or("escape pela metade")? {
                'n' => saida.push('\n'),
                't' => saida.push('\t'),
                'r' => saida.push('\r'),
                'u' => {
                    let hex: String = it.by_ref().take(4).collect();
                    let codigo =
                        u32::from_str_radix(&hex, 16).map_err(|_| "escape \\u inválido")?;
                    saida.push(char::from_u32(codigo).ok_or("escape \\u fora do alcance")?);
                }
                outro => saida.push(outro),
            },
            c => saida.push(c),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn monta_cada_linha_e_pula_comentario_e_vazia() {
        let entrada = "// lote de teste\n\n{chave:\"123.456.789-09\",nome:\"Ana\",cidade:\"Recife\",valor:10.50 BRL}\n{chave:\"contato@exemplo.com.br\",nome:\"Loja\",cidade:\"Recife\",txid:\"PED1\"}\n";
        let registros = montar(entrada).unwrap();
        assert_eq!(registros.len(), 2);
        assert!(registros[0].contains("valor:10.50 BRL"), "{}", registros[0]);
        assert!(registros[1].contains("txid:\"PED1\""), "{}", registros[1]);
        assert!(registros[1].contains("valor:null"));
    }

    #[test]
    fn a_linha_ruim_diz_qual_e_por_que() {
        let entrada = "{chave:\"123.456.789-09\",nome:\"Ana\",cidade:\"Recife\"}\n\n{chave:\"123.456.789-00\",nome:\"Ana\",cidade:\"Recife\"}\n";
        let (linha, razao) = montar(entrada).unwrap_err();
        assert_eq!(linha, Some(3));
        assert!(razao.contains("CPF inválido"), "{razao}");
    }

    #[test]
    fn linha_com_valor_zero_ou_sem_campo_obrigatorio_e_erro_da_linha() {
        let zero = "{chave:\"123.456.789-09\",nome:\"Ana\",cidade:\"Recife\",valor:0 BRL}";
        assert_eq!(montar(zero).unwrap_err().0, Some(1));
        let sem_nome = "{chave:\"123.456.789-09\",cidade:\"Recife\"}";
        assert_eq!(montar(sem_nome).unwrap_err().0, Some(1));
        let nome_vazio = "{chave:\"123.456.789-09\",nome:\" \",cidade:\"Recife\"}";
        let (_, razao) = montar(nome_vazio).unwrap_err();
        assert!(razao.contains("nome do recebedor"), "{razao}");
    }

    #[test]
    fn arquivo_sem_registro_nao_passa() {
        assert_eq!(
            montar("// só comentário\n\n"),
            Err((
                None,
                "o arquivo de entrada não tem nenhum registro".to_string()
            ))
        );
    }

    #[test]
    fn campos_le_a_forma_canonica_com_texto_nulo_e_escape() {
        let c = campos(r#"{chave:"a,b}c",nome:"Ana \"A\"",valor:10.50 BRL,txid:null}"#).unwrap();
        assert_eq!(c[0], ("chave".into(), Some("a,b}c".into())));
        assert_eq!(c[1], ("nome".into(), Some("Ana \"A\"".into())));
        assert_eq!(c[2], ("valor".into(), Some("10.50".into())));
        assert_eq!(c[3], ("txid".into(), None));
        assert_eq!(
            texto_lido(&mut "\\u00e7\"".chars().peekable()).unwrap(),
            "ç"
        );
        assert!(campos("sem chaves").is_err());
        assert_eq!(campos("{}").unwrap(), Vec::new());
    }
}
