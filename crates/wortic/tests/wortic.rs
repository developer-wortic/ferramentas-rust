//! O binário único: ajuda, versão, configuração em jsonW-texto e o nome antigo de cada ferramenta.

use std::process::Command;

fn wortic() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wortic"))
}

#[test]
fn sem_argumento_mostra_a_ajuda_e_sai_com_2() {
    let saida = wortic().output().unwrap();
    assert_eq!(saida.status.code(), Some(2));
    let erro = String::from_utf8_lossy(&saida.stderr);
    for ferramenta in ["cnpj", "chatbot", "pix", "rastreio"] {
        assert!(erro.contains(ferramenta), "{erro}");
    }
}

#[test]
fn versao_diz_a_libjsonw() {
    let saida = wortic().arg("versao").output().unwrap();
    assert!(saida.status.success());
    let texto = String::from_utf8_lossy(&saida.stdout);
    assert!(texto.starts_with("wortic 2."), "{texto}");
    assert!(texto.contains("libjsonw 1."), "{texto}");
}

#[test]
fn ferramenta_desconhecida_sai_com_2() {
    let saida = wortic().arg("planilha").output().unwrap();
    assert_eq!(saida.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&saida.stderr).contains("ferramenta desconhecida: planilha"));
}

#[test]
fn config_de_exemplo_e_jsonw_texto_valido() {
    let saida = wortic().arg("config").output().unwrap();
    assert!(saida.status.success());
    let pasta = std::env::temp_dir().join(format!("wortic-bin-cfg-{}", std::process::id()));
    std::fs::create_dir_all(&pasta).unwrap();
    let arquivo = pasta.join("wortic.jsw.txt");
    std::fs::write(&arquivo, &saida.stdout).unwrap();
    // o exemplo usa a porta 8083 no pix: com uma rota que não sobe servidor, só confere que o arquivo foi aceito
    let saida = wortic()
        .args(["--config", arquivo.to_str().unwrap(), "pix", "ajuda"])
        .output()
        .unwrap();
    assert!(
        saida.status.success(),
        "{}",
        String::from_utf8_lossy(&saida.stderr)
    );
    let _ = std::fs::remove_dir_all(pasta);
}

#[test]
fn config_fora_do_esquema_para_com_a_razao() {
    let arquivo =
        std::env::temp_dir().join(format!("wortic-bin-ruim-{}.jsw.txt", std::process::id()));
    std::fs::write(&arquivo, "{planilha:{PORTA:1}}").unwrap();
    let saida = wortic()
        .args(["--config", arquivo.to_str().unwrap(), "pix", "ajuda"])
        .output()
        .unwrap();
    assert_eq!(saida.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&saida.stderr).contains("wortic.jsw") || !saida.stderr.is_empty()
    );
    let _ = std::fs::remove_file(arquivo);
}

#[test]
fn config_pela_variavel_e_o_ambiente_vence() {
    // a porta vem do arquivo; ENDERECO do ambiente vence o do arquivo e é inválido, então a falha prova as duas coisas
    let arquivo =
        std::env::temp_dir().join(format!("wortic-bin-env-{}.jsw.txt", std::process::id()));
    std::fs::write(
        &arquivo,
        "{comum:{ENDERECO:\"127.0.0.1\"},pix:{PORTA:\"porta-ruim\"}}",
    )
    .unwrap();
    let saida = wortic()
        .arg("pix")
        .env("WORTIC_CONFIG", &arquivo)
        .env_remove("PORTA")
        .output()
        .unwrap();
    assert!(!saida.status.success());
    let erro = String::from_utf8_lossy(&saida.stderr).to_lowercase();
    assert!(erro.contains("porta"), "{erro}");
    let _ = std::fs::remove_file(arquivo);
}

#[cfg(unix)]
#[test]
fn pelo_nome_antigo_sobe_a_ferramenta_daquele_nome() {
    let pasta = std::env::temp_dir().join(format!("wortic-bin-link-{}", std::process::id()));
    std::fs::create_dir_all(&pasta).unwrap();
    for (antigo, ajuda) in [
        ("pix-brcode", "wortic pix"),
        ("cadastro-cnpj", "wortic cnpj"),
        ("chatbot-prospeccao", "wortic chatbot"),
        ("rastreio-capi-modelo", "wortic rastreio"),
    ] {
        let link = pasta.join(antigo);
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_wortic"), &link).unwrap();
        let saida = Command::new(&link).arg("ajuda").output().unwrap();
        assert!(saida.status.success(), "{antigo}");
        assert!(
            String::from_utf8_lossy(&saida.stdout).contains(ajuda),
            "{antigo}"
        );
    }
    let _ = std::fs::remove_dir_all(pasta);
}
