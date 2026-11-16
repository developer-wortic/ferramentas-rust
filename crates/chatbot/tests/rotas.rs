//! Rotas HTTP do servidor, contra o servidor falso do provedor. Nenhuma chamada sai da máquina.

mod comum;

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chatbot_prospeccao::configuracao::{ARQUIVO, Configuracao};
use chatbot_prospeccao::provedor::{Segredo, Tipo, Transporte};
use chatbot_prospeccao::roteiro::{EXEMPLO, Roteiro};
use chatbot_prospeccao::servidor;
use chatbot_prospeccao::servidor::{Estado, servir};
use comum::falso::{Falso, Resposta, eco_openai, subir};
use comum::fixtures::OPENAI_SSE;
use serde_json::{Value, json}; // borda-json: corpo das chamadas às rotas (a tela e o widget falam JSON)
use tiny_http::Server;

const CHAVE: &str = "chave-de-teste-123";

/// Resposta lida pelo cliente: status, corpo e cabeçalhos.
struct Lida {
    status: u16,
    corpo: String,
    cabecalhos: Vec<(String, String)>,
}

impl Lida {
    fn json(&self) -> Value {
        serde_json::from_str(&self.corpo).unwrap_or(Value::Null) // borda-json: resposta da rota
    }
    fn cabecalho(&self, nome: &str) -> Option<&str> {
        self.cabecalhos
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(nome))
            .map(|(_, v)| v.as_str())
    }
}

fn pasta(nome: &str) -> PathBuf {
    let caminho = std::env::temp_dir().join(format!("chatbot-rotas-{nome}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&caminho);
    fs::create_dir_all(&caminho).unwrap();
    caminho
}

fn configuracao(
    dados: PathBuf,
    roteiro: PathBuf,
    base: Option<String>,
    chave: Option<&str>,
) -> Configuracao {
    Configuracao {
        provedor: Tipo::OpenAi,
        modelo: Tipo::OpenAi.modelo_padrao().to_string(),
        chave: chave.map(Segredo::new),
        base,
        origens: vec!["https://site-do-cliente.exemplo".into()],
        porta: 0,
        endereco: "127.0.0.1".into(),
        dados,
        roteiro,
    }
}

/// Sobe o servidor do modelo numa porta livre e devolve o endereço base.
fn no_ar(configuracao: Configuracao, roteiro: Roteiro) -> (String, Arc<Estado>) {
    let mut estado = Estado::novo(configuracao, roteiro);
    estado.transporte = Transporte {
        tempo_limite: Duration::from_secs(5),
        reenvios: 0,
        pausa: Duration::ZERO,
    };
    let estado = Arc::new(estado);
    let servidor = Server::http("127.0.0.1:0").unwrap();
    let porta = servidor.server_addr().to_ip().unwrap().port();
    let para_o_fio = Arc::clone(&estado);
    std::thread::spawn(move || servir(&servidor, &para_o_fio));
    (format!("http://127.0.0.1:{porta}"), estado)
}

fn pedir(metodo: &str, url: &str, corpo: Option<&Value>, cabecalhos: &[(&str, &str)]) -> Lida {
    let mut pedido = ureq::request(metodo, url).timeout(Duration::from_secs(5));
    for (nome, valor) in cabecalhos {
        pedido = pedido.set(nome, valor);
    }
    let resultado = match corpo {
        Some(valor) => pedido.send_string(&valor.to_string()), // borda-json: corpo enviado à rota
        None => pedido.call(),
    };
    let resposta = match resultado {
        Ok(r) | Err(ureq::Error::Status(_, r)) => r,
        Err(outro) => panic!("falha de conexão: {outro}"),
    };
    let status = resposta.status();
    let cabecalhos = resposta
        .headers_names()
        .into_iter()
        .filter_map(|n| resposta.header(&n).map(|v| (n.clone(), v.to_string())))
        .collect();
    let corpo = resposta.into_string().unwrap_or_default();
    Lida {
        status,
        corpo,
        cabecalhos,
    }
}

fn roteiro_de_exemplo() -> Roteiro {
    Roteiro::ler(EXEMPLO).unwrap()
}

fn falso_com_eco() -> Falso {
    subir(|pedido, _| eco_openai(pedido))
}

#[test]
fn raiz_e_demo_servem_a_tela_so_para_a_maquina_local() {
    let (base, _) = no_ar(
        configuracao(pasta("tela"), pasta("tela").join("r.jsw.txt"), None, None),
        roteiro_de_exemplo(),
    );
    let raiz = pedir("GET", &format!("{base}/"), None, &[]);
    assert_eq!(raiz.status, 200);
    assert!(
        raiz.cabecalho("content-type")
            .unwrap()
            .starts_with("text/html")
    );
    let demo = pedir("GET", &format!("{base}/demo"), None, &[]);
    assert_eq!(demo.status, 200);
    let de_fora = pedir(
        "GET",
        &format!("{base}/"),
        None,
        &[("x-forwarded-for", "203.0.113.9")],
    );
    assert_eq!(de_fora.status, 403);
    assert_eq!(de_fora.json()["erro"], "so_local");
}

#[test]
fn widget_e_publico_e_sai_com_o_tipo_de_script() {
    let (base, _) = no_ar(
        configuracao(
            pasta("widget"),
            pasta("widget").join("r.jsw.txt"),
            None,
            None,
        ),
        roteiro_de_exemplo(),
    );
    let widget = pedir(
        "GET",
        &format!("{base}/widget.js"),
        None,
        &[("x-forwarded-for", "203.0.113.9")],
    );
    assert_eq!(widget.status, 200);
    assert!(
        widget
            .cabecalho("content-type")
            .unwrap()
            .contains("javascript")
    );
}

#[test]
fn config_diz_se_tem_chave_mas_nunca_mostra_a_chave() {
    let (base, _) = no_ar(
        configuracao(
            pasta("config"),
            pasta("config").join("r.jsw.txt"),
            None,
            Some(CHAVE),
        ),
        roteiro_de_exemplo(),
    );
    let c = pedir("GET", &format!("{base}/api/config"), None, &[]).json();
    assert_eq!(c["tem_chave"], true);
    assert_eq!(c["provedor"], "openai");
    assert_eq!(c["provedores"].as_array().unwrap().len(), 5);
    assert!(!c.to_string().contains(CHAVE));
}

#[test]
fn salvar_chave_grava_com_600_e_responde_sem_ecoar_a_chave() {
    use std::os::unix::fs::PermissionsExt;
    let dados = pasta("salvar");
    let (base, estado) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, None),
        roteiro_de_exemplo(),
    );
    let vazia = pedir(
        "POST",
        &format!("{base}/api/chave"),
        Some(&json!({"provedor": "groq", "chave": "  "})),
        &[],
    );
    assert_eq!(vazia.status, 400);
    assert_eq!(vazia.json()["erro"], "chave_vazia");
    let ruim = pedir(
        "POST",
        &format!("{base}/api/chave"),
        Some(&json!({"provedor": "nenhum", "chave": CHAVE})),
        &[],
    );
    assert_eq!(ruim.json()["erro"], "provedor_invalido");
    let ok = pedir(
        "POST",
        &format!("{base}/api/chave"),
        Some(&json!({"provedor": "groq", "chave": CHAVE})),
        &[],
    );
    assert_eq!(ok.status, 200);
    assert!(!ok.corpo.contains(CHAVE));
    let modo = fs::metadata(dados.join(ARQUIVO))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(modo, 0o600);
    let c = estado.configuracao.read().unwrap();
    assert_eq!(c.provedor, Tipo::Groq);
    assert_eq!(c.modelo, Tipo::Groq.modelo_padrao());
}

