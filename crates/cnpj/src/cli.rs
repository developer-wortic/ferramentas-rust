//! `wortic cnpj`: a linha de comando do cadastro. Sem argumento (ou com `servir`), sobe a tela e a API. Com um
//! subcomando, consulta a mesma API pela mesma chave (`WORTIC_API_KEY`) e base (`CADASTRO_API_BASE`) e escreve em
//! jsonW-texto. `--json` sai em JSON, a forma da API.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use serde_json::{Value, json}; // borda-json: API cnpj.wortic.com (o `--json` devolve a forma da API; o registro de validar sai igual)
use wortic_base::Formato;

use crate::{api, cnpj, lote, saida};

const AJUDA: &str = "\
wortic cnpj: cadastro de clientes que se preenche pelo CNPJ

uso: wortic cnpj [servir | consultar | socios | buscar | validar | lote | ajuda]

  servir                             (padrão) sobe a tela e a API. Variáveis: PORTA (8080), ENDERECO (127.0.0.1),
                                     WORTIC_API_KEY (opcional), CADASTRO_CACHE_MAX (5000), CADASTRO_API_BASE (a API aberta)
  consultar <cnpj>                   a ficha da empresa
  socios <cnpj>                      os sócios e administradores
  buscar --nome <texto> [--uf SP]    empresas ativas pelo nome (4 letras ou mais)
  validar <cnpj>                     confere os dígitos, sem rede (aceita o CNPJ alfanumérico)
  lote <entrada> <saida.jsw>         uma ficha por CNPJ da entrada (um registro jsonW por linha, {cnpj:\"...\"})

A saída é jsonW-texto. Com --json, sai em JSON (a forma da API). Erro vai para stderr e sai com código 1;
uso errado sai com código 2.
";

const MSG_DIGITOS: &str = "CNPJ inválido: confira os dígitos.";

/// As opções e os argumentos de um subcomando.
#[derive(Debug, Default, PartialEq)]
struct Opcoes {
    posicionais: Vec<String>,
    json: bool,
    nome: Option<String>,
    uf: Option<String>,
}

/// Lê o que vem depois do subcomando: `--json`, `--nome X`, `--uf X`, e o resto como argumento.
fn ler_opcoes(args: &[String]) -> Result<Opcoes, String> {
    let mut opcoes = Opcoes::default();
    let mut restantes = args.iter();
    while let Some(arg) = restantes.next() {
        match arg.as_str() {
            "--json" => opcoes.json = true,
            "--nome" => opcoes.nome = Some(valor_de(&mut restantes, "--nome")?),
            "--uf" => opcoes.uf = Some(valor_de(&mut restantes, "--uf")?),
            outro if outro.starts_with("--") => {
                return Err(format!("opção desconhecida: {outro}"));
            }
            _ => opcoes.posicionais.push(arg.clone()),
        }
    }
    Ok(opcoes)
}

fn valor_de<'a>(
    restantes: &mut impl Iterator<Item = &'a String>,
    opcao: &str,
) -> Result<String, String> {
    restantes
        .next()
        .cloned()
        .ok_or_else(|| format!("{opcao} pede um valor"))
}

/// O formato da saída: jsonW-texto, ou JSON com `--json`.
fn formato_de(opcoes: &Opcoes) -> Formato {
    if opcoes.json {
        Formato::Json
    } else {
        Formato::Texto
    }
}

/// Uso errado (comando, argumento ou opção): mostra a ajuda e sai com 2.
fn uso_errado(mensagem: &str) -> ExitCode {
    eprintln!("wortic cnpj: {mensagem}\n");
    eprint!("{AJUDA}");
    ExitCode::from(2)
}

/// Falha de uma consulta ou de uma entrada: uma linha no stderr e sai com 1.
fn falhar(mensagem: &str) -> ExitCode {
    eprintln!("wortic cnpj: {mensagem}");
    ExitCode::from(1)
}

/// Mostra a resposta da API (no formato pedido) ou o erro dela. Um 200 que não traz JSON é falha, não sucesso.
fn imprimir_api(r: &api::Resposta, formato: Formato) -> ExitCode {
    if r.status != 200 {
        return falhar(&api::descrever_erro(r));
    }
    // borda-json: API cnpj.wortic.com (confere que a API mandou JSON antes de converter a forma)
    if serde_json::from_str::<Value>(&r.corpo).is_err() {
        return falhar("A API respondeu algo que não consegui ler.");
    }
    println!("{}", saida::corpo(formato, &r.corpo));
    ExitCode::SUCCESS
}

/// O CNPJ do argumento, limpo e conferido. Sem dígito certo, não sai da máquina.
fn cnpj_do_argumento(arg: &str) -> Result<String, String> {
    let limpo = cnpj::limpar(arg);
    if cnpj::valido(&limpo) {
        Ok(limpo)
    } else {
        Err(MSG_DIGITOS.to_string())
    }
}

/// Ponto de entrada do subcomando.
pub fn principal(args: &[String]) -> ExitCode {
    let Some((sub, resto)) = args.split_first() else {
        return servir();
    };
    match sub.as_str() {
        "servir" => servir(),
        "ajuda" | "--help" | "-h" => {
            print!("{AJUDA}");
            ExitCode::SUCCESS
        }
        "consultar" | "socios" | "validar" | "buscar" | "lote" => match ler_opcoes(resto) {
            Ok(opcoes) => comando(sub, &opcoes),
            Err(erro) => uso_errado(&erro),
        },
        outro => uso_errado(&format!("comando desconhecido: {outro}")),
    }
}

fn servir() -> ExitCode {
    crate::principal();
    ExitCode::SUCCESS
}

/// Roda um subcomando já lido. Cada um confere os seus argumentos antes de sair da máquina.
fn comando(sub: &str, opcoes: &Opcoes) -> ExitCode {
    let formato = formato_de(opcoes);
    match (sub, opcoes.posicionais.as_slice()) {
        ("consultar", [arg]) => match cnpj_do_argumento(arg) {
            Ok(numero) => imprimir_api(
                &api::Api::nova().buscar(&format!("/cnpj/{numero}")),
                formato,
            ),
            Err(erro) => falhar(&erro),
        },
        ("socios", [arg]) => match cnpj_do_argumento(arg) {
            Ok(numero) => imprimir_api(
                &api::Api::nova().buscar(&format!("/cnpj/{numero}/socios")),
                formato,
            ),
            Err(erro) => falhar(&erro),
        },
        ("validar", [arg]) => validar(arg, formato),
        ("buscar", []) => buscar(opcoes, formato),
        ("lote", [entrada, destino]) => executar_lote(entrada, destino),
        _ => uso_errado(&format!("uso incorreto de `{sub}`")),
    }
}

/// Confere os dígitos sem rede. Sai com 1 quando o CNPJ não fecha, com o registro mostrado mesmo assim.
fn validar(arg: &str, formato: Formato) -> ExitCode {
    let limpo = cnpj::limpar(arg);
    let valido = cnpj::valido(&limpo);
    let registro = json!({"cnpj": limpo, "valido": valido}).to_string();
    println!("{}", saida::corpo(formato, &registro));
    if valido {
        ExitCode::SUCCESS
    } else {
        falhar(MSG_DIGITOS)
    }
}

/// Busca empresas pelo nome, com UF opcional. Mesma regra da tela: pelo menos 4 letras.
fn buscar(opcoes: &Opcoes, formato: Formato) -> ExitCode {
    let Some(nome) = opcoes.nome.as_deref() else {
        return uso_errado("buscar pede --nome <texto>");
    };
    if nome.chars().filter(|c| !c.is_whitespace()).count() < 4 {
        return falhar("Digite pelo menos 4 letras do nome.");
    }
    let mut caminho = format!("/empresas?nome={}", api::codificar(nome));
    if let Some(uf) = opcoes.uf.as_deref() {
        caminho = format!("{caminho}&uf={}", api::codificar(uf));
    }
    imprimir_api(&api::Api::nova().buscar(&caminho), formato)
}

/// Lê a entrada, consulta cada CNPJ e grava o `.jsw`. Linha que falha vai no arquivo com o motivo; o lote segue.
fn executar_lote(entrada: &str, destino: &str) -> ExitCode {
    let texto = match fs::read_to_string(entrada) {
        Ok(texto) => texto,
        Err(erro) => return falhar(&format!("não consegui ler {entrada}: {erro}")),
    };
    match lote::executar(&api::Api::nova(), &texto, Path::new(destino)) {
        Ok(resumo) => {
            eprintln!(
                "{} CNPJs, {} com erro. Arquivo: {destino}",
                resumo.total, resumo.com_erro
            );
            ExitCode::SUCCESS
        }
        Err(erro) => falhar(&erro),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn lista(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| (*a).to_string()).collect()
    }

    #[test]
    fn opcoes_separam_flags_de_argumentos() {
        let o = ler_opcoes(&lista(&["--nome", "padaria x", "--uf", "SP", "--json"])).unwrap();
        assert_eq!(
            o,
            Opcoes {
                posicionais: vec![],
                json: true,
                nome: Some("padaria x".to_string()),
                uf: Some("SP".to_string()),
            }
        );
        let o = ler_opcoes(&lista(&["37.853.407/0001-71"])).unwrap();
        assert_eq!(o.posicionais, vec!["37.853.407/0001-71".to_string()]);
        assert!(!o.json);
    }

    #[test]
    fn opcao_sem_valor_ou_desconhecida_e_erro() {
        assert_eq!(
            ler_opcoes(&lista(&["--nome"])).unwrap_err(),
            "--nome pede um valor"
        );
        assert_eq!(
            ler_opcoes(&lista(&["--uf"])).unwrap_err(),
            "--uf pede um valor"
        );
        assert_eq!(
            ler_opcoes(&lista(&["--xis"])).unwrap_err(),
            "opção desconhecida: --xis"
        );
    }

    #[test]
    fn formato_padrao_e_jsonw_e_json_com_a_flag() {
        assert_eq!(formato_de(&Opcoes::default()), Formato::Texto);
        let com = Opcoes {
            json: true,
            ..Opcoes::default()
        };
        assert_eq!(formato_de(&com), Formato::Json);
    }

    #[test]
    fn cnpj_do_argumento_limpa_e_recusa_o_que_nao_fecha() {
        assert_eq!(
            cnpj_do_argumento("37.853.407/0001-71").unwrap(),
            "37853407000171"
        );
        assert_eq!(
            cnpj_do_argumento("12.abc.345/01de-35").unwrap(),
            "12ABC34501DE35"
        );
        assert_eq!(
            cnpj_do_argumento("37853407000172").unwrap_err(),
            MSG_DIGITOS
        );
    }
}
