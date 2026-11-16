//! Roda o binário de verdade (o `main` só é coberto assim). Nada sai da máquina.

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

/// Sobe o binário com o ambiente dado e devolve o processo e um canal com as linhas da saída.
fn subir_binario(ambiente: &[(&str, &str)], sem_chave: bool) -> (Child, mpsc::Receiver<String>) {
    let mut comando = wortic("cnpj");
    comando
        // porta 0: o sistema escolhe e o aviso de "no ar" diz qual (sem corrida entre testes em paralelo)
        .env("PORTA", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if sem_chave {
        comando.env_remove("WORTIC_API_KEY");
    }
    for (chave, valor) in ambiente {
        comando.env(chave, valor);
    }
    let mut filho = comando.spawn().expect("sobe o binário");
    let saida = filho.stdout.take().expect("saída do binário");
    let (envio, recebe) = mpsc::channel();
    thread::spawn(move || {
        for linha in BufReader::new(saida).lines().map_while(Result::ok) {
            if envio.send(linha).is_err() {
                break;
            }
        }
    });
    (filho, recebe)
}

#[test]
fn porta_que_nao_abre_para_o_binario_com_a_mensagem() {
    let saida = wortic("cnpj")
        .env("PORTA", "abc")
        .output()
        .expect("roda o binário");
    assert!(!saida.status.success());
    let erro = String::from_utf8_lossy(&saida.stderr);
    assert!(
        erro.contains("não consegui abrir a porta"),
        "stderr: {erro}"
    );
}

#[test]
fn binario_sobe_na_porta_pedida_e_serve_a_tela() {
    // pega uma porta livre, solta, e pede ao binário que use ela
    let porta = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut filho = wortic("cnpj")
        .env("PORTA", porta.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("sobe o binário");
    let url = format!("http://127.0.0.1:{porta}/");
    let inicio = Instant::now();
    let mut corpo = None;
    let mut ultimo = String::new();
    while inicio.elapsed() < Duration::from_secs(10) {
        match ureq::get(&url).timeout(Duration::from_secs(2)).call() {
            Ok(r) => {
                corpo = r.into_string().ok();
                break;
            }
            Err(e) => ultimo = e.to_string(),
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = filho.kill();
    let _ = filho.wait();
    let corpo = corpo.unwrap_or_else(|| panic!("o binário não respondeu na raiz: {ultimo}"));
    assert!(
        corpo.contains("<html") || corpo.contains("<!DOCTYPE"),
        "tela: {corpo:.80}"
    );
}

#[test]
fn sem_chave_e_fora_da_maquina_o_log_avisa_a_cota_compartilhada() {
    let (mut filho, linhas) = subir_binario(&[("ENDERECO", "0.0.0.0")], true);
    let no_ar = linhas.recv_timeout(Duration::from_secs(10));
    let aviso = linhas.recv_timeout(Duration::from_secs(10));
    let _ = filho.kill();
    let _ = filho.wait();
    assert!(no_ar.expect("linha de no ar").contains("no ar"));
    let aviso = aviso.expect("o aviso de cota no log");
    assert!(
        aviso.contains("dividem 60 consultas por minuto e 2.000 por dia"),
        "aviso: {aviso}"
    );
}

#[test]
fn com_chave_fora_da_maquina_nao_ha_aviso_de_cota() {
    let (mut filho, linhas) = subir_binario(
        &[
            ("ENDERECO", "0.0.0.0"),
            ("WORTIC_API_KEY", "chave-de-teste"),
        ],
        false,
    );
    let no_ar = linhas.recv_timeout(Duration::from_secs(10));
    // a segunda linha só viria com o aviso: se não vem em 1,5 s, não há aviso
    let segunda = linhas.recv_timeout(Duration::from_millis(1500));
    let _ = filho.kill();
    let _ = filho.wait();
    assert!(no_ar.expect("linha de no ar").contains("no ar"));
    assert!(
        segunda.is_err(),
        "não devia haver segunda linha: {segunda:?}"
    );
}

#[test]
fn na_propria_maquina_sem_chave_nao_ha_aviso_de_cota() {
    let (mut filho, linhas) = subir_binario(&[("ENDERECO", "127.0.0.1")], true);
    let no_ar = linhas.recv_timeout(Duration::from_secs(10));
    let segunda = linhas.recv_timeout(Duration::from_millis(1500));
    let _ = filho.kill();
    let _ = filho.wait();
    assert!(no_ar.expect("linha de no ar").contains("no ar"));
    assert!(
        segunda.is_err(),
        "não devia haver segunda linha: {segunda:?}"
    );
}

/// O binário único com o subcomando da ferramenta (antes era um binário por ferramenta).
fn wortic(ferramenta: &str) -> Command {
    let mut comando = Command::new(env!("CARGO_BIN_EXE_wortic"));
    comando.arg(ferramenta);
    comando
}

/// Um subcomando do `wortic cnpj` com os argumentos dados, sem chave no ambiente (cada teste põe a sua).
fn cnpj_com(args: &[&str]) -> Command {
    let mut comando = wortic("cnpj");
    comando.args(args).env_remove("WORTIC_API_KEY");
    comando
}

/// Resposta do servidor falso: status, corpo e cabeçalhos extras.
type Roteiro = (u16, String, Vec<(&'static str, &'static str)>);

/// Caminho e chave de cada pedido que chegou ao servidor falso.
type Pedidos = Arc<Mutex<Vec<(String, Option<String>)>>>;

/// Um servidor local que faz de conta que é a API da Wortic. Guarda o caminho e a chave de cada pedido.
struct Falso {
    base: String,
    pedidos: Pedidos,
    servidor: Arc<tiny_http::Server>,
}

impl Drop for Falso {
    fn drop(&mut self) {
        self.servidor.unblock();
    }
}

/// Sobe a API falsa numa porta livre. `roteiro` decide a resposta para cada caminho.
fn falso(roteiro: impl Fn(&str) -> Roteiro + Send + Sync + 'static) -> Falso {
    let servidor = Arc::new(tiny_http::Server::http("127.0.0.1:0").expect("porta livre"));
    let base = format!(
        "http://{}",
        servidor.server_addr().to_ip().expect("endereço IP")
    );
    let pedidos = Arc::new(Mutex::new(Vec::new()));
    let (no_fio, gravados) = (servidor.clone(), pedidos.clone());
    thread::spawn(move || {
        for pedido in no_fio.incoming_requests() {
            let url = pedido.url().to_string();
            let chave = pedido
                .headers()
                .iter()
                .find(|h| h.field.equiv("x-api-key"))
                .map(|h| h.value.as_str().to_string());
            gravados.lock().unwrap().push((url.clone(), chave));
            let (status, corpo, cabecalhos) = roteiro(&url);
            let mut resposta = tiny_http::Response::from_string(corpo).with_status_code(status);
            for (nome, valor) in cabecalhos {
                resposta = resposta
                    .with_header(tiny_http::Header::from_bytes(nome, valor).expect("cabeçalho"));
            }
            let _ = pedido.respond(resposta);
        }
    });
    Falso {
        base,
        pedidos,
        servidor,
    }
}

/// Os caminhos que a API falsa recebeu, na ordem.
fn caminhos(f: &Falso) -> Vec<String> {
    f.pedidos
        .lock()
        .unwrap()
        .iter()
        .map(|(c, _)| c.clone())
        .collect()
}

/// A ficha mínima que a API manda (o CNPJ e os dados são de teste).
// borda-json: API cnpj.wortic.com (a ficha chega da API em JSON; o binário a converte)
const FICHA: &str = r#"{"cnpj":"37853407000171","razao_social":"EMPRESA DE TESTE LTDA"}"#;

#[test]
fn consultar_sai_em_jsonw_por_padrao() {
    let f = falso(|_| (200, FICHA.to_string(), vec![]));
    let saida = cnpj_com(&["consultar", "37.853.407/0001-71"])
        .env("CADASTRO_API_BASE", &f.base)
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(0), "{saida:?}");
    assert_eq!(
        String::from_utf8_lossy(&saida.stdout),
        "{cnpj:\"37853407000171\",razao_social:\"EMPRESA DE TESTE LTDA\"}\n"
    );
    assert_eq!(caminhos(&f), vec!["/cnpj/37853407000171".to_string()]);
}

#[test]
fn consultar_com_a_flag_sai_a_forma_da_api() {
    let f = falso(|_| (200, FICHA.to_string(), vec![]));
    // borda-json: API cnpj.wortic.com (a flag devolve a forma da API, sem converter)
    let saida = cnpj_com(&["consultar", "37853407000171", "--json"])
        .env("CADASTRO_API_BASE", &f.base)
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&saida.stdout), format!("{FICHA}\n"));
}

#[test]
fn consultar_cnpj_com_digito_errado_sai_com_1_e_nao_consulta() {
    let f = falso(|_| (200, FICHA.to_string(), vec![]));
    let saida = cnpj_com(&["consultar", "37853407000172"])
        .env("CADASTRO_API_BASE", &f.base)
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&saida.stderr).contains("CNPJ inválido"));
    assert_eq!(saida.stdout.len(), 0, "nada no stdout");
    assert_eq!(caminhos(&f).len(), 0, "nenhum pedido à API");
}

