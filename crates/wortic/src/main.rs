//! `wortic`: as ferramentas abertas da Wortic num binário só.
//!
//! ```text
//! wortic cnpj        cadastro de clientes que se preenche pelo CNPJ (tela + API)
//! wortic chatbot     chatbot de prospecção com a sua chave (painel + widget)
//! wortic pix         Pix estático: gera e lê o BR Code e desenha o QR (tela + API)
//! wortic rastreio    rastreio de UTM e cliques + Conversions API da Meta (painel + pixel)
//! wortic versao      versão do wortic e da libjsonw
//! ```
//!
//! Cada ferramenta lê as mesmas variáveis de antes (`PORTA`, `ENDERECO`, ...). Elas também podem vir de um arquivo de
//! configuração em jsonW-texto (`--config wortic.jsw.txt` ou `WORTIC_CONFIG`), com uma seção por ferramenta e uma
//! `comum`; a variável de ambiente sempre vence o arquivo.
//!
//! Chamado pelo nome antigo (um link `pix-brcode`, `cadastro-cnpj`, `chatbot-prospeccao` ou `rastreio-capi-modelo`
//! apontando para `wortic`), sobe a ferramenta daquele nome: quem usava os binários separados troca sem mudar nada.

mod configuracao;

use std::process::ExitCode;

const AJUDA: &str = "\
wortic: ferramentas abertas da Wortic (jsonW em tudo, JSON só na borda)

uso: wortic [--config arquivo.jsw.txt] <ferramenta> [argumentos]

  cnpj       cadastro de clientes que se preenche pelo CNPJ (tela + API)
  chatbot    chatbot de prospecção com a sua chave (painel + widget)
  pix        Pix estático: gera e lê o BR Code e desenha o QR (tela + API)
  rastreio   rastreio de UTM e cliques + Conversions API da Meta (painel + pixel)
  config     mostra o arquivo de configuração de exemplo (jsonW-texto)
  versao     versão do wortic e da libjsonw

Sem argumento, cada ferramenta sobe o servidor dela (PORTA, padrão 8080; ENDERECO, padrão 127.0.0.1).
As variáveis podem vir de --config (ou WORTIC_CONFIG); a do ambiente vence o arquivo.
`wortic <ferramenta> ajuda` mostra o resto.
";

/// O nome do binário (sem pasta e sem extensão).
fn nome_chamado(args: &[String]) -> String {
    args.first()
        .and_then(|a| std::path::Path::new(a).file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string()
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().collect();
    // pelo nome antigo do binário, a ferramenta é o próprio nome
    let pelo_nome = match nome_chamado(&args).as_str() {
        "cadastro-cnpj" => Some("cnpj"),
        "chatbot-prospeccao" => Some("chatbot"),
        "pix-brcode" => Some("pix"),
        "rastreio-capi-modelo" | "rastreio-capi" => Some("rastreio"),
        _ => None,
    };
    let mut arquivo_config = std::env::var("WORTIC_CONFIG")
        .ok()
        .filter(|c| !c.is_empty());
    if pelo_nome.is_none() && args.get(1).map(String::as_str) == Some("--config") {
        let Some(caminho) = args.get(2).cloned() else {
            eprintln!("wortic: --config precisa do caminho do arquivo");
            return ExitCode::from(2);
        };
        arquivo_config = Some(caminho);
        args.drain(1..3);
    }
    let ferramenta = match pelo_nome {
        Some(nome) => nome.to_string(),
        None if args.len() < 2 => {
            eprint!("{AJUDA}");
            return ExitCode::from(2);
        }
        None => args.remove(1),
    };
    if let Some(caminho) = &arquivo_config
        && let Err(erro) = configuracao::aplicar(std::path::Path::new(caminho), &ferramenta)
    {
        eprintln!("wortic: {erro}");
        return ExitCode::from(2);
    }
    let resto = &args[1..];
    match ferramenta.as_str() {
        "cnpj" => cadastro_cnpj::cli::principal(resto),
        "chatbot" => chatbot_prospeccao::cli::principal(resto),
        "pix" => pix_brcode::cli::principal(resto),
        "rastreio" => rastreio_capi::cli::principal(resto),
        "config" => {
            print!("{}", configuracao::EXEMPLO);
            ExitCode::SUCCESS
        }
        "versao" | "--version" | "-V" => {
            println!(
                "wortic {} (libjsonw {})",
                env!("CARGO_PKG_VERSION"),
                wortic_base::versao_jsonw()
            );
            ExitCode::SUCCESS
        }
        "ajuda" | "--help" | "-h" => {
            print!("{AJUDA}");
            ExitCode::SUCCESS
        }
        outra => {
            eprintln!("ferramenta desconhecida: {outra}\n");
            eprint!("{AJUDA}");
            ExitCode::from(2)
        }
    }
}
