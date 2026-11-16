//! O arquivo de configuração do `wortic`, em jsonW-texto: uma seção `comum` e uma por ferramenta, cada uma com as
//! variáveis daquela ferramenta (`PORTA`, `ENDERECO`, `WORTIC_API_KEY`, `CHATBOT_ROTEIRO`...). O arquivo é conferido
//! contra o esquema pela libjsonw: seção que não existe ou valor que não é texto, número ou booleano para com a razão.
//! A variável que já está no ambiente vence o arquivo.

use std::path::Path;

use serde_json::Value; // borda-json: o serde lê o JSON que a libjsonw devolveu do jsonW-texto

/// Esquema do arquivo.
pub const ESQUEMA: &str = "comum:mapa<texto,qualquer>?,cnpj:mapa<texto,qualquer>?,chatbot:mapa<texto,qualquer>?,\
pix:mapa<texto,qualquer>?,rastreio:mapa<texto,qualquer>?";

/// O arquivo de exemplo (`wortic config`).
pub const EXEMPLO: &str = "\
// wortic.jsw.txt: ajustes das ferramentas Wortic, em jsonW-texto.
// Uso: wortic --config wortic.jsw.txt <ferramenta>   (ou WORTIC_CONFIG=wortic.jsw.txt)
// Cada seção tem as variáveis da ferramenta; a variável que já está no ambiente vence o arquivo.
// Chave de API e token: prefira o ambiente ou um arquivo com permissão 600.
{
  comum: {ENDERECO: \"127.0.0.1\"},
  cnpj: {PORTA: 8081, CADASTRO_CACHE_MAX: 5000},
  chatbot: {PORTA: 8082, CHATBOT_DADOS: \"dados\", CHATBOT_ROTEIRO: \"roteiro.jsw.txt\"},
  pix: {PORTA: 8083},
  rastreio: {PORTA: 8084},
}
";

/// Lê o arquivo e põe no ambiente as variáveis da seção `comum` e da ferramenta (a da ferramenta vence a comum;
/// o ambiente vence as duas). Chamado no começo do `main`, antes de qualquer thread.
///
/// # Errors
/// Arquivo que não existe, fora do esquema, ou valor que não vira variável (lista, objeto).
pub fn aplicar(caminho: &Path, ferramenta: &str) -> Result<(), String> {
    let pares = variaveis(caminho, ferramenta)?;
    for (nome, valor) in pares {
        if std::env::var_os(&nome).is_none() {
            // SAFETY: chamado no começo do main, com uma thread só (nenhuma outra lê o ambiente ao mesmo tempo).
            unsafe { std::env::set_var(nome, valor) };
        }
    }
    Ok(())
}

/// As variáveis que o arquivo dá para a ferramenta, já com a seção da ferramenta por cima da comum.
///
/// # Errors
/// Como em [`aplicar`].
pub fn variaveis(caminho: &Path, ferramenta: &str) -> Result<Vec<(String, String)>, String> {
    let json = wortic_base::config::ler(caminho, ESQUEMA)?
        .ok_or_else(|| format!("{}: o arquivo não existe", caminho.display()))?;
    let raiz: Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let mut pares: Vec<(String, String)> = Vec::new();
    for secao in ["comum", ferramenta] {
        let Some(mapa) = raiz.get(secao).and_then(Value::as_object) else {
            continue;
        };
        for (nome, valor) in mapa {
            let texto = match valor {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => {
                    return Err(format!(
                        "{}: {secao}.{nome} precisa ser texto, número ou booleano",
                        caminho.display()
                    ));
                }
            };
            pares.retain(|(n, _)| n != nome);
            pares.push((nome.clone(), texto));
        }
    }
    Ok(pares)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn arquivo(nome: &str, conteudo: &str) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("wortic-cfg-{nome}-{}.jsw.txt", std::process::id()));
        std::fs::write(&p, conteudo).unwrap();
        p
    }

    #[test]
    fn exemplo_casa_com_o_esquema() {
        let p = arquivo("exemplo", EXEMPLO);
        let v = variaveis(&p, "chatbot").unwrap();
        assert!(v.contains(&("ENDERECO".into(), "127.0.0.1".into())));
        assert!(v.contains(&("PORTA".into(), "8082".into())));
        assert!(v.contains(&("CHATBOT_ROTEIRO".into(), "roteiro.jsw.txt".into())));
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn secao_da_ferramenta_vence_a_comum() {
        let p = arquivo("vence", "{comum:{PORTA:1},pix:{PORTA:2}}");
        assert_eq!(
            variaveis(&p, "pix").unwrap(),
            vec![("PORTA".to_string(), "2".to_string())]
        );
        assert_eq!(
            variaveis(&p, "cnpj").unwrap(),
            vec![("PORTA".to_string(), "1".to_string())]
        );
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn secao_desconhecida_e_valor_composto_param() {
        let p = arquivo("ruim", "{outra:{A:1}}");
        assert!(variaveis(&p, "pix").is_err());
        let p2 = arquivo("lista", "{pix:{A:[1,2]}}");
        assert!(variaveis(&p2, "pix").unwrap_err().contains("pix.A"));
        let _ = std::fs::remove_file(p);
        let _ = std::fs::remove_file(p2);
    }

    #[test]
    fn arquivo_que_nao_existe_diz_o_caminho() {
        let erro = variaveis(Path::new("/nao/existe.jsw.txt"), "pix").unwrap_err();
        assert!(erro.contains("/nao/existe.jsw.txt"));
    }
}