#[test]
fn erro_da_api_vai_para_o_stderr_com_a_espera_e_o_link() {
    let f = falso(|_| {
        (
            402,
            "{\"erro\":{\"codigo\":\"limite_gratis\",\"mensagem\":\"x\"},\"planos\":[]}"
                .to_string(),
            vec![("retry-after", "37")],
        )
    });
    let saida = cnpj_com(&["consultar", "37853407000171"])
        .env("CADASTRO_API_BASE", &f.base)
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(1));
    assert_eq!(saida.stdout.len(), 0, "nada no stdout");
    let erro = String::from_utf8_lossy(&saida.stderr);
    assert!(erro.contains("Passou do limite grátis"), "{erro}");
    assert!(erro.contains("Tente de novo em 37 s."), "{erro}");
    assert!(
        erro.contains("Planos: https://cnpj.wortic.com/#planos"),
        "{erro}"
    );
}

#[test]
fn socios_consulta_a_rota_e_sai_em_jsonw() {
    let f = falso(|_| (200, "[{\"nome\":\"ANA\"}]".to_string(), vec![]));
    let saida = cnpj_com(&["socios", "37853407000171"])
        .env("CADASTRO_API_BASE", &f.base)
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&saida.stdout), "[{nome:\"ANA\"}]\n");
    assert_eq!(
        caminhos(&f),
        vec!["/cnpj/37853407000171/socios".to_string()]
    );
}

