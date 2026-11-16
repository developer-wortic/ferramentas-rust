//! O arquivo coluna `.jsw` pelo ABI oficial (jsonw.h): gravar uma lista de registros com esquema (troca atômica, só o `fechar`
//! publica) e ler de volta em JSON ou jsonW-texto. É o formato das listas que as ferramentas exportam (leads,
//! registros de rastreio) e o que a linha de comando lê e escreve.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Grava os registros (cada um em JSON ou jsonW-texto) num `.jsw` com o esquema.
///
/// # Errors
/// Registro fora do esquema (com a razão e a posição) ou erro de disco. Em erro, o destino fica como estava.
pub fn gravar<'a>(
    caminho: &Path,
    esquema: &str,
    registros: impl IntoIterator<Item = &'a str>,
) -> Result<u64, String> {
    let destino = caminho
        .to_str()
        .ok_or_else(|| "caminho fora de UTF-8".to_string())?;
    let mut escritor = crate::abi::Escritor::criar(destino, esquema)?;
    let mut n = 0u64;
    for registro in registros {
        if let Err(erro) = escritor.gravar(registro) {
            drop(escritor);
            return Err(format!("registro {n}: {erro}"));
        }
        n += 1;
    }
    escritor.fechar()?;
    Ok(n)
}

/// Os bytes de um `.jsw` com os registros (para mandar numa resposta `application/jsonw`).
///
/// # Errors
/// Como em [`gravar`].
pub fn bytes<'a>(esquema: &str, registros: impl IntoIterator<Item = &'a str>) -> Result<Vec<u8>, String> {
    let temporario = Temporario::novo();
    gravar(&temporario.0, esquema, registros)?;
    fs::read(&temporario.0).map_err(|e| e.to_string())
}

/// Os registros em jsonW-texto canônico, um por linha (tipos do esquema: `12.50 BRL`, `1.50d`, `t"..."`).
///
/// # Errors
/// Como em [`gravar`].
pub fn texto<'a>(esquema: &str, registros: impl IntoIterator<Item = &'a str>) -> Result<String, String> {
    let temporario = Temporario::novo();
    gravar(&temporario.0, esquema, registros)?;
    let mut saida = String::new();
    for linha in ler(&temporario.0, true)? {
        saida.push_str(&linha);
        saida.push('\n');
    }
    Ok(saida)
}

/// Lê todas as linhas, em jsonW-texto (`texto`) ou em JSON.
///
/// # Errors
/// Arquivo que não é jsonW ou corrompido (a custódia é conferida antes).
pub fn ler(caminho: &Path, em_texto: bool) -> Result<Vec<String>, String> {
    let origem = caminho
        .to_str()
        .ok_or_else(|| "caminho fora de UTF-8".to_string())?;
    let leitor = crate::abi::Leitor::abrir(origem)?;
    leitor.conferir()?;
    (0..leitor.linhas)
        .map(|i| if em_texto { leitor.texto(i) } else { leitor.json(i) })
        .collect::<Result<_, _>>()

}

/// Arquivo temporário que some ao sair do escopo.
struct Temporario(PathBuf);

impl Temporario {
    fn novo() -> Self {
        static SEQUENCIA: AtomicU64 = AtomicU64::new(0);
        let n = SEQUENCIA.fetch_add(1, Ordering::Relaxed);
        Self(std::env::temp_dir().join(format!(
            "wortic-{}-{n}.jsw",
            std::process::id()
        )))
    }
}

impl Drop for Temporario {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const ESQUEMA: &str = "nome:texto,valor:dinheiro,nota:i64?";

    #[test]
    fn grava_e_le_de_volta_em_json_e_em_texto() {
        let registros = [
            r#"{"nome":"Ana","valor":"12.50 BRL","nota":80}"#,
            "{nome:\"Bia\",valor:1.234,56 BRL}",
        ];
        let texto = texto(ESQUEMA, registros).unwrap();
        assert_eq!(
            texto,
            "{nome:\"Ana\",valor:12.50 BRL,nota:80}\n{nome:\"Bia\",valor:1234.56 BRL,nota:null}\n"
        );
    }

    #[test]
    fn bytes_comecam_pela_marca_do_formato() {
        let b = bytes(ESQUEMA, [r#"{"nome":"Ana","valor":"1 BRL"}"#]).unwrap();
        assert!(b.starts_with(b"JSW1"), "{:?}", &b[..8.min(b.len())]);
    }

    #[test]
    fn registro_fora_do_esquema_nao_publica() {
        let destino = std::env::temp_dir().join(format!("wortic-ruim-{}.jsw", std::process::id()));
        let erro = gravar(&destino, ESQUEMA, [r#"{"nome":1,"valor":"x"}"#]).unwrap_err();
        assert!(erro.starts_with("registro 0"), "{erro}");
        assert!(!destino.exists());
    }
}
