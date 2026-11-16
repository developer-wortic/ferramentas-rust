//! Roda o binário de verdade (o `main` só é coberto assim). Nada sai da máquina e nenhum evento vai à Meta:
//! o modelo sem Pixel e token fica em modo demonstração.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Uma porta livre agora. Ela é solta logo depois, para o binário usar.
fn porta_livre() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("porta livre")
        .local_addr()
        .expect("endereço")
        .port()
}

#[test]
fn porta_que_nao_abre_para_com_a_mensagem_e_codigo_1() {
    let saida = wortic("rastreio")
        .env("PORTA", "abc")
        .env_remove("RASTREIO_PIXEL_ID")
        .output()
        .expect("roda o binário");
    assert_eq!(saida.status.code(), Some(1));
    let erro = String::from_utf8_lossy(&saida.stderr);
    assert!(erro.contains("não consegui abrir"), "stderr: {erro}");
}

#[test]
fn segredo_curto_para_com_a_mensagem() {
    let saida = wortic("rastreio")
        .env("PORTA", porta_livre().to_string())
        .env("RASTREIO_SEGREDO", "curto")
        .output()
        .expect("roda o binário");
    assert_eq!(saida.status.code(), Some(1));
    let erro = String::from_utf8_lossy(&saida.stderr);
    assert!(erro.contains("32 caracteres"), "stderr: {erro}");
}

#[test]
fn sobe_atende_os_pedidos_e_sai_limpo_no_limite() {
    let porta = porta_livre();
    let mut filho = wortic("rastreio")
        .env("PORTA", porta.to_string())
        .env("RASTREIO_MAX_PEDIDOS", "3")
        .env("RASTREIO_SEGREDO", "0123456789abcdef0123456789abcdef")
        .env_remove("RASTREIO_PIXEL_ID")
        .env_remove("RASTREIO_ACCESS_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sobe o binário");
    let linhas = BufReader::new(filho.stdout.take().expect("saída"));
    let (envio, recebe) = std::sync::mpsc::channel();
    thread::spawn(move || {
        for linha in linhas.lines().map_while(Result::ok) {
            if envio.send(linha).is_err() {
                break;
            }
        }
    });

    // a primeira resposta conta como pedido 1; antes de subir, a conexão recusada não conta
    let base = format!("http://127.0.0.1:{porta}");
    let inicio = Instant::now();
    loop {
        if ureq::get(&format!("{base}/")).call().is_ok() {
            break;
        }
        assert!(
            inicio.elapsed() < Duration::from_secs(10),
            "o binário não subiu"
        );
        thread::sleep(Duration::from_millis(50));
    }
    let pixel = ureq::get(&format!(
        "{base}/r?u=https%3A%2F%2Floja.com%2F%3Futm_source%3Dfb"
    ))
    .call();
    assert!(pixel.is_ok(), "pixel respondeu");
    let agente = ureq::AgentBuilder::new().redirects(0).build();
    let _ = agente
        .get(&format!("{base}/consentimento?aceitar=nao"))
        .call();

    let limite = Instant::now();
    let codigo = loop {
        if let Some(status) = filho.try_wait().expect("espera") {
            break status;
        }
        if limite.elapsed() > Duration::from_secs(10) {
            let _ = filho.kill();
            panic!("o binário não saiu depois do limite de pedidos");
        }
        thread::sleep(Duration::from_millis(50));
    };
    assert!(codigo.success(), "saiu com {codigo}");
    let primeira = recebe
        .recv_timeout(Duration::from_secs(5))
        .expect("a primeira linha da saída");
    assert!(
        primeira.starts_with("Rastreio CAPI no ar em http://127.0.0.1:"),
        "saída: {primeira}"
    );
}