#[test]
fn resposta_200_que_nao_e_json_e_falha_com_1() {
    let f = falso(|_| (200, "manutencao".to_string(), vec![]));
    let saida = cnpj_com(&["socios", "37853407000171"])
        .env("CADASTRO_API_BASE", &f.base)
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(1));
    assert_eq!(saida.stdout.len(), 0, "nada no stdout");
    assert!(String::from_utf8_lossy(&saida.stderr).contains("não consegui ler"));
}

#[test]
fn buscar_manda_o_nome_codificado_e_a_uf() {
    let f = falso(|_| (200, "{\"empresas\":[]}".to_string(), vec![]));
    let saida = cnpj_com(&["buscar", "--nome", "padaria x", "--uf", "SP"])
        .env("CADASTRO_API_BASE", &f.base)
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&saida.stdout), "{empresas:[]}\n");
    assert_eq!(
        caminhos(&f),
        vec!["/empresas?nome=padaria%20x&uf=SP".to_string()]
    );
}

#[test]
fn buscar_com_nome_curto_sai_com_1_sem_consultar() {
    let f = falso(|_| (200, "[]".to_string(), vec![]));
    let saida = cnpj_com(&["buscar", "--nome", "abc"])
        .env("CADASTRO_API_BASE", &f.base)
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&saida.stderr).contains("pelo menos 4 letras"));
    assert_eq!(caminhos(&f).len(), 0, "nenhum pedido à API");
}

