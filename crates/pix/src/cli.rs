//! `wortic pix`: a linha de comando do Pix. Sem argumento (ou com `servir`), sobe o servidor (tela + API). Os outros
//! subcomandos são offline e devolvem jsonW-texto. A opção `--json` existe para a borda: ferramenta de fora que só fala
//! JSON. borda-json: quem pede --json é ferramenta de fora. Erro sai em stderr com código diferente de zero: 1 para
//! dado ruim, 2 para uso errado.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

use wortic_base::{Formato, arquivo, config};

use crate::chave::Chave;
use crate::cobranca::Cobranca;
use crate::erro::Erro;
use crate::jsonw::{ESQUEMA_CHAVE, ESQUEMA_COBRANCA, ESQUEMA_ERRO, ESQUEMA_LEITURA, ESQUEMA_LOTE};
use crate::{jsonw, leitura, lote, qr, registro, servidor};

const AJUDA: &str = concat!(
    "wortic pix: Pix estático (BR Code e QR), com jsonW em tudo\n",
    "\n",
    "uso: wortic pix [servir | gerar | ler | chave | lote | ajuda] [argumentos]\n",
    "\n",
    "  servir                             (padrão) sobe a tela e a API. Variáveis: PORTA (8080), ENDERECO (127.0.0.1), WORTIC_API_KEY (opcional)\n",
    "  gerar --chave X --nome Y --cidade Z [--valor 12,50] [--txid T] [--descricao D] [--svg arquivo.svg]\n",
    "                                     monta a cobrança: copia e cola, campos e valor. --svg grava o QR em SVG\n",
    "  ler <codigo|->                     lê um copia e cola (- lê da entrada padrão) e devolve os campos\n",
    "  chave <chave>                      confere a chave (tipo e validade)\n",
    "  lote entrada.jsw.txt saida.jsw     uma cobrança por linha da entrada; grava o .jsw só se todas as linhas passarem\n",
    "\n",
    "Opções: --json, para quem de fora só fala JSON (o padrão é jsonW-texto). O erro sai em stderr, no mesmo formato,\n",
    "com código 1 (dado) ou 2 (uso). // borda-json: ferramenta de fora que só fala JSON\n",
);

/// O que um subcomando devolve: o registro jsonW e o esquema que ele segue.
struct Saida {
    esquema: &'static str,
    registro: String,
}

/// Uma falha: o código de saída, a linha do lote (quando for de uma linha) e a razão em português.
#[derive(Debug)]
struct Falha {
    codigo: u8,
    linha: Option<usize>,
    razao: String,
}

impl Falha {
    fn dado(razao: impl Into<String>) -> Self {
        Self {
            codigo: 1,
            linha: None,
            razao: razao.into(),
        }
    }

    fn uso(razao: impl Into<String>) -> Self {
        Self {
            codigo: 2,
            linha: None,
            razao: razao.into(),
        }
    }
}

impl From<Erro> for Falha {
    fn from(erro: Erro) -> Self {
        Self::dado(erro.to_string())
    }
}

/// Ponto de entrada do subcomando.
pub fn principal(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        None | Some("servir") => {
            let codigo = servidor::executar(&|nome| std::env::var(nome).ok());
            ExitCode::from(u8::try_from(codigo).unwrap_or(1))
        }
        Some("ajuda" | "--help" | "-h") => {
            print!("{AJUDA}");
            ExitCode::SUCCESS
        }
        Some(comando @ ("gerar" | "ler" | "chave" | "lote")) => {
            let resto = &args[1..];
            let formato = if resto.iter().any(|a| a == "--json") {
                // borda-json: ferramenta de fora
                Formato::Json
            } else {
                Formato::Texto
            };
            match executar(comando, resto) {
                Ok(saida) => match jsonw::registro(formato, saida.esquema, &saida.registro) {
                    Ok(texto) => {
                        println!("{texto}");
                        ExitCode::SUCCESS
                    }
                    Err(razao) => sair_com(formato, &Falha::dado(razao)),
                },
                Err(falha) => sair_com(formato, &falha),
            }
        }
        Some(outro) => {
            eprintln!("wortic pix: comando desconhecido: {outro}\n");
            eprint!("{AJUDA}");
            ExitCode::from(2)
        }
    }
}

fn executar(comando: &str, resto: &[String]) -> Result<Saida, Falha> {
    match comando {
        "gerar" => gerar(resto),
        "ler" => ler(resto),
        "chave" => chave(resto),
        _ => lote(resto),
    }
}

