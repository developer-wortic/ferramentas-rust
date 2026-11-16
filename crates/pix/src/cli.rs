//! `wortic pix`: a linha de comando do Pix. Sem argumento, sobe o servidor (tela + API).

use std::process::ExitCode;

const AJUDA: &str = "\
wortic pix: Pix estático (BR Code e QR)

uso: wortic pix [servir]

  servir   (padrão) sobe a tela e a API. Variáveis: PORTA (8080), ENDERECO (127.0.0.1), WORTIC_API_KEY (opcional)
";

/// Ponto de entrada do subcomando.
#[must_use]
pub fn principal(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None | Some("servir") => {
            let codigo = crate::servidor::executar(&|nome| std::env::var(nome).ok());
            ExitCode::from(u8::try_from(codigo).unwrap_or(1))
        }
        Some("ajuda" | "--help" | "-h") => {
            print!("{AJUDA}");
            ExitCode::SUCCESS
        }
        Some(outro) => {
            eprintln!("wortic pix: comando desconhecido: {outro}\n");
            eprint!("{AJUDA}");
            ExitCode::from(2)
        }
    }
}