#[test]
fn buscar_sem_nome_e_uso_errado_com_2() {
    let saida = cnpj_com(&["buscar", "--uf", "SP"]).output().expect("roda");
    assert_eq!(saida.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&saida.stderr).contains("--nome"));
}

#[test]
fn validar_e_offline_aceita_o_alfanumerico_e_recusa_o_dv_errado() {
    // a base aponta para um endereço fechado: validar não pode tocar na rede
    let ok = cnpj_com(&["validar", "12.ABC.345/01DE-35"])
        .env("CADASTRO_API_BASE", "http://127.0.0.1:9")
        .output()
        .expect("roda");
    assert_eq!(ok.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&ok.stdout),
        "{cnpj:\"12ABC34501DE35\",valido:true}\n"
    );
    // borda-json: API cnpj.wortic.com (a flag devolve o registro em JSON, a forma da API)
    let ruim = cnpj_com(&["validar", "37853407000172", "--json"])
        .env("CADASTRO_API_BASE", "http://127.0.0.1:9")
        .output()
        .expect("roda");
    assert_eq!(ruim.status.code(), Some(1));
    assert_eq!(
        String::from_utf8_lossy(&ruim.stdout),
        "{\"cnpj\":\"37853407000172\",\"valido\":false}\n"
    );
}

#[test]
fn lote_grava_o_jsw_com_fichas_e_erro_por_linha() {
    let f = falso(|caminho| {
        if caminho == "/cnpj/37853407000171" {
            (
                200,
                "{\"cnpj\":\"37853407000171\",\"razao_social\":\"EMPRESA DE TESTE LTDA\",\"endereco\":{\"uf\":\"SC\"},\"situacao\":{\"descricao\":\"Ativa\"},\"capital_social\":1234.5,\"abertura\":\"2020-07-24\"}".to_string(),
                vec![],
            )
        } else {
            (404, "{\"erro\":\"nao achei\"}".to_string(), vec![])
        }
    });
    let pasta = std::env::temp_dir().join(format!("wortic-lote-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let entrada = pasta.join("entrada.jsw.txt");
    let destino = pasta.join("fichas.jsw");
    std::fs::write(
        &entrada,
        "// clientes\n{cnpj:\"37.853.407/0001-71\"}\n{cnpj:\"37853407000172\"}\n{cnpj:\"11444777000161\"}\n",
    )
    .unwrap();
    let saida = cnpj_com(&["lote", entrada.to_str().unwrap(), destino.to_str().unwrap()])
        .env("CADASTRO_API_BASE", &f.base)
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(0), "{saida:?}");
    // o lote consulta só os dois CNPJs com dígito certo
    assert_eq!(
        caminhos(&f),
        vec![
            "/cnpj/37853407000171".to_string(),
            "/cnpj/11444777000161".to_string()
        ]
    );
    // a custódia do .jsw confere, e a libjsonw lê as três linhas em jsonW-texto
    let linhas = wortic_base::arquivo::ler(&destino, true).expect("o .jsw confere");
    assert_eq!(linhas.len(), 3, "{linhas:?}");
    assert!(
        linhas[0].starts_with("{cnpj:\"37853407000171\",razao_social:\"EMPRESA DE TESTE LTDA\"")
    );
    assert!(
        linhas[0].contains("capital_social:1234.50 BRL"),
        "{}",
        linhas[0]
    );
    assert!(linhas[1].contains("erro:\"CNPJ inválido"), "{}", linhas[1]);
    assert!(
        linhas[2].contains("erro:\"A API respondeu com erro 404.\""),
        "{}",
        linhas[2]
    );
    let _ = std::fs::remove_dir_all(&pasta);
}

#[test]
fn lote_com_linha_fora_do_formato_nao_grava_e_sai_com_1() {
    let pasta = std::env::temp_dir().join(format!("wortic-lote-ruim-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    let entrada = pasta.join("entrada.txt");
    let destino = pasta.join("fichas.jsw");
    std::fs::write(&entrada, "37853407000171\n").unwrap();
    let saida = cnpj_com(&["lote", entrada.to_str().unwrap(), destino.to_str().unwrap()])
        .env("CADASTRO_API_BASE", "http://127.0.0.1:9")
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&saida.stderr).contains("linha 1"));
    assert!(!destino.exists());
    let _ = std::fs::remove_dir_all(&pasta);
}

#[test]
fn chave_vai_no_cabecalho_e_nunca_sai_na_saida() {
    let f = falso(|_| (200, FICHA.to_string(), vec![]));
    let saida = cnpj_com(&["consultar", "37853407000171"])
        .env("CADASTRO_API_BASE", &f.base)
        .env("WORTIC_API_KEY", "segredo-de-teste")
        .output()
        .expect("roda");
    assert_eq!(saida.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&saida.stdout).contains("segredo"));
    assert_eq!(
        f.pedidos.lock().unwrap()[0].1.as_deref(),
        Some("segredo-de-teste")
    );
}