/// Roda `wortic rastreio <args>` com uma entrada opcional e variáveis extras; devolve a saída inteira.
fn roda(args: &[&str], entrada: Option<&str>, ambiente: &[(&str, &str)]) -> std::process::Output {
    let mut comando = wortic("rastreio");
    comando
        .args(args)
        .env_remove("RASTREIO_PIXEL_ID")
        .env_remove("RASTREIO_ACCESS_TOKEN")
        .env_remove("RASTREIO_CONFIG")
        .env_remove("RASTREIO_ARQUIVO")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (chave, valor) in ambiente {
        comando.env(chave, valor);
    }
    let mut filho = comando.spawn().expect("roda o binário");
    if let Some(texto) = entrada {
        let mut entrada_do_filho = filho.stdin.take().expect("entrada do filho");
        entrada_do_filho
            .write_all(texto.as_bytes())
            .expect("escreve a entrada");
    }
    filho.wait_with_output().expect("espera o binário")
}

fn temporario(nome: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("rastreio-bin-{nome}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("pasta de teste");
    p
}

fn agora_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("relógio")
        .as_secs()
}

fn texto(saida: &[u8]) -> String {
    String::from_utf8_lossy(saida).into_owned()
}

#[test]
fn ajuda_lista_os_subcomandos_e_o_desconhecido_sai_com_2() {
    let ajuda = roda(&["ajuda"], None, &[]);
    assert_eq!(ajuda.status.code(), Some(0));
    let saida = texto(&ajuda.stdout);
    for palavra in [
        "captura",
        "dados",
        "evento",
        "registros",
        "--config",
        "--json",
    ] {
        // borda-json: a flag --json é a saída para quem lê de fora
        assert!(saida.contains(palavra), "falta {palavra} na ajuda: {saida}");
    }
    let ruim = roda(&["nada"], None, &[]);
    assert_eq!(ruim.status.code(), Some(2));
}

#[test]
fn captura_mostra_utm_e_o_clid_mascarado_em_jsonw() {
    let s = roda(
        &[
            "captura",
            "https://loja.com/?utm_source=facebook&fbclid=ABCDEFGHIJK",
        ],
        None,
        &[],
    );
    assert_eq!(s.status.code(), Some(0), "{}", texto(&s.stderr));
    let saida = texto(&s.stdout);
    assert!(saida.contains("fonte:\"facebook\""), "{saida}");
    assert!(saida.contains("fbclid:\"ABCDEF…\""), "{saida}");
    assert!(!saida.contains("GHIJK"), "o clid inteiro não sai: {saida}");
}

#[test]
fn captura_sem_url_da_erro_de_uso() {
    let s = roda(&["captura"], None, &[]);
    assert_eq!(s.status.code(), Some(1));
    assert!(texto(&s.stderr).contains("uso:"), "{}", texto(&s.stderr));
}

#[test]
fn dados_devolve_so_o_hash_do_cliente_e_o_json_e_na_borda() {
    let entrada = r#"{email:"John_Smith@gmail.com",telefone:"(650)555-1212",nome:"Mary"}"#;
    let s = roda(&["dados", "-"], Some(entrada), &[]);
    assert_eq!(s.status.code(), Some(0), "{}", texto(&s.stderr));
    let saida = texto(&s.stdout);
    // vetor publicado pela Meta para john_smith@gmail.com
    assert!(
        saida.contains("62a14e44f765419d10fea99367361a727c12365e2520f32218d505ed9aa0f62f"),
        "{saida}"
    );
    assert!(
        !saida.contains("John") && !saida.contains("Mary") && !saida.contains("555"),
        "{saida}"
    );
    let json = roda(&["dados", "-", "--json"], Some(entrada), &[]); // borda-json: saída --json para quem lê de fora
    assert!(
        texto(&json.stdout).contains("\"em\":[\"62a14e44"),
        "{}",
        texto(&json.stdout)
    ); // borda-json: saída --json para quem lê de fora
}

