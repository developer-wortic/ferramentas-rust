//! `wortic chatbot`: a linha de comando do chatbot. Sem argumento, sobe o servidor (painel e widget). Os outros
//! comandos trabalham com o roteiro, a migração do TOML antigo e o arquivo `.jsw` de leads, tudo em jsonW-texto.

use std::path::Path;
use std::process::ExitCode;

use crate::migracao;
use crate::roteiro::{self, Roteiro};

const AJUDA: &str = concat!(
    "wortic chatbot: chatbot de prospecção com a sua chave (BYOK)\n",
    "\n",
    "uso: wortic chatbot [comando]\n",
    "\n",
    "  servir                       (padrão) sobe o painel, o widget e a API. Variáveis: PORTA (8080), ENDERECO (127.0.0.1),\n",
    "                               CHATBOT_PROVEDOR, CHATBOT_MODELO, CHATBOT_CHAVE, CHATBOT_BASE_URL, CHATBOT_ORIGENS,\n",
    "                               CHATBOT_DADOS, CHATBOT_ROTEIRO\n",
    "  roteiro exemplo              imprime o roteiro de exemplo em jsonW-texto (para copiar e editar)\n",
    "  roteiro conferir <arquivo>   confere um roteiro: sem erro sai com 0; com erro, diz a razão e sai com 1\n",
    "  migrar [pasta]               converte o roteiro.toml e o dados/configuracao.toml antigos para jsonW-texto\n",
    "                               (o original fica em <nome>.toml.antes; padrão: pasta atual)\n",
    "  leads <arquivo.jsw>          lê um export de leads (.jsw) e imprime em jsonW-texto\n",
    "  leads <arquivo.jsw> --json   o mesmo, em JSON, para quem consome de fora\n", // borda-json: API de fora, quem pede --json
    "  ajuda                        esta mensagem\n",
);

/// Ponto de entrada do subcomando.
pub fn principal(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None | Some("servir") => crate::servidor::principal(std::env::vars()),
        Some("ajuda" | "--help" | "-h") => {
            print!("{AJUDA}");
            ExitCode::SUCCESS
        }
        Some("roteiro") => roteiro_cmd(&args[1..]),
        Some("migrar") => migrar_cmd(&args[1..]),
        Some("leads") => leads_cmd(&args[1..]),
        Some(outro) => uso_errado(&format!("comando desconhecido: {outro}")),
    }
}

/// Mensagem de uso e código de saída 2 (erro de uso, não de conteúdo).
fn uso_errado(motivo: &str) -> ExitCode {
    eprintln!("wortic chatbot: {motivo}\n");
    eprint!("{AJUDA}");
    ExitCode::from(2)
}

fn roteiro_cmd(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("exemplo") => {
            print!("{}", roteiro::EXEMPLO);
            ExitCode::SUCCESS
        }
        Some("conferir") => match args.get(1) {
            Some(arquivo) => conferir_roteiro(Path::new(arquivo)),
            None => uso_errado("roteiro conferir precisa do arquivo"),
        },
        _ => uso_errado("roteiro aceita exemplo ou conferir <arquivo>"),
    }
}

/// Confere o arquivo pelo mesmo caminho que o servidor usa. Erro vai para o stderr com saída 1.
fn conferir_roteiro(arquivo: &Path) -> ExitCode {
    let texto = match std::fs::read_to_string(arquivo) {
        Ok(texto) => texto,
        Err(erro) => {
            eprintln!("não consegui ler {}: {erro}", arquivo.display());
            return ExitCode::from(1);
        }
    };
    match Roteiro::ler(&texto) {
        Ok(roteiro) => {
            println!(
                "ok: {} confere ({} perguntas, empresa {})",
                arquivo.display(),
                roteiro.perguntas().len(),
                roteiro.empresa.nome.trim()
            );
            ExitCode::SUCCESS
        }
        Err(razao) => {
            eprintln!("{}: {razao}", arquivo.display());
            ExitCode::from(1)
        }
    }
}

fn migrar_cmd(args: &[String]) -> ExitCode {
    let raiz = Path::new(args.first().map_or(".", String::as_str));
    match migracao::pasta(raiz) {
        Ok(avisos) if avisos.is_empty() => {
            println!("nada para migrar em {}", raiz.display());
            ExitCode::SUCCESS
        }
        Ok(avisos) => {
            for aviso in avisos {
                println!("{aviso}");
            }
            ExitCode::SUCCESS
        }
        Err(erro) => {
            eprintln!("{erro}");
            ExitCode::from(1)
        }
    }
}

fn leads_cmd(args: &[String]) -> ExitCode {
    let em_json = args.iter().any(|a| a == "--json"); // borda-json: --json é a borda para quem consome de fora
    let Some(arquivo) = args.iter().find(|a| !a.starts_with("--")) else {
        return uso_errado("leads precisa do arquivo .jsw");
    };
    match wortic_base::arquivo::ler(Path::new(arquivo), !em_json) {
        Ok(linhas) => {
            for linha in linhas {
                println!("{linha}");
            }
            ExitCode::SUCCESS
        }
        Err(erro) => {
            eprintln!("{arquivo}: {erro}");
            ExitCode::from(1)
        }
    }
}
