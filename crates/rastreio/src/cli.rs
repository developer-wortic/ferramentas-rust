//! `wortic rastreio`: o servidor e a linha de comando. Sem subcomando, sobe o servidor (painel, pixel, `/evento`,
//! `/registros`). A saída é jsonW-texto; `--json` dá JSON para quem lê de fora (a borda). Nada disto envia evento à Meta:
//! só o servidor envia, e só com Pixel e token.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use wortic_base::arquivo;

use crate::captura::{self, Plataforma};
use crate::configuracao;
use crate::dados::Dados;
use crate::esquemas;
use crate::evento::{self, Entrada};
use crate::identificadores;
use crate::jsonw;
use crate::modelo::executar;
use crate::painel::mascarar;
use crate::saida;
use crate::venda::{self, buscar};

const AJUDA: &str = "\
wortic rastreio: rastreio de UTM e cliques + Conversions API da Meta

uso: wortic rastreio [servir] [--config arquivo.jsw.txt]
     wortic rastreio captura <url> [--json]
     wortic rastreio dados <arquivo.jsw.txt|-> [--json]
     wortic rastreio evento <arquivo.jsw.txt|-> [--json] [--config arquivo.jsw.txt]
     wortic rastreio registros <arquivo.jsw> [--json]
     wortic rastreio ajuda

  servir     (padrão) sobe o painel, o pixel, /evento e /registros. As variáveis estão no README (seção \"Ajustes\").
  captura    mostra a página, o UTM e os clids da URL (cada clid mascarado).
  dados      faz o hash SHA-256 dos dados do cliente, como a Meta pede. Não envia nada.
  evento     monta o evento da Conversions API a partir de uma venda. Não envia nada.
             Com --json, mostra o corpo exato que iria para a Meta.
  registros  lê um .jsw exportado (por exemplo o de RASTREIO_ARQUIVO) e confere a custódia.

  entrada    um registro jsonW-texto; `-` lê da entrada padrão.
  --json     saída em JSON, para quem lê de fora. O padrão é jsonW-texto.
  --config   arquivo jsonW-texto com as chaves das variáveis (também RASTREIO_CONFIG). A variável de ambiente vence.
";

/// As opções que valem para qualquer subcomando, e o que sobrou (o subcomando e os argumentos).
struct Opcoes {
    json: bool,
    config: Option<PathBuf>,
    resto: Vec<String>,
}

fn separar(args: &[String]) -> Result<Opcoes, String> {
    let mut opcoes = Opcoes {
        json: false,
        config: None,
        resto: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--json" {
            opcoes.json = true;
        } else if arg == "--config" {
            let valor = it.next().ok_or("--config precisa do nome do arquivo")?;
            opcoes.config = Some(PathBuf::from(valor));
        } else if let Some(valor) = arg.strip_prefix("--config=") {
            opcoes.config = Some(PathBuf::from(valor));
        } else {
            opcoes.resto.push(arg.clone());
        }
    }
    Ok(opcoes)
}

/// Ponto de entrada do subcomando.
pub fn principal(args: &[String]) -> ExitCode {
    let opcoes = match separar(args) {
        Ok(opcoes) => opcoes,
        Err(razao) => {
            eprintln!("wortic rastreio: {razao}");
            return ExitCode::from(2);
        }
    };
    let resultado = match opcoes.resto.first().map(String::as_str) {
        None | Some("servir") => servir(&opcoes),
        Some("captura") => captura_cmd(&opcoes),
        Some("dados") => dados_cmd(&opcoes),
        Some("evento") => evento_cmd(&opcoes),
        Some("registros") => registros_cmd(&opcoes),
        Some("ajuda" | "--help" | "-h") => {
            print!("{AJUDA}");
            Ok(())
        }
        Some(outro) => {
            eprintln!("wortic rastreio: comando desconhecido: {outro}\n");
            eprint!("{AJUDA}");
            return ExitCode::from(2);
        }
    };
    match resultado {
        Ok(()) => ExitCode::SUCCESS,
        Err(razao) => {
            eprintln!("{razao}");
            ExitCode::from(1)
        }
    }
}

fn servir(opcoes: &Opcoes) -> Result<(), String> {
    let ajustes = configuracao::ajustes(opcoes.config.as_deref())?;
    executar(&ajustes)
}

/// `wortic rastreio captura <url>`: a página, o UTM e os clids da URL. O clid sai mascarado, como na tela.
fn captura_cmd(opcoes: &Opcoes) -> Result<(), String> {
    let url = opcoes
        .resto
        .get(1)
        .ok_or("uso: wortic rastreio captura <url>")?;
    let captura = captura::de_url(url);
    let pagina = captura::url_sem_consulta(url).unwrap_or_else(|| "(página inválida)".to_string());
    let mut campos: Vec<(&str, String)> = vec![
        ("pagina", jsonw::texto(&pagina)),
        ("fonte", jsonw::opcional(captura.utm.source.as_deref())),
        ("meio", jsonw::opcional(captura.utm.medium.as_deref())),
        ("campanha", jsonw::opcional(captura.utm.campaign.as_deref())),
        ("conteudo", jsonw::opcional(captura.utm.content.as_deref())),
        ("termo", jsonw::opcional(captura.utm.term.as_deref())),
    ];
    for plataforma in Plataforma::TODAS {
        campos.push((
            plataforma.parametro(),
            jsonw::opcional(captura.clid(plataforma).map(mascarar).as_deref()),
        ));
    }
    emitir(opcoes.json, esquemas::CAPTURA, &jsonw::obj(&campos))
}