#[test]
fn salvar_chave_em_pasta_impossivel_responde_500() {
    let arquivo =
        std::env::temp_dir().join(format!("chatbot-rotas-arquivo-{}", std::process::id()));
    fs::write(&arquivo, "x").unwrap();
    let (base, _) = no_ar(
        configuracao(arquivo.clone(), arquivo.join("r.jsw.txt"), None, None),
        roteiro_de_exemplo(),
    );
    let r = pedir(
        "POST",
        &format!("{base}/api/chave"),
        Some(&json!({"provedor": "openai", "chave": CHAVE})),
        &[],
    );
    assert_eq!(r.status, 500);
    assert_eq!(r.json()["erro"], "gravacao");
    let _ = fs::remove_file(&arquivo);
}

#[test]
fn testar_chave_faz_uma_chamada_minima_e_mostra_o_resultado() {
    let falso = subir(|_, _| Resposta::sse(OPENAI_SSE));
    let dados = pasta("testar");
    let (base, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            None,
        ),
        roteiro_de_exemplo(),
    );
    let ok = pedir(
        "POST",
        &format!("{base}/api/testar"),
        Some(&json!({"provedor": "openai", "chave": CHAVE})),
        &[],
    )
    .json();
    assert_eq!(ok["ok"], true);
    assert_eq!(ok["resposta"], "Hi there!");
    assert_eq!(falso.recebidos(), 1);
    assert!(!ok.to_string().contains(CHAVE));
    assert!(!dados.join(ARQUIVO).exists(), "testar não grava nada");
}