#[test]
fn evento_monta_o_corpo_da_meta_sem_enviar_nada() {
    let momento = agora_s();
    let entrada = format!(
        r#"{{evento:"Purchase",pedido:"1001",valor:99.90 BRL,u:"https://loja.com/obrigado?pedido=1001",email:"John_Smith@gmail.com",momento:{momento}}}"#
    );
    // a base da API aponta para uma porta fechada: se o comando tentasse enviar, falharia
    let ambiente = [
        ("RASTREIO_PIXEL_ID", "123456789012345"),
        ("RASTREIO_ACCESS_TOKEN", "TOKEN_DE_TESTE_NAO_SAI"),
        ("RASTREIO_API_BASE", "http://127.0.0.1:9"),
        ("RASTREIO_TEST_EVENT_CODE", "TEST77"),
    ];
    let s = roda(&["evento", "-"], Some(&entrada), &ambiente);
    assert_eq!(s.status.code(), Some(0), "{}", texto(&s.stderr));
    let saida = texto(&s.stdout);
    assert!(saida.contains("event_name:\"Purchase\""), "{saida}");
    assert!(
        saida.contains("event_source_url:\"https://loja.com/obrigado\""),
        "{saida}"
    );
    assert!(
        !saida.contains("TOKEN_DE_TESTE") && !saida.contains("John"),
        "{saida}"
    );

    let json = roda(&["evento", "-", "--json"], Some(&entrada), &ambiente); // borda-json: o corpo exato da Graph API, mostrado com --json
    assert_eq!(json.status.code(), Some(0), "{}", texto(&json.stderr));
    let corpo = texto(&json.stdout);
    assert!(corpo.contains("\"data\":[{"), "{corpo}"); // borda-json: corpo da Graph API
    assert!(corpo.contains("\"test_event_code\":\"TEST77\""), "{corpo}"); // borda-json: corpo da Graph API
    assert!(corpo.contains("\"value\":99.9"), "{corpo}"); // borda-json: corpo da Graph API
    assert!(!corpo.contains("TOKEN_DE_TESTE"), "{corpo}");
}

