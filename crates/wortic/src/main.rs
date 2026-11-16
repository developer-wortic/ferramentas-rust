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
//! Cada ferramenta lê as mesmas variáveis de antes (`PORTA`, `ENDERECO`, ...). Chamado pelo nome antigo (um link
//! `pix-brcode`, `cadastro-cnpj`, `chatbot-prospeccao` ou `rastreio-capi-modelo` apontando para `wortic`), sobe a
//! ferramenta daquele nome: quem já usava os binários separados troca sem mudar nada.

use std::process::ExitCode;

const AJUDA: &str = "\
wortic: ferramentas abertas da Wortic (jsonW em tudo, JSON só na borda)

uso: wortic <ferramenta> [argumentos]

  cnpj       cadastro de clientes que se preenche pelo CNPJ (tela + API)
  chatbot    chatbot de prospecção com a sua chave (painel + widget)
  pix        Pix estático: gera e lê o BR Code e desenha o QR (tela + API)
  rastreio   rastreio de UTM e cliques + Conversions API da Meta (painel + pixel)
  versao     versão do wortic e da libjsonw

Sem argumento, cada ferramenta sobe o servidor dela (PORTA, padrão 8080; ENDERECO, padrão 127.0.0.1).
`wortic <ferramenta> ajuda` mostra o resto.
";

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().collect();
    let chamado = args
        .first()
        .map(|a| {
            std::path::Path::new(a)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default();
    // pelo nome antigo do binário, a ferramenta é o próprio nome
    let ferramenta = match chamado.as_str() {
        "cadastro-cnpj" => "cnpj".to_string(),
        "chatbot-prospeccao" => "chatbot".to_string(),
        "pix-brcode" => "pix".to_string(),
        "rastreio-capi-modelo" | "rastreio-capi" => "rastreio".to_string(),
        _ => {
            if args.len() < 2 {
                eprint!("{AJUDA}");
                return ExitCode::from(2);
            }
            args.remove(1)
        }
    };
    let resto = &args[1..];
    match ferramenta.as_str() {
        "cnpj" => cadastro_cnpj::cli::principal(resto),
        "chatbot" => chatbot_prospeccao::cli::principal(resto),
        "pix" => pix_brcode::cli::principal(resto),
        "rastreio" => rastreio_capi::cli::principal(resto),
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