#[test]
fn testar_usa_a_chave_salva_quando_a_tela_nao_manda_outra() {
    let falso = subir(|_, _| Resposta::sse(OPENAI_SSE));
    let dados = pasta("testar-salva");
    let (base, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            Some(CHAVE),
        ),
        roteiro_de_exemplo(),
    );
    let r = pedir("POST", &format!("{base}/api/testar"), Some(&json!({})), &[]).json();
    assert_eq!(r["ok"], true);
    assert_eq!(
        falso.ultimo().cabecalho("authorization"),
        Some("Bearer chave-de-teste-123")
    );
}

#[test]
fn testar_sem_chave_nenhuma_responde_400() {
    let dados = pasta("testar-vazio");
    let (base, _) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, None),
        roteiro_de_exemplo(),
    );
    let r = pedir("POST", &format!("{base}/api/testar"), Some(&json!({})), &[]);
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["erro"], "sem_chave");
}

#[test]
fn testar_com_chave_recusada_mostra_a_mensagem_sem_a_chave() {
    let falso = subir(|_, _| Resposta::com(401, "{}"));
    let dados = pasta("testar-recusada");
    let (base, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            None,
        ),
        roteiro_de_exemplo(),
    );
    let r = pedir(
        "POST",
        &format!("{base}/api/testar"),
        Some(&json!({"chave": CHAVE})),
        &[],
    )
    .json();
    assert_eq!(r["ok"], false);
    assert!(r["mensagem"].as_str().unwrap().contains("não aceitou"));
    assert!(!r.to_string().contains(CHAVE));
}

#[test]
fn roteiro_le_e_salva_com_validacao() {
    let dados = pasta("roteiro");
    let caminho = dados.join("roteiro.jsw.txt");
    fs::write(&caminho, EXEMPLO).unwrap();
    let (base, estado) = no_ar(
        configuracao(dados.clone(), caminho.clone(), None, None),
        roteiro_de_exemplo(),
    );
    let lido = pedir("GET", &format!("{base}/api/roteiro"), None, &[]).json();
    assert!(lido["jsonw"].as_str().unwrap().contains("Loja Exemplo"));

    let ruim = pedir(
        "PUT",
        &format!("{base}/api/roteiro"),
        Some(&json!({"jsonw": "empresa = 1"})),
        &[],
    );
    assert_eq!(ruim.status, 400);
    assert_eq!(ruim.json()["erro"], "roteiro_invalido");

    let novo = EXEMPLO.replace("Loja Exemplo", "Mercado Modelo");
    let ok = pedir(
        "POST",
        &format!("{base}/api/roteiro"),
        Some(&json!({"jsonw": novo})),
        &[],
    );
    assert_eq!(ok.status, 200);
    assert!(
        fs::read_to_string(&caminho)
            .unwrap()
            .contains("Mercado Modelo")
    );
    assert_eq!(
        estado.roteiro.read().unwrap().empresa.nome,
        "Mercado Modelo"
    );
}

#[test]
fn roteiro_sem_arquivo_mostra_o_de_exemplo_e_erro_de_gravacao_aparece() {
    let dados = pasta("roteiro-sem");
    let sumido = dados.join("nao-existe.jsw.txt");
    let (base, _) = no_ar(
        configuracao(dados.clone(), sumido, None, None),
        roteiro_de_exemplo(),
    );
    let lido = pedir("GET", &format!("{base}/api/roteiro"), None, &[]).json();
    assert!(lido["jsonw"].as_str().unwrap().contains("Loja Exemplo"));

    // a pasta do meio é um arquivo: a gravação não tem onde criar o roteiro e tem de responder 500
    fs::write(dados.join("bloqueio"), "x").unwrap();
    let caminho_que_nao_grava = dados.join("bloqueio").join("r.jsw.txt");
    let (base2, _) = no_ar(
        configuracao(dados.clone(), caminho_que_nao_grava, None, None),
        roteiro_de_exemplo(),
    );
    let r = pedir(
        "POST",
        &format!("{base2}/api/roteiro"),
        Some(&json!({"jsonw": EXEMPLO})),
        &[],
    );
    assert_eq!(r.status, 500);
    assert_eq!(r.json()["erro"], "gravacao");
}

#[test]
fn roteiro_que_nao_le_do_disco_responde_500() {
    let dados = pasta("roteiro-pasta");
    let pasta_no_lugar = dados.join("r.jsw.txt");
    fs::create_dir_all(&pasta_no_lugar).unwrap();
    let (base, _) = no_ar(
        configuracao(dados.clone(), pasta_no_lugar, None, None),
        roteiro_de_exemplo(),
    );
    let r = pedir("GET", &format!("{base}/api/roteiro"), None, &[]);
    assert_eq!(r.status, 500);
    assert_eq!(r.json()["erro"], "leitura");
}

