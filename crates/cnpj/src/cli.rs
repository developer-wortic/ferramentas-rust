//! `wortic cnpj`: a linha de comando do cadastro. Sem argumento, sobe o servidor (tela + API).

use std::process::ExitCode;

const AJUDA: &str = "\
wortic cnpj: cadastro de clientes que se preenche pelo CNPJ

uso: wortic cnpj [servir]

  servir   (padrão) sobe a tela e a API. Variáveis: PORTA (8080), ENDERECO (127.0.0.1),
           WORTIC_API_KEY (opcional), CADASTRO_CACHE_MAX (5000), CADASTRO_API_BASE (a API aberta)
";

/// Ponto de entrada do subcomando.
#[must_use]
pub fn principal(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None | Some("servir") => {
            crate::principal();
            ExitCode::SUCCESS
        }
        Some("ajuda" | "--help" | "-h") => {
            print!("{AJUDA}");
            ExitCode::SUCCESS
        }
        Some(outro) => {
            eprintln!("wortic cnpj: comando desconhecido: {outro}\n");
            eprint!("{AJUDA}");
            ExitCode::from(2)
        }
    }
}