#[test]
fn comando_desconhecido_sai_com_2_e_a_ajuda_lista_os_subcomandos() {
    let desconhecido = cnpj_com(&["nao-existe"]).output().expect("roda");
    assert_eq!(desconhecido.status.code(), Some(2));
    let ajuda = cnpj_com(&["ajuda"]).output().expect("roda");
    assert_eq!(ajuda.status.code(), Some(0));
    let texto = String::from_utf8_lossy(&ajuda.stdout);
    for sub in ["consultar", "socios", "buscar", "validar", "lote"] {
        assert!(texto.contains(sub), "a ajuda não mostra {sub}");
    }
}

#[test]
fn binario_serve_a_api_no_formato_pedido() {
    let f = falso(|_| (200, FICHA.to_string(), vec![]));
    let (mut filho, linhas) = subir_binario(&[("CADASTRO_API_BASE", &f.base)], true);
    let no_ar = linhas
        .recv_timeout(Duration::from_secs(10))
        .expect("linha de no ar");
    // "Cadastro pelo CNPJ no ar em http://127.0.0.1:PORTA": a porta é a última parte
    let porta = no_ar.rsplit(':').next().unwrap_or_default().to_string();
    let url = format!("http://127.0.0.1:{porta}/api/empresa/37853407000171");
    // borda-json: navegador (sem pedido, a tela recebe a ficha em JSON, como antes)
    let padrao = ureq::get(&url).timeout(Duration::from_secs(5)).call();
    let texto = ureq::get(&url)
        .timeout(Duration::from_secs(5))
        .set("accept", "text/jsonw")
        .call();
    let _ = filho.kill();
    let _ = filho.wait();
    let padrao = padrao
        .expect("resposta padrão")
        .into_string()
        .expect("corpo");
    assert_eq!(padrao, FICHA);
    let texto = texto.expect("resposta em jsonW");
    assert_eq!(
        texto.header("content-type"),
        Some("text/jsonw; charset=utf-8")
    );
    assert_eq!(
        texto.into_string().expect("corpo"),
        "{cnpj:\"37853407000171\",razao_social:\"EMPRESA DE TESTE LTDA\"}"
    );
}