#[test]
fn chat_responde_pela_sessao_e_cria_uma_nova_quando_nao_vem_id() {
    let falso = falso_com_eco();
    let dados = pasta("chat");
    let (base, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            Some(CHAVE),
        ),
        roteiro_de_exemplo(),
    );
    let primeira = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"mensagem": "Oi, tenho uma loja"})),
        &[],
    )
    .json();
    assert_eq!(primeira["resposta"], "Oi, tenho uma loja");
    assert_eq!(primeira["recusada"], false);
    let sessao = primeira["sessao"].as_str().unwrap().to_string();
    assert_eq!(sessao.len(), 32);
    let segunda = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"sessao": sessao, "mensagem": "Somos três pessoas"})),
        &[],
    )
    .json();
    assert_eq!(segunda["sessao"], sessao.as_str());
}

#[test]
fn chat_com_problema_responde_o_erro_certo() {
    let dados = pasta("chat-erros");
    let (sem_chave, _) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, None),
        roteiro_de_exemplo(),
    );
    let r = pedir(
        "POST",
        &format!("{sem_chave}/api/chat"),
        Some(&json!({"mensagem": "oi"})),
        &[],
    );
    assert_eq!(r.status, 503);
    assert_eq!(r.json()["erro"], "sem_chave");

    let falso = falso_com_eco();
    let (base, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            Some(CHAVE),
        ),
        roteiro_de_exemplo(),
    );
    let vazia = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"mensagem": "   "})),
        &[],
    );
    assert_eq!(vazia.status, 400);
    assert_eq!(vazia.json()["erro"], "mensagem_vazia");

    let quebrado = pedir("POST", &format!("{base}/api/chat"), None, &[]);
    assert_eq!(quebrado.status, 400);
    assert_eq!(quebrado.json()["erro"], "json_invalido");

    let falso_fora = subir(|_, _| Resposta::com(401, "{}"));
    let (base_fora, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso_fora.base)),
            Some(CHAVE),
        ),
        roteiro_de_exemplo(),
    );
    let fora = pedir(
        "POST",
        &format!("{base_fora}/api/chat"),
        Some(&json!({"mensagem": "oi"})),
        &[],
    );
    assert_eq!(fora.status, 503);
    assert!(
        !fora.corpo.contains(CHAVE),
        "a chave não vai para o visitante"
    );
}

#[test]
fn chat_com_sessao_cheia_responde_429() {
    let falso = falso_com_eco();
    let dados = pasta("chat-cheia");
    let (base, _) = {
        let mut estado = Estado::novo(
            configuracao(
                dados.clone(),
                dados.join("r.jsw.txt"),
                Some(format!("{}/v1", falso.base)),
                Some(CHAVE),
            ),
            roteiro_de_exemplo(),
        );
        estado.limites = chatbot_prospeccao::atendimento::Limites {
            mensagens_por_sessao: 2,
            sessoes: 10,
        };
        estado.transporte = Transporte {
            tempo_limite: Duration::from_secs(5),
            reenvios: 0,
            pausa: Duration::ZERO,
        };
        let estado = Arc::new(estado);
        let servidor = Server::http("127.0.0.1:0").unwrap();
        let porta = servidor.server_addr().to_ip().unwrap().port();
        let para_o_fio = Arc::clone(&estado);
        std::thread::spawn(move || servir(&servidor, &para_o_fio));
        (format!("http://127.0.0.1:{porta}"), estado)
    };
    let um = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"mensagem": "um"})),
        &[],
    )
    .json();
    let sessao = um["sessao"].as_str().unwrap().to_string();
    let r = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"sessao": sessao, "mensagem": "dois"})),
        &[],
    );
    assert_eq!(r.status, 429);
    assert_eq!(r.json()["erro"], "conversa_cheia");
}

#[test]
fn cors_so_para_origem_da_lista() {
    let falso = falso_com_eco();
    let dados = pasta("cors");
    let (base, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            Some(CHAVE),
        ),
        roteiro_de_exemplo(),
    );
    let permitida = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"mensagem": "oi"})),
        &[("origin", "https://site-do-cliente.exemplo")],
    );
    assert_eq!(
        permitida.cabecalho("access-control-allow-origin"),
        Some("https://site-do-cliente.exemplo")
    );
    let outra = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"mensagem": "oi"})),
        &[("origin", "https://mau.exemplo")],
    );
    assert_eq!(outra.cabecalho("access-control-allow-origin"), None);
    let previa = pedir(
        "OPTIONS",
        &format!("{base}/api/chat"),
        None,
        &[("origin", "https://site-do-cliente.exemplo")],
    );
    assert_eq!(previa.status, 204);
    assert_eq!(
        previa.cabecalho("access-control-allow-methods"),
        Some("POST, OPTIONS")
    );
    assert_eq!(
        previa.cabecalho("access-control-allow-origin"),
        Some("https://site-do-cliente.exemplo")
    );
}