/// `wortic rastreio dados <arquivo|->`: os dados do cliente em claro entram, e sai só o `user_data` em hash.
fn dados_cmd(opcoes: &Opcoes) -> Result<(), String> {
    let ajustes = configuracao::ajustes(opcoes.config.as_deref())?;
    let registro = ler_entrada(opcoes.resto.get(1), "dados")?;
    let pares = jsonw::ler_registro(esquemas::DADOS, &registro)?;
    let s = |chave: &str| buscar(&pares, chave);
    let dados = Dados {
        email: s("email"),
        telefone: s("telefone"),
        nome: s("nome"),
        sobrenome: s("sobrenome"),
        cidade: s("cidade"),
        estado: s("uf"),
        cep: s("cep"),
        pais: s("pais"),
        nascimento: s("nascimento"),
        externo: s("cliente"),
    };
    let ddi = s("ddi").unwrap_or(&ajustes.ddi_padrao);
    let normalizados = dados.normalizados(ddi);
    let user_data = evento::UserData::de(&normalizados, None, None, None, None);
    emitir(
        opcoes.json,
        esquemas::DADOS_HASH,
        &saida::user_data(&user_data),
    )
}

/// `wortic rastreio evento <arquivo|->`: monta o evento como a Conversions API espera. Não envia.
/// Com `--json`, mostra o corpo exato do lote que iria para a Meta.
fn evento_cmd(opcoes: &Opcoes) -> Result<(), String> {
    let ajustes = configuracao::ajustes(opcoes.config.as_deref())?;
    let registro = ler_entrada(opcoes.resto.get(1), "evento")?;
    let campos = venda::campos_de(esquemas::EVENTO, &registro)
        .map_err(|erro| format!("{}: {}", erro.codigo, erro.mensagem))?;
    let pedido =
        venda::pedido(&campos).map_err(|erro| format!("{}: {}", erro.codigo, erro.mensagem))?;
    let s = |chave: &str| buscar(&campos, chave);
    let agora_s = agora_s();
    let momento_s = match s("momento") {
        None => agora_s,
        Some(valor) => valor
            .parse::<u64>()
            .map_err(|_| "momento precisa ser segundos desde 1970".to_string())?,
    };
    let dados = Dados {
        email: s("email"),
        telefone: s("telefone"),
        nome: s("nome"),
        sobrenome: s("sobrenome"),
        cidade: s("cidade"),
        estado: s("uf"),
        cep: s("cep"),
        pais: s("pais"),
        nascimento: s("nascimento"),
        externo: s("cliente"),
    };
    let entrada = Entrada {
        nome: pedido.nome,
        event_id: pedido.pedido,
        momento_s,
        agora_s,
        pagina: pedido.pagina,
        dados,
        ip: s("ip"),
        user_agent: s("user_agent"),
        fbc: s("fbc").and_then(identificadores::fbc_valido),
        fbp: s("fbp").and_then(identificadores::fbp_valido),
        valor_centavos: pedido.valor,
        moeda: Some(pedido.moeda),
        ddi_padrao: s("ddi").unwrap_or(&ajustes.ddi_padrao),
        enviar_ip_e_user_agent: !ajustes.sem_ip_e_user_agent,
    };
    let montado = evento::montar(&entrada).map_err(|erro| format!("evento inválido: {erro}"))?;
    if opcoes.json {
        // borda-json: o corpo exato do lote da Graph API da Meta; só mostrado, nada é enviado
        println!(
            "{}",
            evento::corpo(
                std::slice::from_ref(&montado),
                ajustes.test_event_code.as_deref()
            )
        ); // borda-json: API de Conversões da Meta
        return Ok(());
    }
    emitir(false, esquemas::EVENTO_MONTADO, &saida::evento(&montado))
}

/// `wortic rastreio registros <arquivo.jsw>`: lê um `.jsw` (confere a custódia) e mostra os registros.
fn registros_cmd(opcoes: &Opcoes) -> Result<(), String> {
    let caminho = opcoes
        .resto
        .get(1)
        .ok_or("uso: wortic rastreio registros <arquivo.jsw>")?;
    let caminho = Path::new(caminho);
    if opcoes.json {
        let linhas = arquivo::ler(caminho, false).map_err(|r| jsonw::sem_trecho(&r))?;
        println!("[{}]", linhas.join(",")); // borda-json: saída --json para quem lê de fora
    } else {
        for linha in arquivo::ler(caminho, true).map_err(|r| jsonw::sem_trecho(&r))? {
            println!("{linha}");
        }
    }
    Ok(())
}

/// Confere o registro contra o esquema pela libjsonw e imprime: jsonW-texto canônico, ou JSON na borda com `--json`.
fn emitir(json: bool, esquema: &str, registro: &str) -> Result<(), String> {
    let canonico = jsonw::conferir(esquema, registro)?;
    if json {
        println!("{}", jsonw::como_json(esquema, &canonico)?);
    } else {
        println!("{canonico}");
    }
    Ok(())
}

/// A entrada de um subcomando: o arquivo dado, ou a entrada padrão com `-`.
fn ler_entrada(arg: Option<&String>, comando: &str) -> Result<String, String> {
    match arg.map(String::as_str) {
        None => Err(format!(
            "uso: wortic rastreio {comando} <arquivo.jsw.txt|->"
        )),
        Some("-") => {
            let mut texto = String::new();
            std::io::stdin()
                .read_to_string(&mut texto)
                .map_err(|erro| format!("não consegui ler a entrada: {erro}"))?;
            Ok(texto)
        }
        Some(caminho) => fs::read_to_string(caminho)
            .map_err(|erro| format!("não consegui ler {caminho}: {erro}")),
    }
}

fn agora_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
