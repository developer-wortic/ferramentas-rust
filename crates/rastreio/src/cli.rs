//! `wortic rastreio`: a linha de comando do rastreio. Sem argumento, sobe o servidor (painel + pixel).

use std::process::ExitCode;

use crate::modelo::{Ajustes, executar};

const AJUDA: &str = "\
wortic rastreio: rastreio de UTM e cliques + Conversions API da Meta

uso: wortic rastreio [servir]

  servir   (padrão) sobe o painel, o pixel e o /evento. As variáveis estão no README (seção \"Ajustes\").
";

/// Ponto de entrada do subcomando.
#[must_use]
pub fn principal(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None | Some("servir") => match executar(&Ajustes::do_ambiente()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(erro) => {
                eprintln!("{erro}");
                ExitCode::from(1)
            }
        },
        Some("ajuda" | "--help" | "-h") => {
            print!("{AJUDA}");
            ExitCode::SUCCESS
        }
        Some(outro) => {
            eprintln!("wortic rastreio: comando desconhecido: {outro}\n");
            eprint!("{AJUDA}");
            ExitCode::from(2)
        }
    }
}