#[test]
fn encerrar_gera_o_lead_guarda_na_lista_e_aparece_em_leads() {
    let resumo = r#"{"nome":"Maria Exemplo","contato":"contato@exemplo.com.br","interesse":"controle de estoque","nota":81,"motivo":"quer começar em breve"}"#;
    let falso = subir(move |pedido, _| {
        if pedido.json()["instructions"]
            .as_str()
            .unwrap_or("")
            .contains("resumo de um lead")
        {
            Resposta::sse(&[
                "event: response.output_text.delta\n",
                &format!(
                    "data: {}\n\n",
                    json!({"type": "response.output_text.delta", "delta": resumo})
                ), // borda-json: resumo no fio da OpenAI
                "event: response.completed\n",
                "data: {\"type\":\"response.completed\"}\n\n",
            ])
        } else {
            eco_openai(pedido)
        }
    });
    let dados = pasta("encerrar");
    let (base, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            Some(CHAVE),
        ),
        roteiro_de_exemplo(),
    );
    let conversa = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"mensagem": "Sou a Maria"})),
        &[],
    )
    .json();
    let sessao = conversa["sessao"].as_str().unwrap().to_string();
    let lead = pedir(
        "POST",
        &format!("{base}/api/encerrar"),
        Some(&json!({"sessao": sessao})),
        &[],
    );
    assert_eq!(lead.status, 200);
    assert_eq!(lead.json()["lead"]["nota"], 81);
    let lista = pedir("GET", &format!("{base}/api/leads"), None, &[]).json();
    assert_eq!(lista["leads"].as_array().unwrap().len(), 1);
    assert_eq!(lista["leads"][0]["nome"], "Maria Exemplo");
    assert!(
        !dados.join(ARQUIVO).exists()
            || !fs::read_to_string(dados.join(ARQUIVO))
                .unwrap()
                .contains("Maria")
    );
}

#[test]
fn encerrar_com_sessao_desconhecida_ou_resumo_ruim_responde_erro() {
    let falso = subir(|_, _| {
        Resposta::sse(&[
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"nao e objeto\"}\n\n",
        ])
    });
    let dados = pasta("encerrar-erros");
    let (base, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            Some(CHAVE),
        ),
        roteiro_de_exemplo(),
    );
    let inexistente = pedir(
        "POST",
        &format!("{base}/api/encerrar"),
        Some(&json!({"sessao": "nao"})),
        &[],
    );
    assert_eq!(inexistente.status, 404);
    assert_eq!(inexistente.json()["erro"], "conversa_nao_encontrada");

    let conversa = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"mensagem": "oi"})),
        &[],
    )
    .json();
    let sessao = conversa["sessao"].as_str().unwrap().to_string();
    let ruim = pedir(
        "POST",
        &format!("{base}/api/encerrar"),
        Some(&json!({"sessao": sessao})),
        &[],
    );
    assert_eq!(ruim.status, 502);
    assert_eq!(ruim.json()["erro"], "resumo");
}

#[test]
fn rotas_desconhecidas_e_metodos_errados_respondem_certo() {
    let dados = pasta("rotas");
    let (base, _) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, None),
        roteiro_de_exemplo(),
    );
    assert_eq!(pedir("GET", &format!("{base}/nada"), None, &[]).status, 404);
    let metodo = pedir("GET", &format!("{base}/api/chat"), None, &[]);
    assert_eq!(metodo.status, 405);
    assert_eq!(metodo.json()["erro"], "metodo");
}

// ---------- preparação, partida e corpo inválido em todas as rotas ----------

fn configuracao_para_partida(
    roteiro: PathBuf,
    chave: Option<&str>,
    endereco: &str,
) -> Configuracao {
    let mut c = configuracao(pasta("partida"), roteiro, None, chave);
    c.porta = 0;
    c.endereco = endereco.to_string();
    c
}

#[test]
fn preparar_avisa_roteiro_de_exemplo_sem_chave_e_rede() {
    let sumido = pasta("partida-avisos").join("nao-existe.jsw.txt");
    let preparado = servidor::preparar(configuracao_para_partida(sumido, None, "0.0.0.0")).unwrap();
    let todos = preparado.avisos.join("\n");
    assert!(todos.contains("no ar"), "{todos}");
    assert!(todos.contains("roteiro de exemplo"), "{todos}");
    assert!(todos.contains("sem chave"), "{todos}");
    assert!(todos.contains("aberto na rede"), "{todos}");
    preparado.servidor.unblock();
}

#[test]
fn preparar_com_chave_e_local_nao_avisa_nada_disso() {
    let dados = pasta("partida-limpa");
    fs::write(dados.join("roteiro.jsw.txt"), EXEMPLO).unwrap();
    let preparado = servidor::preparar(configuracao_para_partida(
        dados.join("roteiro.jsw.txt"),
        Some(CHAVE),
        "127.0.0.1",
    ))
    .unwrap();
    assert_eq!(preparado.avisos.len(), 1, "{:?}", preparado.avisos);
    preparado.servidor.unblock();
}