/// Escreve o erro em stderr, no formato pedido, e devolve o código de saída.
fn sair_com(formato: Formato, falha: &Falha) -> ExitCode {
    let registro = registro::erro(falha.linha, &falha.razao);
    let texto = jsonw::registro(formato, ESQUEMA_ERRO, &registro).unwrap_or(registro);
    eprintln!("{texto}");
    ExitCode::from(falha.codigo)
}

/// Os argumentos de um subcomando: as opções nomeadas (com valor) e as posições, na ordem. `--json` não entra aqui.
#[derive(Default)]
struct Argumentos {
    nomeados: Vec<(&'static str, String)>,
    posicoes: Vec<String>,
}

impl Argumentos {
    /// O valor da última vez que a opção veio.
    fn valor(&self, nome: &str) -> Option<&str> {
        self.nomeados
            .iter()
            .rfind(|(n, _)| *n == nome)
            .map(|(_, v)| v.as_str())
    }
}

/// Separa as opções conhecidas (`--nome valor` ou `--nome=valor`), as posições e o `--json`. Opção desconhecida é erro de uso.
fn separar(resto: &[String], com_valor: &[&'static str]) -> Result<Argumentos, Falha> {
    let mut a = Argumentos::default();
    let mut itens = resto.iter();
    while let Some(arg) = itens.next() {
        if arg == "--json" {
            // borda-json: ferramenta de fora
            continue;
        }
        let Some(sem_traco) = arg.strip_prefix("--") else {
            a.posicoes.push(arg.clone());
            continue;
        };
        let (nome, embutido) = match sem_traco.split_once('=') {
            Some((nome, valor)) => (nome, Some(valor.to_string())),
            None => (sem_traco, None),
        };
        let Some(&opcao) = com_valor.iter().find(|c| **c == nome) else {
            return Err(Falha::uso(format!("opção desconhecida: --{nome}")));
        };
        let valor = match embutido {
            Some(valor) => valor,
            None => itens
                .next()
                .cloned()
                .ok_or_else(|| Falha::uso(format!("--{nome} pede um valor")))?,
        };
        a.nomeados.push((opcao, valor));
    }
    Ok(a)
}

/// As posições certas para o subcomando, ou um erro de uso que diz o que ele pede.
fn posicoes<'a>(a: &'a Argumentos, quantas: usize, pede: &str) -> Result<&'a [String], Falha> {
    if a.posicoes.len() == quantas {
        Ok(&a.posicoes)
    } else {
        Err(Falha::uso(format!("uso: {pede}")))
    }
}

const OPCOES_GERAR: &[&str] = &[
    "chave",
    "nome",
    "cidade",
    "valor",
    "txid",
    "descricao",
    "svg",
];

fn gerar(resto: &[String]) -> Result<Saida, Falha> {
    let a = separar(resto, OPCOES_GERAR)?;
    posicoes(
        &a,
        0,
        "gerar --chave X --nome Y --cidade Z [--valor 12,50] [--txid T] [--descricao D] [--svg arquivo.svg]",
    )?;
    let obrigatorio = |nome: &'static str, campo: &'static str| {
        a.valor(nome)
            .ok_or_else(|| Falha::from(Erro::Obrigatorio(campo)))
    };
    let mut cobranca = Cobranca::nova(
        obrigatorio("chave", "chave Pix")?,
        obrigatorio("nome", "nome do recebedor")?,
        obrigatorio("cidade", "cidade")?,
    )?;
    if let Some(valor) = a.valor("valor") {
        cobranca = cobranca.com_valor(valor)?;
    }
    if let Some(txid) = a.valor("txid") {
        cobranca = cobranca.com_txid(txid)?;
    }
    if let Some(descricao) = a.valor("descricao") {
        cobranca = cobranca.com_descricao(descricao)?;
    }
    if let Some(caminho) = a.valor("svg") {
        let svg = qr::svg(&cobranca.codigo()?)?;
        config::gravar(Path::new(caminho), &svg, false)
            .map_err(|erro| Falha::dado(format!("não consegui gravar {caminho}: {erro}")))?;
    }
    Ok(Saida {
        esquema: ESQUEMA_COBRANCA,
        registro: registro::cobranca(&cobranca, None)?,
    })
}

fn ler(resto: &[String]) -> Result<Saida, Falha> {
    let a = separar(resto, &[])?;
    let [entrada] = posicoes(&a, 1, "ler <codigo|-> (- lê da entrada padrão)")? else {
        unreachable!("posicoes confere a quantidade");
    };
    let codigo = if entrada == "-" {
        let mut lido = String::new();
        std::io::stdin()
            .read_to_string(&mut lido)
            .map_err(|erro| Falha::dado(format!("não consegui ler a entrada padrão: {erro}")))?;
        lido
    } else {
        entrada.clone()
    };
    let l = leitura::ler(&codigo)?;
    Ok(Saida {
        esquema: ESQUEMA_LEITURA,
        registro: registro::leitura(&l),
    })
}

