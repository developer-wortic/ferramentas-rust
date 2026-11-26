//! Roda o binário de verdade e fala com ele por HTTP, sem crate extra: um `TcpStream` basta.
//! Tudo fica na máquina local (127.0.0.1 ou `::1`), sem rede externa.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Contador da suíte: cada porta pedida é diferente da anterior, para dois testes nunca dividirem uma porta.
static PEDIDOS: AtomicUsize = AtomicUsize::new(0);

/// Uma porta que nenhum outro teste desta suíte recebeu. Fica abaixo da faixa efêmera do sistema
/// (a partir de 49152), para não colidir com conexões de saída de outros programas.
fn porta_livre() -> u16 {
    let n = PEDIDOS.fetch_add(1, Ordering::SeqCst);
    20_000 + u16::try_from((std::process::id() as usize * 7 + n * 13) % 20_000).unwrap()
}

/// Um binário no ar, derrubado quando sai do escopo.
struct Servidor {
    filho: Child,
    porta: u16,
}

impl Drop for Servidor {
    fn drop(&mut self) {
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

fn subir(ambiente: &[(&str, &str)]) -> Servidor {
    let porta = porta_livre();
    let mut comando = wortic("pix");
    comando
        .env_remove("WORTIC_API_KEY")
        .env("PORTA", porta.to_string())
        .env("ENDERECO", "127.0.0.1")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (chave, valor) in ambiente {
        comando.env(chave, valor);
    }
    let filho = comando.spawn().expect("sobe o binário");
    let servidor = Servidor { filho, porta };
    esperar(&format!("127.0.0.1:{porta}"));
    servidor
}

/// Espera o binário responder HTTP (até 10 segundos). Uma conexão aberta não basta:
/// tem de vir a resposta do nosso servidor.
fn esperar(endereco: &str) {
    let fim = Instant::now() + Duration::from_secs(10);
    while Instant::now() < fim {
        if responde(endereco) {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("o binário não respondeu em {endereco}");
}

/// Um GET de teste: `true` se a resposta começa com 200.
fn responde(endereco: &str) -> bool {
    let Ok(mut s) = std::net::TcpStream::connect(endereco) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = write!(
        s,
        "GET /api/config HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    );
    let mut bruto = String::new();
    let _ = s.read_to_string(&mut bruto);
    bruto.starts_with("HTTP/1.1 200")
}

/// Uma resposta HTTP: status, tipo do conteúdo e corpo.
struct Visto {
    status: u16,
    tipo: String,
    corpo: String,
}

fn pedir(servidor: &Servidor, caminho: &str) -> Visto {
    pedir_em(&format!("127.0.0.1:{}", servidor.porta), caminho)
}

fn pedir_em(endereco: &str, caminho: &str) -> Visto {
    pedir_com(endereco, caminho, &[])
}

/// GET com cabeçalhos extras (cada um como `nome: valor`).
fn pedir_com(endereco: &str, caminho: &str, extras: &[&str]) -> Visto {
    let mut s = std::net::TcpStream::connect(endereco).expect("conecta");
    s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut cabecalhos = String::new();
    for extra in extras {
        cabecalhos.push_str(extra);
        cabecalhos.push_str("\r\n");
    }
    write!(
        s,
        "GET {caminho} HTTP/1.1\r\nHost: localhost\r\n{cabecalhos}Connection: close\r\n\r\n"
    )
    .unwrap();
    let mut bruto = String::new();
    s.read_to_string(&mut bruto).unwrap();
    let status = bruto
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("linha de status");
    let tipo = bruto
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("content-type:"))
        .map(|l| l["content-type:".len()..].trim().to_string())
        .unwrap_or_default();
    let corpo = bruto
        .split_once("\r\n\r\n")
        .map_or_else(String::new, |(_, c)| c.to_string());
    Visto {
        status,
        tipo,
        corpo,
    }
}

/// Codifica para a query: o que não é letra, número ou `-._~` vira %XX.
fn codificar(texto: &str) -> String {
    texto
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

#[test]
fn serve_a_tela_e_a_config_sem_mostrar_a_chave() {
    let s = subir(&[("WORTIC_API_KEY", "segredo-de-teste")]);
    let tela = pedir(&s, "/");
    assert_eq!(tela.status, 200);
    assert!(tela.corpo.contains("Pix com QR Code"));
    let config = pedir(&s, "/api/config");
    assert_eq!(config.status, 200);
    assert_eq!(config.corpo, "{\"chave\":true}");
    assert!(!config.corpo.contains("segredo-de-teste"));
}

#[test]
fn sem_chave_a_config_diz_false() {
    let s = subir(&[("WORTIC_API_KEY", "")]);
    assert_eq!(pedir(&s, "/api/config").corpo, "{\"chave\":false}");
}

#[test]
fn gerar_e_ler_pelo_http_com_a_mesma_cobranca() {
    let s = subir(&[]);
    let consulta = format!(
        "/api/gerar?chave={}&nome={}&cidade={}&valor={}",
        codificar("123.456.789-09"),
        codificar("Loja Exemplo"),
        codificar("Sao Paulo"),
        codificar("10,50")
    );
    let gerado = pedir(&s, &consulta);
    assert_eq!(gerado.status, 200, "{}", gerado.corpo);
    assert!(gerado.corpo.contains("\"tipo_chave\":\"CPF\""));
    assert!(gerado.corpo.contains("<svg "));
    let codigo = gerado
        .corpo
        .split("\"codigo\":\"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .expect("código no JSON")
        .to_string();
    assert!(codigo.starts_with("00020101021126"));
    assert!(codigo.contains("540510.50"));

    let lido = pedir(&s, &format!("/api/ler?codigo={}", codificar(&codigo)));
    assert_eq!(lido.status, 200, "{}", lido.corpo);
    assert!(lido.corpo.contains("\"chave\":\"12345678909\""));
    assert!(lido.corpo.contains("\"valor\":\"10.50\""));
    assert!(lido.corpo.contains("\"nome\":\"Loja Exemplo\""));
}

#[test]
fn gerar_sem_campo_obrigatorio_da_400() {
    let s = subir(&[]);
    let r = pedir(&s, "/api/gerar?nome=Ana&cidade=Recife");
    assert_eq!(r.status, 400);
    assert!(r.corpo.contains("chave Pix é obrigatório."));
}

#[test]
fn ler_codigo_alterado_da_400_com_o_motivo() {
    let s = subir(&[]);
    let r = pedir(
        &s,
        "/api/ler?codigo=00020101021126330014br.gov.bcb.pix0111123456789095204000053039865802BR5912Loja%20Exemplo6009Sao%20Paulo62070503***630400AA",
    );
    assert_eq!(r.status, 400);
    assert!(r.corpo.contains("CRC não confere"));
}

#[test]
fn rota_que_nao_existe_da_404_em_json() {
    let s = subir(&[]);
    let r = pedir(&s, "/nao_existe");
    assert_eq!(r.status, 404);
    assert!(r.corpo.contains("Rota não encontrada."));
}

#[test]
fn porta_que_nao_abre_sai_com_a_mensagem() {
    let saida = wortic("pix")
        .env("PORTA", "abc")
        .env("ENDERECO", "127.0.0.1")
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
fn endereco_ipv6_sobe_entre_colchetes() {
    let porta = porta_livre();
    let mut comando = wortic("pix");
    comando
        .env("PORTA", porta.to_string())
        .env("ENDERECO", "::1")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut filho = comando.spawn().expect("sobe o binário");
    let endereco = format!("[::1]:{porta}");
    let fim = Instant::now() + Duration::from_secs(10);
    let subiu = loop {
        if std::net::TcpStream::connect(&endereco).is_ok() {
            break true;
        }
        if Instant::now() > fim {
            break false;
        }
        thread::sleep(Duration::from_millis(20));
    };
    if subiu {
        let r = pedir_em(&endereco, "/api/config");
        assert_eq!(r.status, 200);
    }
    let _ = filho.kill();
    let _ = filho.wait();
    assert!(subiu, "não abriu [::1]:{porta}");
}

/// Roda `wortic pix` com os argumentos e devolve a saída inteira.
fn pix(args: &[&str]) -> std::process::Output {
    wortic("pix").args(args).output().expect("roda o binário")
}

/// Roda `wortic pix` com um texto na entrada padrão.
fn pix_com_entrada(args: &[&str], entrada: &str) -> std::process::Output {
    use std::io::Write as _;
    let mut comando = wortic("pix");
    comando
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut filho = comando.spawn().expect("sobe o binário");
    filho
        .stdin
        .take()
        .expect("entrada padrão")
        .write_all(entrada.as_bytes())
        .unwrap();
    filho.wait_with_output().expect("espera o binário")
}

fn texto(saida: &[u8]) -> String {
    String::from_utf8_lossy(saida).into_owned()
}

/// Uma pasta de teste própria (nome + pid + número), limpa antes de usar.
fn pasta_de_teste(nome: &str) -> std::path::PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let pasta = std::env::temp_dir().join(format!("wortic-pix-{nome}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pasta);
    std::fs::create_dir_all(&pasta).unwrap();
    pasta
}

/// O copia e cola de uma saída de `gerar` em jsonW (o texto entre `codigo:"` e a próxima aspa).
fn codigo_do_gerar(saida: &str) -> String {
    saida
        .split("codigo:\"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .expect("codigo na saída")
        .to_string()
}

#[test]
fn gerar_sai_jsonw_por_padrao_e_json_com_a_opcao() {
    let args = [
        "gerar",
        "--chave",
        "123.456.789-09",
        "--nome",
        "Loja Exemplo",
        "--cidade",
        "Sao Paulo",
        "--valor",
        "10,50",
    ];
    let jsonw = pix(&args);
    assert!(jsonw.status.success(), "{}", texto(&jsonw.stderr));
    let t = texto(&jsonw.stdout);
    assert!(t.starts_with("{codigo:\"000201"), "{t}");
    assert!(t.contains("tipo_chave:\"CPF\""), "{t}");
    assert!(t.contains("valor:10.50 BRL"), "{t}");
    assert!(t.contains("qr_svg:null"), "{t}");

    let mut com_json = args.to_vec();
    com_json.push("--json"); // borda-json: ferramenta de fora que só fala JSON
    let para_fora = pix(&com_json); // borda-json: ferramenta de fora que só fala JSON
    assert!(para_fora.status.success());
    let j = texto(&para_fora.stdout); // borda-json: ferramenta de fora que só fala JSON
    assert!(j.starts_with("{\"codigo\":\"000201"), "{j}"); // borda-json: ferramenta de fora que só fala JSON
    assert!(j.contains("\"valor\":\"10.50 BRL\""), "{j}"); // borda-json: ferramenta de fora que só fala JSON
}

#[test]
fn gerar_com_svg_grava_o_qr_e_sem_svg_nao_grava_nada() {
    let pasta = pasta_de_teste("svg");
    let destino = pasta.join("qr.svg");
    let saida = pix(&[
        "gerar",
        "--chave",
        "contato@exemplo.com.br",
        "--nome",
        "Loja",
        "--cidade",
        "Recife",
        "--svg",
        destino.to_str().unwrap(),
    ]);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    let svg = std::fs::read_to_string(&destino).unwrap();
    assert!(svg.starts_with("<svg "), "{svg}");
    assert!(svg.contains("role=\"img\""));
    let _ = std::fs::remove_dir_all(&pasta);
}

#[test]
fn gerar_sem_campo_obrigatorio_sai_com_codigo_1_e_o_erro_em_jsonw() {
    let saida = pix(&["gerar", "--nome", "Ana", "--cidade", "Recife"]);
    assert_eq!(saida.status.code(), Some(1));
    assert_eq!(saida.stdout, Vec::<u8>::new());
    let erro = texto(&saida.stderr);
    assert!(
        erro.contains("mensagem:\"chave Pix é obrigatório.\""),
        "{erro}"
    );
    assert!(erro.contains("erro:{linha:null"), "{erro}");
}

#[test]
fn opcao_que_nao_existe_ou_argumento_a_mais_e_uso_2() {
    let saida = pix(&["gerar", "--zorra", "1"]);
    assert_eq!(saida.status.code(), Some(2));
    assert!(texto(&saida.stderr).contains("opção desconhecida: --zorra"));
    let saida = pix(&["ler"]);
    assert_eq!(saida.status.code(), Some(2));
    let saida = pix(&["comando-que-nao-existe"]);
    assert_eq!(saida.status.code(), Some(2));
}

#[test]
fn ler_o_que_o_gerar_fez_pelo_argumento_e_pela_entrada_padrao() {
    let gerado = pix(&[
        "gerar",
        "--chave",
        "123.456.789-09",
        "--nome",
        "Ana Maria",
        "--cidade",
        "Recife",
        "--valor",
        "25,90",
        "--txid",
        "PED7",
        "--descricao",
        "Pedido 7",
    ]);
    let codigo = codigo_do_gerar(&texto(&gerado.stdout));

    let lido = pix(&["ler", &codigo]);
    assert!(lido.status.success(), "{}", texto(&lido.stderr));
    let l = texto(&lido.stdout);
    assert!(l.starts_with("{chave:\"12345678909\""), "{l}");
    assert!(l.contains("valor:\"25.90\""), "{l}");
    assert!(l.contains("txid:\"PED7\""), "{l}");
    assert!(l.contains("descricao:\"Pedido 7\""), "{l}");
    assert!(l.contains("{id:\"00\",valor:\"01\"}"), "{l}");

    let da_entrada = pix_com_entrada(&["ler", "-"], &format!("{codigo}\n"));
    assert!(da_entrada.status.success(), "{}", texto(&da_entrada.stderr));
    assert_eq!(texto(&da_entrada.stdout), l);
}

#[test]
fn ler_codigo_alterado_sai_com_codigo_1() {
    let saida = pix(&[
        "ler",
        "00020101021126330014br.gov.bcb.pix0111123456789095204000053039865802BR5912Loja%20Exemplo6009Sao%20Paulo62070503***630400AA",
    ]);
    assert_eq!(saida.status.code(), Some(1));
    assert!(texto(&saida.stderr).contains("CRC não confere"));
}

#[test]
fn chave_valida_sai_normalizada_e_chave_ruim_sai_com_erro() {
    let ok = pix(&["chave", "123.456.789-09"]);
    assert!(ok.status.success());
    assert_eq!(
        texto(&ok.stdout),
        "{chave:\"12345678909\",tipo_chave:\"CPF\"}\n"
    );
    let ok = pix(&["chave", "+55 (11) 99999-8888", "--json"]); // borda-json: ferramenta de fora que só fala JSON
    assert_eq!(
        texto(&ok.stdout),
        "{\"chave\":\"+5511999998888\",\"tipo_chave\":\"telefone\"}\n" // borda-json: ferramenta de fora que só fala JSON
    );
    let ruim = pix(&["chave", "123.456.789-00"]);
    assert_eq!(ruim.status.code(), Some(1));
    assert!(texto(&ruim.stderr).contains("CPF inválido"));
}

#[test]
fn lote_grava_as_cobrancas_e_pula_comentario_e_linha_vazia() {
    let pasta = pasta_de_teste("lote-ok");
    let entrada = pasta.join("entrada.jsw.txt");
    let saida = pasta.join("saida.jsw");
    std::fs::write(
        &entrada,
        "// cobranças de novembro\n\n{chave:\"123.456.789-09\",nome:\"Ana\",cidade:\"Recife\",valor:10.50 BRL}\n{chave:\"contato@exemplo.com.br\",nome:\"Loja\",cidade:\"Recife\",txid:\"PED1\"}\n",
    )
    .unwrap();
    let r = pix(&["lote", entrada.to_str().unwrap(), saida.to_str().unwrap()]);
    assert!(r.status.success(), "{}", texto(&r.stderr));
    assert_eq!(
        texto(&r.stdout),
        format!("{{registros:2,arquivo:\"{}\"}}\n", saida.display())
    );
    let linhas = wortic_base::arquivo::ler(&saida, true).unwrap();
    assert_eq!(linhas.len(), 2);
    assert!(linhas[0].contains("valor:10.50 BRL"), "{}", linhas[0]);
    assert!(linhas[1].contains("txid:\"PED1\""), "{}", linhas[1]);
    let _ = std::fs::remove_dir_all(&pasta);
}

#[test]
fn lote_com_uma_linha_ruim_nao_publica_nada_e_diz_a_linha() {
    let pasta = pasta_de_teste("lote-ruim");
    let entrada = pasta.join("entrada.jsw.txt");
    let saida = pasta.join("saida.jsw");
    std::fs::write(
        &entrada,
        "{chave:\"123.456.789-09\",nome:\"Ana\",cidade:\"Recife\"}\n\n{chave:\"123.456.789-09\",nome:\"Ana\",cidade:\"Recife\",valor:0 BRL}\n",
    )
    .unwrap();
    let r = pix(&["lote", entrada.to_str().unwrap(), saida.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(1));
    let erro = texto(&r.stderr);
    assert!(erro.contains("linha:3"), "{erro}");
    assert!(erro.contains("maior que zero"), "{erro}");
    assert!(
        !saida.exists(),
        "o arquivo de saída não pode aparecer pela metade"
    );
    let _ = std::fs::remove_dir_all(&pasta);
}

#[test]
fn lote_sem_registro_ou_sem_arquivo_sai_com_erro() {
    let pasta = pasta_de_teste("lote-vazio");
    let entrada = pasta.join("entrada.jsw.txt");
    std::fs::write(&entrada, "// nada aqui\n").unwrap();
    let saida = pasta.join("saida.jsw");
    let r = pix(&["lote", entrada.to_str().unwrap(), saida.to_str().unwrap()]);
    assert_eq!(r.status.code(), Some(1));
    assert!(texto(&r.stderr).contains("não tem nenhum registro"));
    let r = pix(&[
        "lote",
        pasta.join("nao-existe.txt").to_str().unwrap(),
        saida.to_str().unwrap(),
    ]);
    assert_eq!(r.status.code(), Some(1));
    assert!(!saida.exists());
    let _ = std::fs::remove_dir_all(&pasta);
}

#[test]
fn ajuda_lista_os_subcomandos() {
    let r = pix(&["ajuda"]);
    assert!(r.status.success());
    let t = texto(&r.stdout);
    for sub in ["gerar", "ler", "chave", "lote", "servir"] {
        assert!(t.contains(sub), "{t}");
    }
}

#[test]
fn http_responde_em_jsonw_pelo_accept_e_pelo_parametro() {
    let s = subir(&[]);
    let endereco = format!("127.0.0.1:{}", s.porta);
    let r = pedir_com(&endereco, "/api/config", &["Accept: text/jsonw"]);
    assert_eq!(r.status, 200);
    assert_eq!(r.tipo, "text/jsonw; charset=utf-8");
    assert_eq!(r.corpo, "{chave:false}");

    let r = pedir(&s, "/api/config?formato=jsonw");
    assert_eq!(r.tipo, "text/jsonw; charset=utf-8");
    assert_eq!(r.corpo, "{chave:false}");
}

#[test]
fn http_sem_pedido_segue_em_json_para_a_tela() {
    let s = subir(&[]);
    let r = pedir(&s, "/api/config");
    assert_eq!(r.tipo, "application/json; charset=utf-8"); // borda-json: navegador (a tela lê JSON)
    assert_eq!(r.corpo, "{\"chave\":false}"); // borda-json: navegador (a tela lê JSON)
    let r = pedir_com(
        &format!("127.0.0.1:{}", s.porta),
        "/api/config",
        &["Accept: */*"],
    );
    assert_eq!(r.tipo, "application/json; charset=utf-8"); // borda-json: navegador (a tela lê JSON)
}

#[test]
fn http_gerar_e_ler_em_jsonw_com_a_mesma_cobranca() {
    let s = subir(&[]);
    let endereco = format!("127.0.0.1:{}", s.porta);
    let consulta = format!(
        "/api/gerar?chave={}&nome={}&cidade={}&valor={}",
        codificar("123.456.789-09"),
        codificar("Loja Exemplo"),
        codificar("Sao Paulo"),
        codificar("10,50")
    );
    let gerado = pedir_com(&endereco, &consulta, &["Accept: text/jsonw"]);
    assert_eq!(gerado.status, 200, "{}", gerado.corpo);
    assert_eq!(gerado.tipo, "text/jsonw; charset=utf-8");
    assert!(
        gerado.corpo.starts_with("{codigo:\"000201"),
        "{}",
        gerado.corpo
    );
    assert!(gerado.corpo.contains("valor:10.50 BRL"), "{}", gerado.corpo);
    let codigo = codigo_do_gerar(&gerado.corpo);

    let lido = pedir(
        &s,
        &format!("/api/ler?codigo={}&formato=jsonw", codificar(&codigo)),
    );
    assert_eq!(lido.status, 200, "{}", lido.corpo);
    assert_eq!(lido.tipo, "text/jsonw; charset=utf-8");
    assert!(lido.corpo.contains("valor:\"10.50\""), "{}", lido.corpo);
    assert!(
        lido.corpo.contains("nome:\"Loja Exemplo\""),
        "{}",
        lido.corpo
    );
}

#[test]
fn http_erro_e_404_seguem_o_formato_pedido() {
    let s = subir(&[]);
    let endereco = format!("127.0.0.1:{}", s.porta);
    let r = pedir_com(&endereco, "/api/gerar?nome=Ana", &["Accept: text/jsonw"]);
    assert_eq!(r.status, 400);
    assert_eq!(r.tipo, "text/jsonw; charset=utf-8");
    assert!(
        r.corpo.contains("mensagem:\"chave Pix é obrigatório.\""),
        "{}",
        r.corpo
    );
    let r = pedir_com(&endereco, "/nao_existe", &["Accept: text/jsonw"]);
    assert_eq!(r.status, 404);
    assert!(
        r.corpo.contains("mensagem:\"Rota não encontrada.\""),
        "{}",
        r.corpo
    );
    let r = pedir(&s, "/nao_existe");
    assert_eq!(r.status, 404);
    assert_eq!(r.tipo, "application/json; charset=utf-8"); // borda-json: navegador (a tela lê JSON)
}

/// O binário único com o subcomando da ferramenta (antes era um binário por ferramenta).
fn wortic(ferramenta: &str) -> Command {
    let mut comando = Command::new(env!("CARGO_BIN_EXE_wortic"));
    comando.arg(ferramenta);
    comando
}
