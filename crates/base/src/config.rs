//! Configuração em jsonW-texto: um arquivo `.jsw.txt` com um registro, conferido contra o esquema da ferramenta.
//! Comentário `//`, vírgula sobrando e chave sem aspas valem; tipo errado ou campo obrigatório faltando para com a
//! razão. Gravar é troca atômica (arquivo temporário ao lado, depois `rename`), com permissão 600 quando pedido.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::texto;

/// Lê o arquivo e devolve o registro em JSON (para o serde). `Ok(None)` quando o arquivo não existe.
///
/// # Errors
/// Arquivo ilegível ou fora do esquema, com o caminho e a razão.
pub fn ler(caminho: &Path, esquema: &str) -> Result<Option<String>, String> {
    match fs::read_to_string(caminho) {
        Ok(conteudo) => texto::para_json(esquema, &conteudo)
            .map(Some)
            .map_err(|razao| format!("{}: {razao}", caminho.display())),
        Err(erro) if erro.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(erro) => Err(format!("não consegui ler {}: {erro}", caminho.display())),
    }
}

/// Confere um texto contra o esquema sem arquivo (o que o painel manda antes de gravar).
///
/// # Errors
/// A razão da libjsonw.
pub fn conferir(esquema: &str, conteudo: &str) -> Result<String, String> {
    texto::para_json(esquema, conteudo)
}

/// Grava o conteúdo com troca atômica. `privado` fecha a permissão em 600 (arquivo com chave).
///
/// # Errors
/// A pasta não deixa escrever, ou o `rename` falhou.
pub fn gravar(caminho: &Path, conteudo: &str, privado: bool) -> io::Result<()> {
    let pasta = caminho.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(pasta) = pasta {
        fs::create_dir_all(pasta)?;
    }
    let mut nome = caminho.file_name().unwrap_or_default().to_os_string();
    nome.push(format!(".{}.tmp", std::process::id()));
    let temporario = caminho.with_file_name(nome);
    {
        let mut arquivo = abrir_novo(&temporario, privado)?;
        arquivo.write_all(conteudo.as_bytes())?;
        arquivo.sync_all()?;
    }
    fs::rename(&temporario, caminho).inspect_err(|_| {
        let _ = fs::remove_file(&temporario);
    })
}

#[cfg(unix)]
fn abrir_novo(caminho: &Path, privado: bool) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(if privado { 0o600 } else { 0o644 })
        .open(caminho)
}

#[cfg(not(unix))]
fn abrir_novo(caminho: &Path, _privado: bool) -> io::Result<fs::File> {
    fs::File::create(caminho)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pasta(nome: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("wortic-base-{nome}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn grava_atomico_e_le_de_volta() {
        let p = pasta("ida");
        let arquivo = p.join("c.jsw.txt");
        gravar(&arquivo, "// teste\n{provedor:\"openai\",modelo:\"x\",}\n", true).unwrap();
        let json = ler(&arquivo, "provedor:texto?,modelo:texto?,chave:texto?").unwrap();
        assert_eq!(
            json.as_deref(),
            Some(r#"{"provedor":"openai","modelo":"x","chave":null}"#)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let modo = fs::metadata(&arquivo).unwrap().permissions().mode() & 0o777;
            assert_eq!(modo, 0o600);
        }
        assert_eq!(fs::read_dir(&p).unwrap().count(), 1, "sem temporário sobrando");
        let _ = fs::remove_dir_all(&p);
    }

    #[test]
    fn arquivo_que_nao_existe_e_none() {
        assert_eq!(ler(&pasta("nada").join("x.jsw.txt"), "a:i64").unwrap(), None);
    }

    #[test]
    fn fora_do_esquema_diz_o_caminho() {
        let p = pasta("ruim");
        let arquivo = p.join("r.jsw.txt");
        gravar(&arquivo, "{a:\"x\"}", false).unwrap();
        let erro = ler(&arquivo, "a:i64").unwrap_err();
        assert!(erro.contains("r.jsw.txt"), "{erro}");
        let _ = fs::remove_dir_all(&p);
    }
}