#[test]
fn preparar_recusa_roteiro_que_nao_lê_e_porta_ocupada() {
    let dados = pasta("partida-ruim");
    let pasta_no_lugar = dados.join("roteiro.jsw.txt");
    fs::create_dir_all(&pasta_no_lugar).unwrap();
    let erro = servidor::preparar(configuracao_para_partida(pasta_no_lugar, None, "127.0.0.1"))
        .err()
        .expect("roteiro que não lê");
    assert!(erro.contains("não consegui ler o roteiro"), "{erro}");

    let ocupada = Server::http("127.0.0.1:0").unwrap();
    let porta = ocupada.server_addr().to_ip().unwrap().port();
    fs::write(dados.join("ok.jsw.txt"), EXEMPLO).unwrap();
    let mut c = configuracao_para_partida(dados.join("ok.jsw.txt"), None, "127.0.0.1");
    c.porta = porta;
    let erro = servidor::preparar(c).err().expect("porta ocupada");
    assert!(erro.contains("não consegui abrir a porta"), "{erro}");
}

#[test]
fn iniciar_imprime_os_avisos_e_volta_quando_o_servidor_para() {
    let dados = pasta("iniciar");
    let preparado = servidor::preparar(configuracao_para_partida(
        dados.join("nao.jsw.txt"),
        None,
        "127.0.0.1",
    ))
    .unwrap();
    preparado.servidor.unblock();
    assert_eq!(
        servidor::iniciar(preparado),
        std::process::ExitCode::SUCCESS
    );
}

#[test]
fn principal_devolve_falha_para_configuracao_ruim_e_nao_serve() {
    let ambiente = vec![
        (
            "CHATBOT_DADOS".to_string(),
            pasta("principal").to_string_lossy().to_string(),
        ),
        ("PORTA".to_string(), "abc".to_string()),
    ];
    assert_eq!(
        servidor::principal(ambiente),
        std::process::ExitCode::FAILURE
    );
}

#[test]
fn corpo_que_nao_e_json_em_cada_rota_responde_400() {
    let dados = pasta("corpo-ruim");
    let (base, _) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, Some(CHAVE)),
        roteiro_de_exemplo(),
    );
    for rota in [
        "/api/chave",
        "/api/testar",
        "/api/roteiro",
        "/api/chat",
        "/api/encerrar",
    ] {
        let r = ureq::post(&format!("{base}{rota}"))
            .set("content-type", "application/json")
            .send_string("{ isto não é json");
        let r = match r {
            Ok(ok) => ok,
            Err(ureq::Error::Status(_, resposta)) => resposta,
            Err(outro) => panic!("{outro}"),
        };
        assert_eq!(r.status(), 400, "{rota}");
    }
}

#[test]
fn corpo_com_bytes_invalidos_responde_corpo_ilegivel() {
    let dados = pasta("corpo-bytes");
    let (base, _) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, Some(CHAVE)),
        roteiro_de_exemplo(),
    );
    let r = ureq::post(&format!("{base}/api/chat")).send_bytes(&[0xff, 0xfe, 0xfd]);
    let r = match r {
        Ok(ok) => ok,
        Err(ureq::Error::Status(_, resposta)) => resposta,
        Err(outro) => panic!("{outro}"),
    };
    assert_eq!(r.status(), 400);
    assert!(r.into_string().unwrap().contains("corpo_ilegivel"));
}

#[test]
fn provedor_que_falha_de_outro_jeito_responde_aviso_generico() {
    let falso = subir(|_, _| Resposta::com(500, "{}"));
    let dados = pasta("generico");
    let (base, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            Some(CHAVE),
        ),
        roteiro_de_exemplo(),
    );
    let r = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"mensagem": "oi"})),
        &[],
    );
    assert_eq!(r.status, 503);
    assert!(
        r.json()["mensagem"]
            .as_str()
            .unwrap()
            .contains("indisponível")
    );
}

#[test]
fn encerrar_sem_chave_responde_503_e_nao_chama_nada() {
    let dados = pasta("encerrar-sem-chave");
    let (base, _) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, None),
        roteiro_de_exemplo(),
    );
    let r = pedir(
        "POST",
        &format!("{base}/api/encerrar"),
        Some(&json!({"sessao": "x"})),
        &[],
    );
    assert_eq!(r.status, 503);
    assert_eq!(r.json()["erro"], "sem_chave");
}

