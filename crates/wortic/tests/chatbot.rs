//! Roda o binário de verdade. Nada sai da máquina. A porta é a que o sistema escolhe (PORTA=0),
//! lida do aviso de partida: assim nenhum teste disputa porta com outro.

#[path = "../../chatbot/tests/comum/mod.rs"]
mod comum;

use std::io::{BufRead, BufReader};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use comum::falso::{eco_openai, subir};
use serde_json::json; // borda-json: corpo enviado ao binário

/// Pasta temporária única para cada chamada, para os testes não pisarem uns nos outros.
fn pasta(nome: &str) -> PathBuf {
    let agora = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let caminho =
        std::env::temp_dir().join(format!("chatbot-bin-{nome}-{}-{agora}", std::process::id()));
    std::fs::create_dir_all(&caminho).unwrap();
    caminho
}

/// Variáveis herdadas do ambiente de teste, sem as do modelo (CHATBOT_*, PORTA, ENDERECO).
/// Mantém `LLVM_PROFILE_FILE`, para a cobertura também contar o binário.
fn herdado() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(k, _)| !k.starts_with("CHATBOT_") && k != "PORTA" && k != "ENDERECO")
        .collect()
}

/// Sobe o binário com ambiente limpo, porta 0 e as variáveis dadas. Devolve o processo e as linhas do stdout.
fn subir_binario(ambiente: &[(&str, String)]) -> (Child, Receiver<String>) {
    subir_em(&pasta("dados"), ambiente)
}

