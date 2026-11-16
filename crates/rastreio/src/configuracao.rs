//! A configuração do rastreio: as mesmas chaves das variáveis de ambiente, num arquivo jsonW-texto com esquema
//! (`RASTREIO_CONFIG=arquivo.jsw.txt`, ou `wortic rastreio --config arquivo.jsw.txt`). A variável de ambiente vence o
//! arquivo. Segredo nunca é impresso: a razão de erro passa pelo corte de trecho.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::esquemas;
use crate::jsonw;
use crate::modelo::{Ajustes, ajustes_de};

/// Os ajustes do processo: o arquivo (o de `--config` ou o de `RASTREIO_CONFIG`) por baixo, o ambiente por cima.
///
/// # Errors
/// O arquivo não existe, não pode ser lido, ou não casa com o esquema (com a razão, sem trecho do arquivo).
pub fn ajustes(arquivo: Option<&Path>) -> Result<Ajustes, String> {
    let caminho = arquivo
        .map(Path::to_path_buf)
        .or_else(|| variavel("RASTREIO_CONFIG").map(PathBuf::from));
    let do_arquivo = match caminho {
        Some(caminho) => ler(&caminho)?,
        None => BTreeMap::new(),
    };
    Ok(montar(&do_arquivo, variavel))
}

/// Monta os ajustes com um arquivo lido e uma leitura do ambiente. Variável vazia conta como ausente, e cai no arquivo.
#[must_use]
pub fn montar(
    do_arquivo: &BTreeMap<String, String>,
    ambiente: impl Fn(&str) -> Option<String>,
) -> Ajustes {
    ajustes_de(|chave| {
        ambiente(chave)
            .filter(|valor| !valor.trim().is_empty())
            .or_else(|| do_arquivo.get(chave).cloned())
    })
}

/// Lê o arquivo de configuração e devolve as chaves com os valores em texto.
///
/// # Errors
/// O arquivo não pode ser lido, ou não casa com o esquema `CONFIG`.
pub fn ler(caminho: &Path) -> Result<BTreeMap<String, String>, String> {
    let conteudo = fs::read_to_string(caminho).map_err(|erro| {
        format!(
            "não consegui ler a configuração {}: {erro}",
            caminho.display()
        )
    })?;
    let pares = jsonw::ler_registro(esquemas::CONFIG, &conteudo)
        .map_err(|razao| format!("configuração {}: {razao}", caminho.display()))?;
    Ok(pares.into_iter().collect())
}

fn variavel(chave: &str) -> Option<String> {
    std::env::var(chave).ok()
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::io::Write;

    fn pasta(nome: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rastreio-cfg-{nome}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("pasta");
        p
    }

    fn escrever(caminho: &Path, texto: &str) {
        let mut arquivo = fs::File::create(caminho).expect("arquivo");
        arquivo.write_all(texto.as_bytes()).expect("escreve");
    }

    #[test]
    fn arquivo_com_comentario_e_virgula_sobrando_vale() {
        let p = pasta("ok");
        let c = p.join("rastreio.jsw.txt");
        escrever(
            &c,
            "// configuração de teste\n{\n  PORTA: 9001,\n  RASTREIO_MEMORIA: 50,\n  RASTREIO_DDI: \"1\",\n}\n",
        );
        let do_arquivo = ler(&c).expect("lê");
        assert_eq!(do_arquivo.get("PORTA").map(String::as_str), Some("9001"));
        assert_eq!(
            do_arquivo.get("RASTREIO_DDI").map(String::as_str),
            Some("1")
        );
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn variavel_de_ambiente_vence_o_arquivo() {
        let do_arquivo: BTreeMap<String, String> = [
            ("PORTA".to_string(), "9001".to_string()),
            ("RASTREIO_DDI".to_string(), "1".to_string()),
        ]
        .into_iter()
        .collect();
        let ambiente: BTreeMap<&str, &str> = [("PORTA", "9002"), ("RASTREIO_DDI", "   ")]
            .into_iter()
            .collect();
        let a = montar(&do_arquivo, |c| ambiente.get(c).map(ToString::to_string));
        assert_eq!(a.porta, "9002", "ambiente vence");
        assert_eq!(a.ddi_padrao, "1", "variável vazia cai no arquivo");
    }

    #[test]
    fn chave_que_nao_existe_e_recusada_e_o_segredo_nao_sai() {
        let p = pasta("segredo");
        let c = p.join("ruim.jsw.txt");
        escrever(
            &c,
            "{RASTREIO_SEGREDO: 12345678901234567890123456789012345678, PORTA_X: 1}\n",
        );
        let erro = ler(&c).expect_err("erro");
        assert!(erro.contains("ruim.jsw.txt"), "{erro}");
        assert!(
            !erro.contains("12345678901234567890"),
            "o segredo não sai: {erro}"
        );
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn tipo_errado_no_segredo_nao_mostra_o_valor() {
        let p = pasta("tipo");
        let c = p.join("tipo.jsw.txt");
        escrever(
            &c,
            "{RASTREIO_ACCESS_TOKEN: 98765432109876543210, PORTA: \"x\"}\n",
        );
        let erro = ler(&c).expect_err("erro");
        assert!(!erro.contains("98765432109876543210"), "{erro}");
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn arquivo_que_nao_existe_diz_o_caminho() {
        let erro = ler(Path::new("/nao/existe/rastreio.jsw.txt")).expect_err("erro");
        assert!(erro.contains("rastreio.jsw.txt"), "{erro}");
    }
}