#[test]
fn salvar_e_testar_aceitam_modelo_informado_na_tela() {
    let falso = subir(|_, _| Resposta::sse(OPENAI_SSE));
    let dados = pasta("modelo-informado");
    let (base, estado) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            None,
        ),
        roteiro_de_exemplo(),
    );
    let teste = pedir(
        "POST",
        &format!("{base}/api/testar"),
        Some(&json!({"provedor": "openai", "chave": CHAVE, "modelo": "gpt-5.5"})),
        &[],
    )
    .json();
    assert_eq!(teste["ok"], true);
    assert_eq!(falso.ultimo().json()["model"], "gpt-5.5");
    let salvo = pedir(
        "POST",
        &format!("{base}/api/chave"),
        Some(&json!({"provedor": "openai", "chave": CHAVE, "modelo": "gpt-5.5"})),
        &[],
    );
    assert_eq!(salvo.status, 200);
    assert_eq!(estado.configuracao.read().unwrap().modelo, "gpt-5.5");
}

// ---------- formato: jsonW-texto e arquivo .jsw pelo pedido ----------

/// Sobe o servidor com um provedor que devolve o resumo de lead (Maria, nota 81) e encerra uma conversa.
/// Devolve o endereço base e a pasta de dados.
fn servidor_com_um_lead(nome: &str) -> (String, PathBuf) {
    let resumo = r#"{"nome":"Maria Exemplo","contato":"contato@exemplo.com.br","interesse":"controle de estoque","nota":81,"motivo":"quer começar em breve"}"#;
    let falso = subir(move |pedido, _| {
        if pedido.json()["instructions"]
            .as_str()
            .unwrap_or("")
            .contains("resumo de um lead")
        {
            Resposta::sse(&[
                "event: response.output_text.delta\n",
                &format!(
                    "data: {}\n\n",
                    json!({"type": "response.output_text.delta", "delta": resumo})
                ), // borda-json: resumo no fio da OpenAI
                "event: response.completed\n",
                "data: {\"type\":\"response.completed\"}\n\n",
            ])
        } else {
            eco_openai(pedido)
        }
    });
    let dados = pasta(nome);
    let (base, _) = no_ar(
        configuracao(
            dados.clone(),
            dados.join("r.jsw.txt"),
            Some(format!("{}/v1", falso.base)),
            Some(CHAVE),
        ),
        roteiro_de_exemplo(),
    );
    let conversa = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"mensagem": "Sou a Maria"})),
        &[],
    )
    .json();
    let sessao = conversa["sessao"].as_str().unwrap().to_string();
    let lead = pedir(
        "POST",
        &format!("{base}/api/encerrar"),
        Some(&json!({"sessao": sessao})),
        &[],
    );
    assert_eq!(lead.status, 200);
    // o lead já está guardado no processo: o provedor falso não é mais chamado e pode sair daqui
    drop(falso);
    (base, dados)
}

#[test]
fn roteiro_sai_como_texto_jsonw_quando_pedido_e_como_json_na_borda() {
    let dados = pasta("roteiro-forma");
    let caminho = dados.join("roteiro.jsw.txt");
    fs::write(&caminho, EXEMPLO).unwrap();
    let (base, _) = no_ar(
        configuracao(dados.clone(), caminho, None, None),
        roteiro_de_exemplo(),
    );
    let texto = pedir(
        "GET",
        &format!("{base}/api/roteiro"),
        None,
        &[("accept", "text/jsonw")],
    );
    assert_eq!(texto.status, 200);
    assert!(
        texto
            .cabecalho("content-type")
            .unwrap()
            .starts_with("text/jsonw")
    );
    assert!(
        texto.corpo.starts_with("// Roteiro de prospecção"),
        "{:.80}",
        texto.corpo
    );
    assert!(texto.corpo.contains("nome: \"Loja Exemplo\""));
    assert!(
        !texto.corpo.contains("\"nome\":"),
        "chave sem aspas no jsonW"
    );

    let por_consulta = pedir(
        "GET",
        &format!("{base}/api/roteiro?formato=jsonw"),
        None,
        &[],
    );
    assert_eq!(por_consulta.corpo, texto.corpo);

    let json = pedir("GET", &format!("{base}/api/roteiro"), None, &[]);
    assert!(
        json.cabecalho("content-type")
            .unwrap()
            .starts_with("application/json")
    );
    assert!(
        json.json()["jsonw"]
            .as_str()
            .unwrap()
            .contains("Loja Exemplo")
    );
}

#[test]
fn roteiro_recusado_em_jsonw_diz_o_motivo_no_mesmo_formato() {
    let dados = pasta("roteiro-recusa-texto");
    let (base, _) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, None),
        roteiro_de_exemplo(),
    );
    let ruim = pedir(
        "POST",
        &format!("{base}/api/roteiro?formato=jsonw"),
        Some(&json!({"jsonw": "empresa = 1"})),
        &[],
    );
    assert_eq!(ruim.status, 400);
    assert!(
        ruim.cabecalho("content-type")
            .unwrap()
            .starts_with("text/jsonw")
    );
    assert!(
        ruim.corpo.contains("erro:\"roteiro_invalido\""),
        "{}",
        ruim.corpo
    );
    assert!(ruim.corpo.contains("roteiro inválido"), "{}", ruim.corpo);
}