/// Como `subir_binario`, numa pasta de dados escolhida (para testar a migração na partida).
fn subir_em(dados: &std::path::Path, ambiente: &[(&str, String)]) -> (Child, Receiver<String>) {
    let mut comando = wortic("chatbot");
    comando
        .env_clear()
        .envs(herdado())
        .env("CHATBOT_DADOS", dados.to_string_lossy().to_string())
        .env("CHATBOT_ROTEIRO", "roteiro-que-nao-existe.jsw.txt")
        .env("PORTA", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
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

/// Espera o aviso de partida e tira dele a porta que o sistema abriu.
fn porta_anunciada(linhas: &Receiver<String>) -> u16 {
    let inicio = Instant::now();
    while inicio.elapsed() < Duration::from_secs(10) {
        if let Ok(linha) = linhas.recv_timeout(Duration::from_millis(200))
            && linha.contains("no ar em http://")
        {
            let porta = linha.trim_end_matches('/').rsplit(':').next().unwrap();
            return porta.parse().expect("porta no aviso");
        }
    }
    panic!("o binário não anunciou a porta");
}

fn linhas_de_partida(linhas: &Receiver<String>) -> String {
    (0..3)
        .filter_map(|_| linhas.recv_timeout(Duration::from_secs(10)).ok())
        .collect::<Vec<String>>()
        .join("\n")
}

#[test]
fn porta_invalida_sai_com_erro_de_configuracao() {
    let saida = wortic("chatbot")
        .env_clear()
        .envs(herdado())
        .env(
            "CHATBOT_DADOS",
            pasta("porta").to_string_lossy().to_string(),
        )
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
fn provedor_desconhecido_sai_com_erro_de_configuracao() {
    let saida = wortic("chatbot")
        .env_clear()
        .envs(herdado())
        .env(
            "CHATBOT_DADOS",
            pasta("provedor").to_string_lossy().to_string(),
        )
        .env("CHATBOT_PROVEDOR", "nenhum")
        .output()
        .expect("roda o binário");
    assert!(!saida.status.success());
    assert!(String::from_utf8_lossy(&saida.stderr).contains("provedor desconhecido"));
}

#[test]
fn porta_ocupada_sai_com_erro_ao_abrir() {
    let ocupada = TcpListener::bind("127.0.0.1:0").unwrap();
    let porta = ocupada.local_addr().unwrap().port();
    let saida = wortic("chatbot")
        .env_clear()
        .envs(herdado())
        .env(
            "CHATBOT_DADOS",
            pasta("ocupada").to_string_lossy().to_string(),
        )
        .env("CHATBOT_ROTEIRO", "roteiro-que-nao-existe.jsw.txt")
        .env("PORTA", porta.to_string())
        .output()
        .expect("roda o binário");
    assert!(!saida.status.success());
    assert!(String::from_utf8_lossy(&saida.stderr).contains("não consegui abrir a porta"));
}

#[test]
fn roteiro_que_nao_le_sai_com_erro() {
    let dados = pasta("roteiro-ruim");
    let ruim = dados.join("ruim.jsw.txt");
    std::fs::write(&ruim, "isto = [").unwrap();
    let saida = wortic("chatbot")
        .env_clear()
        .envs(herdado())
        .env("CHATBOT_DADOS", dados.to_string_lossy().to_string())
        .env("CHATBOT_ROTEIRO", ruim.to_string_lossy().to_string())
        .env("PORTA", "0")
        .output()
        .expect("roda o binário");
    assert!(!saida.status.success());
    assert!(String::from_utf8_lossy(&saida.stderr).contains("roteiro inválido"));
}

#[test]
fn binario_sobe_e_avisa_que_nao_tem_chave_e_que_usa_o_roteiro_de_exemplo() {
    let (mut filho, linhas) = subir_binario(&[]);
    let todas = linhas_de_partida(&linhas);
    let _ = filho.kill();
    let _ = filho.wait();
    assert!(todas.contains("no ar"), "{todas}");
    assert!(todas.contains("sem chave"), "{todas}");
    assert!(todas.contains("roteiro de exemplo"), "{todas}");
}

#[test]
fn binario_responde_a_tela_e_o_chat_com_o_provedor_falso() {
    let falso = subir(|pedido, _| eco_openai(pedido));
    let (mut filho, linhas) = subir_binario(&[
        ("CHATBOT_CHAVE", "chave-de-teste-123".into()),
        ("CHATBOT_BASE_URL", format!("{}/v1", falso.base)),
        ("ENDERECO", "0.0.0.0".into()),
    ]);
    let porta = porta_anunciada(&linhas);
    let url = format!("http://127.0.0.1:{porta}");
    let tela = ureq::get(&format!("{url}/"))
        .timeout(Duration::from_secs(5))
        .call()
        .unwrap()
        .into_string()
        .unwrap();
    let chat = ureq::post(&format!("{url}/api/chat"))
        .timeout(Duration::from_secs(5))
        .send_string(&json!({"mensagem": "Oi, quero conhecer o sistema"}).to_string()); // borda-json: corpo enviado ao chat
    let resposta = chat.expect("o chat responde").into_string().unwrap();
    let _ = filho.kill();
    let _ = filho.wait();
    assert!(tela.contains("<!doctype html"), "tela: {tela:.80}");
    assert!(
        resposta.contains("Oi, quero conhecer o sistema"),
        "chat: {resposta}"
    );
    assert_eq!(falso.recebidos(), 1);
}

#[test]
fn saida_com_aviso_de_rede_quando_endereco_nao_e_local() {
    let (mut filho, linhas) = subir_binario(&[
        ("ENDERECO", "0.0.0.0".into()),
        ("CHATBOT_CHAVE", "chave-de-teste-123".into()),
    ]);
    let todas = linhas_de_partida(&linhas);
    let _ = filho.kill();
    let _ = filho.wait();
    assert!(todas.contains("no ar"), "{todas}");
    assert!(todas.contains("aberto na rede"), "{todas}");
    assert!(!todas.contains("sem chave"), "{todas}");
}

/// Lê as linhas da partida até o aviso de "no ar" (que sai depois dos avisos de migração).
fn linhas_ate_no_ar(linhas: &Receiver<String>) -> String {
    let inicio = Instant::now();
    let mut todas = String::new();
    while inicio.elapsed() < Duration::from_secs(10) {
        if let Ok(linha) = linhas.recv_timeout(Duration::from_millis(200)) {
            let no_ar = linha.contains("no ar em");
            todas.push_str(&linha);
            todas.push('\n');
            if no_ar {
                break;
            }
        }
    }
    todas
}

#[test]
fn roteiro_exemplo_imprime_o_jsonw_com_os_comentarios() {
    let saida = wortic("chatbot")
        .args(["roteiro", "exemplo"])
        .output()
        .expect("roda o binário");
    assert!(saida.status.success());
    let texto = String::from_utf8_lossy(&saida.stdout);
    assert!(texto.starts_with("// Roteiro de prospecção"), "{texto:.80}");
    assert!(texto.contains("nome: \"Loja Exemplo\""), "{texto}");
    assert!(!texto.contains("\"nome\":"), "chave sem aspas: {texto}");
}

#[test]
fn roteiro_conferir_aceita_o_exemplo_e_recusa_o_ruim_com_saida_1() {
    let dados = pasta("conferir");
    let bom = dados.join("bom.jsw.txt");
    let exemplo = wortic("chatbot")
        .args(["roteiro", "exemplo"])
        .output()
        .expect("roda o binário");
    std::fs::write(&bom, &exemplo.stdout).unwrap();
    let ok = wortic("chatbot")
        .args(["roteiro", "conferir"])
        .arg(&bom)
        .output()
        .expect("roda o binário");
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert!(String::from_utf8_lossy(&ok.stdout).starts_with("ok:"));

    let ruim = dados.join("ruim.jsw.txt");
    std::fs::write(&ruim, "{empresa: {nome: 7}}").unwrap();
    let erro = wortic("chatbot")
        .args(["roteiro", "conferir"])
        .arg(&ruim)
        .output()
        .expect("roda o binário");
    assert_eq!(erro.status.code(), Some(1));
    let motivo = String::from_utf8_lossy(&erro.stderr);
    assert!(motivo.contains("roteiro inválido"), "{motivo}");
}

#[test]
fn roteiro_sem_arquivo_ou_comando_errado_sai_com_2() {
    let sem_arquivo = wortic("chatbot")
        .args(["roteiro", "conferir"])
        .output()
        .expect("roda o binário");
    assert_eq!(sem_arquivo.status.code(), Some(2));
    let outro = wortic("chatbot")
        .args(["roteiro", "apagar"])
        .output()
        .expect("roda o binário");
    assert_eq!(outro.status.code(), Some(2));
    let inexistente = wortic("chatbot")
        .args(["roteiro", "conferir", "nao-existe.jsw.txt"])
        .output()
        .expect("roda o binário");
    assert_eq!(inexistente.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&inexistente.stderr).contains("não consegui ler"));
}

#[test]
fn migrar_converte_a_pasta_uma_vez_e_depois_diz_que_nao_ha_nada() {
    let raiz = pasta("migrar");
    std::fs::write(
        raiz.join("roteiro.toml"),
        "[empresa]\nnome = \"Padaria Antiga\"\no_que_vende = \"Caixa\"\npara_quem = \"Padarias\"\n\n[qualificacao]\nperguntas = [\"Quantas pessoas?\"]\n\n[humano]\na_partir_da_nota = 70\nquando = [\"pedir uma pessoa\"]\n",
    )
    .unwrap();
    std::fs::create_dir_all(raiz.join("dados")).unwrap();
    std::fs::write(
        raiz.join("dados").join("configuracao.toml"),
        "provedor = \"groq\"\nchave = \"chave-de-teste-987\"\n",
    )
    .unwrap();
    let primeira = wortic("chatbot")
        .arg("migrar")
        .arg(&raiz)
        .output()
        .expect("roda o binário");
    assert!(
        primeira.status.success(),
        "{}",
        String::from_utf8_lossy(&primeira.stderr)
    );
    let avisos = String::from_utf8_lossy(&primeira.stdout);
    assert_eq!(avisos.matches("migrei").count(), 2, "{avisos}");
    assert!(
        !avisos.contains("chave-de-teste-987"),
        "a chave não sai no aviso"
    );
    assert!(raiz.join("roteiro.jsw.txt").exists());
    assert!(raiz.join("roteiro.toml.antes").exists());
    assert!(raiz.join("dados").join("configuracao.jsw.txt").exists());

    let segunda = wortic("chatbot")
        .arg("migrar")
        .arg(&raiz)
        .output()
        .expect("roda o binário");
    assert!(String::from_utf8_lossy(&segunda.stdout).contains("nada para migrar"));
}

#[test]
fn leads_le_o_jsw_em_jsonw_e_em_json_para_quem_vem_de_fora() {
    let dados = pasta("leads");
    let arquivo = dados.join("leads.jsw");
    let registro = r#"{"nome":"Maria Exemplo","contato":"contato@exemplo.com.br","interesse":"estoque","nota":81,"motivo":"tem prazo","criado_em":"2000-02-29T00:00:00Z"}"#;
    wortic_base::arquivo::gravar(&arquivo, chatbot_prospeccao::lead::ESQUEMA, [registro]).unwrap();

    let texto = wortic("chatbot")
        .arg("leads")
        .arg(&arquivo)
        .output()
        .expect("roda o binário");
    assert!(
        texto.status.success(),
        "{}",
        String::from_utf8_lossy(&texto.stderr)
    );
    let linha = String::from_utf8_lossy(&texto.stdout);
    assert!(linha.contains("nome:\"Maria Exemplo\""), "{linha}");
    assert!(linha.contains("nota:81"), "{linha}");

    let em_json = wortic("chatbot")
        .arg("leads")
        .arg(&arquivo)
        .arg("--json")
        .output()
        .expect("roda o binário");
    let registro_json: serde_json::Value =
        serde_json::from_slice(&em_json.stdout).expect("a linha sai em JSON"); // borda-json: confere a saída de quem vem de fora
    assert_eq!(registro_json["nome"], "Maria Exemplo");
    assert_eq!(registro_json["nota"], 81);

    let sem = wortic("chatbot")
        .arg("leads")
        .output()
        .expect("roda o binário");
    assert_eq!(sem.status.code(), Some(2));
}

#[test]
fn ajuda_lista_todos_os_comandos() {
    let saida = wortic("chatbot")
        .arg("ajuda")
        .output()
        .expect("roda o binário");
    let texto = String::from_utf8_lossy(&saida.stdout);
    for comando in [
        "servir",
        "roteiro exemplo",
        "roteiro conferir",
        "migrar",
        "leads",
    ] {
        assert!(texto.contains(comando), "{texto}");
    }
    let desconhecido = wortic("chatbot")
        .arg("nada")
        .output()
        .expect("roda o binário");
    assert_eq!(desconhecido.status.code(), Some(2));
}

#[test]
fn partida_migra_a_configuracao_antiga_e_sobe_com_a_chave() {
    let dados = pasta("partida-migra");
    std::fs::write(
        dados.join("configuracao.toml"),
        "provedor = \"groq\"\nchave = \"chave-de-teste-555\"\n",
    )
    .unwrap();
    let (mut filho, linhas) = subir_em(&dados, &[]);
    let todas = linhas_ate_no_ar(&linhas);
    let _ = filho.kill();
    let _ = filho.wait();
    assert!(todas.contains("migrei"), "{todas}");
    assert!(
        !todas.contains("chave-de-teste-555"),
        "a chave não sai no log"
    );
    assert!(
        !todas.contains("sem chave"),
        "a chave migrada vale: {todas}"
    );
    assert!(dados.join("configuracao.jsw.txt").exists());
    assert!(dados.join("configuracao.toml.antes").exists());
}

#[test]
fn migrar_com_toml_invalido_sai_com_1_e_diz_o_arquivo() {
    let raiz = pasta("migrar-ruim");
    std::fs::write(raiz.join("roteiro.toml"), "empresa = [").unwrap();
    let saida = wortic("chatbot")
        .arg("migrar")
        .arg(&raiz)
        .output()
        .expect("roda o binário");
    assert_eq!(saida.status.code(), Some(1));
    let erro = String::from_utf8_lossy(&saida.stderr);
    assert!(
        erro.contains("roteiro.toml") && erro.contains("não é TOML válido"),
        "{erro}"
    );
}

#[test]
fn leads_com_arquivo_que_nao_e_jsw_sai_com_1() {
    let dados = pasta("leads-ruim");
    let arquivo = dados.join("nao-e-jsw.jsw");
    std::fs::write(&arquivo, "isto não é um arquivo jsonW").unwrap();
    let saida = wortic("chatbot")
        .arg("leads")
        .arg(&arquivo)
        .output()
        .expect("roda o binário");
    assert_eq!(saida.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&saida.stderr).contains("nao-e-jsw.jsw"));
}

/// O binário único com o subcomando da ferramenta (antes era um binário por ferramenta).
fn wortic(ferramenta: &str) -> Command {
    let mut comando = Command::new(env!("CARGO_BIN_EXE_wortic"));
    comando.arg(ferramenta);
    comando
}
