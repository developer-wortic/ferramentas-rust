//! `wortic chatbot`: a linha de comando do chatbot. Sem argumento, sobe o servidor (painel + widget).

use std::process::ExitCode;

const AJUDA: &str = "\
wortic chatbot: chatbot de prospecção com a sua chave (BYOK)

uso: wortic chatbot [servir]

  servir   (padrão) sobe o painel, o widget e a API. Variáveis: PORTA (8080), ENDERECO (127.0.0.1),
           CHATBOT_PROVEDOR, CHATBOT_MODELO, CHATBOT_CHAVE, CHATBOT_BASE_URL, CHATBOT_ORIGENS,
           CHATBOT_DADOS, CHATBOT_ROTEIRO
";

/// Ponto de entrada do subcomando.
#[must_use]
pub fn principal(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None | Some("servir") => crate::servidor::principal(std::env::vars()),
        Some("ajuda" | "--help" | "-h") => {
            print!("{AJUDA}");
            ExitCode::SUCCESS
        }
        Some(outro) => {
            eprintln!("wortic chatbot: comando desconhecido: {outro}\n");
            eprint!("{AJUDA}");
            ExitCode::from(2)
        }
    }
}
