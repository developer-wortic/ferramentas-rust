//! Migração do jeito antigo (TOML) para o jsonW-texto. Roda sozinha na partida, uma vez, e à mão com
//! `wortic chatbot migrar`. O arquivo antigo não se apaga: vai para `<nome>.toml.antes`. Este é o único uso do crate
//! `toml` no chatbot.

use std::fs;
use std::path::{Path, PathBuf};

use crate::configuracao::{self, Arquivo};
use crate::roteiro::Roteiro;

/// Roteiro: se `caminho` (`roteiro.jsw.txt`) não existe e há um `roteiro.toml` ao lado, converte e devolve o caminho
/// novo. Se o `caminho` é o `.toml` antigo, converte para `.jsw.txt` e devolve esse. Sem nada a fazer, devolve o próprio
/// caminho e nenhum aviso. Os avisos vão para o log da partida.
///
/// # Errors
///
/// O TOML antigo não lê ou não passa na validação do roteiro (a mensagem diz o arquivo e o motivo).
pub fn roteiro(caminho: &Path) -> Result<(PathBuf, Vec<String>), String> {
    let nome = caminho.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if let Some(base) = nome.strip_suffix(".jsw.txt") {
        let antigo = caminho.with_file_name(format!("{base}.toml"));
        if caminho.exists() || !antigo.is_file() {
            return Ok((caminho.to_path_buf(), Vec::new()));
        }
        let aviso = converter_roteiro(&antigo, caminho)?;
        return Ok((caminho.to_path_buf(), vec![aviso]));
    }
    if let Some(base) = nome.strip_suffix(".toml") {
        if !caminho.is_file() {
            return Ok((caminho.to_path_buf(), Vec::new()));
        }
        let novo = caminho.with_file_name(format!("{base}.jsw.txt"));
        if novo.exists() {
            let aviso = format!(
                "AVISO: {} ficou de lado, porque {} já existe. Use {}.",
                caminho.display(),
                novo.display(),
                novo.display()
            );
            return Ok((novo, vec![aviso]));
        }
        let aviso = converter_roteiro(caminho, &novo)?;
        return Ok((novo, vec![aviso]));
    }
    Ok((caminho.to_path_buf(), Vec::new()))
}

/// Configuração: se `dados/configuracao.jsw.txt` não existe e há `dados/configuracao.toml`, converte (com 600) e
/// devolve os avisos. A chave não aparece em aviso nem em erro.
///
/// # Errors
///
/// O TOML antigo não lê, ou o arquivo novo não pôde ser gravado.
pub fn configuracao(dados: &Path) -> Result<Vec<String>, String> {
    let novo = dados.join(configuracao::ARQUIVO);
    let antigo = dados.join("configuracao.toml");
    if novo.exists() || !antigo.is_file() {
        return Ok(Vec::new());
    }
    let texto = fs::read_to_string(&antigo)
        .map_err(|e| format!("não consegui ler {}: {e}", antigo.display()))?;
    let arquivo: Arquivo = toml::from_str(&texto)
        .map_err(|e| format!("{} não é TOML válido: {e}", antigo.display()))?;
    configuracao::gravar_arquivo(&novo, &arquivo)?;
    guardar_antigo(&antigo)?;
    Ok(vec![format!(
        "migrei {} para {} (permissão 600). O original ficou em {}.antes.",
        antigo.display(),
        novo.display(),
        antigo.display()
    )])
}

/// Converte as duas peças de uma pasta do projeto: `roteiro.toml` e `dados/configuracao.toml`. É o `wortic chatbot
/// migrar [pasta]`. Devolve os avisos (vazio quando não havia nada antigo).
///
/// # Errors
///
/// Como em [`roteiro`] e [`configuracao`].
pub fn pasta(raiz: &Path) -> Result<Vec<String>, String> {
    let (_, mut avisos) = roteiro(&raiz.join("roteiro.jsw.txt"))?;
    avisos.extend(configuracao(&raiz.join("dados"))?);
    Ok(avisos)
}

/// Lê o TOML do roteiro, valida, grava o jsonW e tira o antigo do caminho. Devolve o aviso.
fn converter_roteiro(antigo: &Path, novo: &Path) -> Result<String, String> {
    let texto = fs::read_to_string(antigo)
        .map_err(|e| format!("não consegui ler {}: {e}", antigo.display()))?;
    let valor: serde_json::Value =
        toml::from_str(&texto) // borda-json: TOML antigo vira o mesmo registro que o roteiro lê
            .map_err(|e| format!("{} não é TOML válido: {e}", antigo.display()))?;
    let roteiro =
        Roteiro::de_json(&valor.to_string()).map_err(|e| format!("{}: {e}", antigo.display()))?;
    let nome = antigo
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let texto_novo = format!(
        "// Convertido de {nome} na partida (formato antigo, TOML). O original ficou em {nome}.antes.\n{}\n",
        roteiro.para_jsonw()
    );
    wortic_base::config::gravar(novo, &texto_novo, false)
        .map_err(|e| format!("não consegui gravar {}: {e}", novo.display()))?;
    guardar_antigo(antigo)?;
    Ok(format!(
        "migrei {} para {} (formato jsonW). O original ficou em {}.antes.",
        antigo.display(),
        novo.display(),
        antigo.display()
    ))
}

/// Renomeia `<nome>.toml` para `<nome>.toml.antes`. Não apaga nada.
fn guardar_antigo(antigo: &Path) -> Result<(), String> {
    let nome = antigo.file_name().unwrap_or_default().to_string_lossy();
    let destino = antigo.with_file_name(format!("{nome}.antes"));
    fs::rename(antigo, &destino).map_err(|e| {
        format!(
            "não consegui guardar {} como {}: {e}",
            antigo.display(),
            destino.display()
        )
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn sem_arquivo_antigo_nao_faz_nada() {
        let caminho = std::env::temp_dir().join("chatbot-mig-nada.jsw.txt");
        let (usado, avisos) = roteiro(&caminho).unwrap();
        assert_eq!(usado, caminho);
        assert_eq!(avisos, Vec::<String>::new());
    }
}