#[test]
fn erro_da_api_sai_em_jsonw_quando_o_pedido_pede() {
    let dados = pasta("erro-jsonw");
    let (base, _) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, None),
        roteiro_de_exemplo(),
    );
    let r = pedir(
        "POST",
        &format!("{base}/api/chat"),
        Some(&json!({"mensagem": "oi"})),
        &[("accept", "text/jsonw")],
    );
    assert_eq!(r.status, 503);
    assert!(
        r.cabecalho("content-type")
            .unwrap()
            .starts_with("text/jsonw")
    );
    assert!(r.corpo.contains("erro:\"sem_chave\""), "{}", r.corpo);
}

#[test]
fn leads_em_lista_jsonw_e_em_arquivo_jsw_lidos_de_volta() {
    let (base, _) = servidor_com_um_lead("leads-jsw");

    let texto = pedir("GET", &format!("{base}/api/leads?formato=jsonw"), None, &[]);
    assert!(
        texto
            .cabecalho("content-type")
            .unwrap()
            .starts_with("text/jsonw")
    );
    assert!(texto.corpo.contains("leads:[{"), "{}", texto.corpo);
    assert!(texto.corpo.contains("nome:\"Maria Exemplo\""));

    let arquivo_resposta = ureq::get(&format!("{base}/api/leads"))
        .set("accept", "application/jsonw")
        .timeout(Duration::from_secs(5))
        .call()
        .unwrap();
    assert_eq!(arquivo_resposta.content_type(), "application/jsonw");
    assert!(
        arquivo_resposta
            .header("content-disposition")
            .unwrap()
            .contains("leads.jsw")
    );
    let mut bytes = Vec::new();
    arquivo_resposta
        .into_reader()
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.starts_with(b"JSW1"), "marca do .jsw");

    let caminho = pasta("leads-jsw-arquivo").join("leads.jsw");
    fs::write(&caminho, &bytes).unwrap();
    let linhas = wortic_base::arquivo::ler(&caminho, true).unwrap();
    assert_eq!(linhas.len(), 1);
    assert!(
        linhas[0].contains("nome:\"Maria Exemplo\""),
        "{}",
        linhas[0]
    );
    assert!(linhas[0].contains("nota:81"), "{}", linhas[0]);
    assert!(linhas[0].contains("criado_em:"), "{}", linhas[0]);
    let em_json = wortic_base::arquivo::ler(&caminho, false).unwrap();
    let registro: Value = serde_json::from_str(&em_json[0]).unwrap(); // borda-json: confere a volta
    assert_eq!(registro["nota"], 81);
    assert_eq!(registro["nome"], "Maria Exemplo");
}

#[test]
fn leads_com_formato_jsw_pela_consulta_tambem_sai_como_arquivo() {
    let (base, _) = servidor_com_um_lead("leads-jsw-consulta");
    let r = ureq::get(&format!("{base}/api/leads?formato=jsw"))
        .timeout(Duration::from_secs(5))
        .call()
        .unwrap();
    assert_eq!(r.content_type(), "application/jsonw");
    let mut bytes = Vec::new();
    r.into_reader().read_to_end(&mut bytes).unwrap();
    assert!(bytes.starts_with(b"JSW1"));
}

#[test]
fn lista_de_leads_vazia_em_jsw_responde_404_sem_leads() {
    let dados = pasta("leads-vazia");
    let (base, _) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, None),
        roteiro_de_exemplo(),
    );
    let r = pedir("GET", &format!("{base}/api/leads?formato=jsw"), None, &[]);
    assert_eq!(r.status, 404);
    assert!(r.corpo.contains("erro:\"sem_leads\""), "{}", r.corpo);
    let vazia = pedir("GET", &format!("{base}/api/leads"), None, &[]).json();
    assert_eq!(vazia["leads"].as_array().unwrap().len(), 0);
}

#[test]
fn rota_sem_formato_mantem_o_json_do_navegador() {
    let dados = pasta("borda-json");
    let (base, _) = no_ar(
        configuracao(dados.clone(), dados.join("r.jsw.txt"), None, Some(CHAVE)),
        roteiro_de_exemplo(),
    );
    let c = pedir(
        "GET",
        &format!("{base}/api/config"),
        None,
        &[("accept", "text/html")],
    );
    assert!(
        c.cabecalho("content-type")
            .unwrap()
            .starts_with("application/json")
    );
    assert_eq!(c.json()["tem_chave"], true);
}