fn chave(resto: &[String]) -> Result<Saida, Falha> {
    let a = separar(resto, &[])?;
    let [texto] = posicoes(&a, 1, "chave <chave>")? else {
        unreachable!("posicoes confere a quantidade");
    };
    let chave = Chave::interpretar(texto)?;
    Ok(Saida {
        esquema: ESQUEMA_CHAVE,
        registro: registro::chave(&chave),
    })
}

fn lote(resto: &[String]) -> Result<Saida, Falha> {
    let a = separar(resto, &[])?;
    let [entrada, saida] = posicoes(&a, 2, "lote entrada.jsw.txt saida.jsw")? else {
        unreachable!("posicoes confere a quantidade");
    };
    let texto = fs::read_to_string(entrada)
        .map_err(|erro| Falha::dado(format!("não consegui ler {entrada}: {erro}")))?;
    let registros = lote::montar(&texto).map_err(|(linha, razao)| Falha {
        codigo: 1,
        linha,
        razao,
    })?;
    let gravados = arquivo::gravar(
        Path::new(saida),
        ESQUEMA_COBRANCA,
        registros.iter().map(String::as_str),
    )
    .map_err(|erro| Falha::dado(jsonw::razao(erro)))?;
    Ok(Saida {
        esquema: ESQUEMA_LOTE,
        registro: registro::lote(gravados, saida),
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn textos(lista: &[&str]) -> Vec<String> {
        lista.iter().map(|t| (*t).to_string()).collect()
    }

    #[test]
    fn separa_opcoes_com_e_sem_igual_e_as_posicoes() {
        let a = separar(
            &textos(&["--nome", "Ana", "--cidade=Recife", "x", "--json", "y"]),
            &["nome", "cidade"],
        )
        .unwrap();
        assert_eq!(a.valor("nome"), Some("Ana"));
        assert_eq!(a.valor("cidade"), Some("Recife"));
        assert_eq!(a.posicoes, ["x", "y"]);
        assert_eq!(a.valor("txid"), None);
    }

    #[test]
    fn a_ultima_vez_da_opcao_vence() {
        let a = separar(&textos(&["--nome", "A", "--nome=B"]), &["nome"]).unwrap();
        assert_eq!(a.valor("nome"), Some("B"));
    }

    #[test]
    fn opcao_desconhecida_ou_sem_valor_e_erro_de_uso() {
        let erro = separar(&textos(&["--zorra", "1"]), &["nome"])
            .err()
            .unwrap();
        assert_eq!(erro.codigo, 2);
        assert!(erro.razao.contains("--zorra"));
        let erro = separar(&textos(&["--nome"]), &["nome"]).err().unwrap();
        assert_eq!(erro.codigo, 2);
        assert!(erro.razao.contains("pede um valor"));
    }

    #[test]
    fn posicoes_so_passam_com_a_quantidade_certa() {
        let a = separar(&textos(&["um"]), &[]).unwrap();
        assert!(posicoes(&a, 1, "ler").is_ok());
        let erro = posicoes(&a, 2, "lote a b").err().unwrap();
        assert_eq!(erro.codigo, 2);
        assert_eq!(erro.razao, "uso: lote a b");
    }

    #[test]
    fn falha_de_uso_e_de_dado_tem_codigos_diferentes() {
        assert_eq!(Falha::uso("x").codigo, 2);
        assert_eq!(Falha::dado("x").codigo, 1);
        let de_erro: Falha = Erro::Qr.into();
        assert_eq!(de_erro.codigo, 1);
        assert_eq!(de_erro.linha, None);
    }

    #[test]
    fn gerar_sem_chave_e_erro_de_dado_com_o_campo() {
        let erro = gerar(&textos(&["--nome", "Ana", "--cidade", "Recife"]))
            .err()
            .unwrap();
        assert_eq!(erro.codigo, 1);
        assert_eq!(erro.razao, "chave Pix é obrigatório.");
    }

    #[test]
    fn gerar_monta_o_registro_com_o_valor_tipado() {
        let s = gerar(&textos(&[
            "--chave",
            "123.456.789-09",
            "--nome",
            "Ana",
            "--cidade",
            "Recife",
            "--valor",
            "12,5",
        ]))
        .unwrap();
        assert_eq!(s.esquema, ESQUEMA_COBRANCA);
        assert!(s.registro.contains("valor:12.50 BRL"), "{}", s.registro);
        assert!(s.registro.contains("qr_svg:null"));
    }
}