#[test]
fn evento_sem_pedido_e_recusado_pelo_esquema() {
    let s = roda(&["evento", "-"], Some(r#"{u:"https://loja.com/"}"#), &[]);
    assert_eq!(s.status.code(), Some(1));
    assert!(
        texto(&s.stderr).contains("jsonw_invalido"),
        "{}",
        texto(&s.stderr)
    );
}

#[test]
fn configuracao_do_arquivo_vale_e_a_variavel_de_ambiente_vence() {
    let dir = temporario("config");
    let arquivo = dir.join("rastreio.jsw.txt");
    std::fs::write(
        &arquivo,
        "// teste\n{\n  PORTA: 1,\n  RASTREIO_MAX_PEDIDOS: 1,\n  RASTREIO_SEGREDO: \"0123456789abcdef0123456789abcdef\",\n}\n",
    )
    .expect("escreve a configuração");
    // a porta 1 do arquivo não serve (não é root); a variável PORTA vence e o limite de 1 pedido vem do arquivo
    let porta = porta_livre();
    let mut filho = wortic("rastreio")
        .args(["--config", arquivo.to_str().expect("caminho")])
        .env("PORTA", porta.to_string())
        .env_remove("RASTREIO_PIXEL_ID")
        .env_remove("RASTREIO_ACCESS_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sobe o binário");
    let saida_inicial = filho.stdout.take().expect("saída");
    let (envio, recebe) = std::sync::mpsc::channel();
    thread::spawn(move || {
        for linha in BufReader::new(saida_inicial).lines().map_while(Result::ok) {
            if envio.send(linha).is_err() {
                break;
            }
        }
    });
    let base = format!("http://127.0.0.1:{porta}");
    let inicio = Instant::now();
    while ureq::get(&format!("{base}/")).call().is_err() {
        assert!(inicio.elapsed() < Duration::from_secs(10), "não subiu");
        thread::sleep(Duration::from_millis(50));
    }
    let limite = Instant::now();
    let codigo = loop {
        if let Some(status) = filho.try_wait().expect("espera") {
            break status;
        }
        if limite.elapsed() > Duration::from_secs(10) {
            let _ = filho.kill();
            panic!("o limite do arquivo não foi aplicado");
        }
        thread::sleep(Duration::from_millis(50));
    };
    assert!(codigo.success(), "saiu com {codigo}");
    let primeira = recebe
        .recv_timeout(Duration::from_secs(5))
        .expect("a linha de início");
    assert_eq!(
        primeira,
        format!("Rastreio CAPI no ar em http://127.0.0.1:{porta}"),
        "a porta da variável venceu o arquivo"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn configuracao_com_segredo_mal_tipado_para_e_nao_mostra_o_valor() {
    let dir = temporario("segredo");
    let arquivo = dir.join("ruim.jsw.txt");
    std::fs::write(
        &arquivo,
        "{RASTREIO_SEGREDO: 98765432109876543210987654321098765432}\n",
    )
    .expect("escreve");
    let s = roda(&["--config", arquivo.to_str().expect("caminho")], None, &[]);
    assert_eq!(s.status.code(), Some(1));
    let erro = texto(&s.stderr);
    assert!(erro.contains("configuração"), "{erro}");
    assert!(
        !erro.contains("98765432109876543210"),
        "o segredo não sai: {erro}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn servidor_grava_o_jsw_ao_sair_e_registros_le_de_volta() {
    let dir = temporario("jsw");
    let eventos = dir.join("registros.jsw");
    let porta = porta_livre();
    let mut filho = wortic("rastreio")
        .env("PORTA", porta.to_string())
        .env("RASTREIO_SEGREDO", "0123456789abcdef0123456789abcdef")
        .env("RASTREIO_MAX_PEDIDOS", "3")
        .env("RASTREIO_ARQUIVO", eventos.to_str().expect("caminho"))
        .env_remove("RASTREIO_PIXEL_ID")
        .env_remove("RASTREIO_ACCESS_TOKEN")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("sobe o binário");
    let base = format!("http://127.0.0.1:{porta}");
    let inicio = Instant::now();
    while ureq::get(&format!("{base}/")).call().is_err() {
        assert!(inicio.elapsed() < Duration::from_secs(10), "não subiu");
        thread::sleep(Duration::from_millis(50));
    }
    // o pixel com consentimento e UTM e clid: a origem entra na memória
    let pixel = ureq::get(&format!(
        "{base}/r?u=https%3A%2F%2Floja.com%2F%3Futm_source%3Dfacebook%26fbclid%3DCLIQUE123"
    ))
    .set("Cookie", "rc_consent=1")
    .call();
    assert!(pixel.is_ok(), "pixel respondeu");
    // terceiro pedido: o limite de 3 encerra e grava os arquivos
    let _ = ureq::get(&format!("{base}/")).call();
    let limite = Instant::now();
    let codigo = loop {
        if let Some(status) = filho.try_wait().expect("espera") {
            break status;
        }
        if limite.elapsed() > Duration::from_secs(10) {
            let _ = filho.kill();
            panic!("o binário não saiu no limite de pedidos");
        }
        thread::sleep(Duration::from_millis(50));
    };
    assert!(codigo.success(), "saiu com {codigo}");
    assert!(eventos.exists(), "o arquivo de eventos foi gravado");

    let origens = dir.join("origens.jsw");
    let s = roda(
        &["registros", origens.to_str().expect("caminho")],
        None,
        &[],
    );
    assert_eq!(s.status.code(), Some(0), "{}", texto(&s.stderr));
    let linha = texto(&s.stdout);
    assert!(linha.contains("fonte:\"facebook\""), "{linha}");
    assert!(
        !linha.contains("CLIQUE123"),
        "o clid não sai no arquivo: {linha}"
    );
    assert!(linha.contains("fbclid_hash:\""), "{linha}");
    let json = roda(
        &["registros", origens.to_str().expect("caminho"), "--json"],
        None,
        &[],
    ); // borda-json: saída --json para quem lê de fora
    assert!(
        texto(&json.stdout).starts_with('['),
        "{}",
        texto(&json.stdout)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// O binário único com o subcomando da ferramenta (antes era um binário por ferramenta).
fn wortic(ferramenta: &str) -> Command {
    let mut comando = Command::new(env!("CARGO_BIN_EXE_wortic"));
    comando.arg(ferramenta);
    comando
}
