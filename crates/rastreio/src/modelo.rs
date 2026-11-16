//! O modelo: um servidor HTTP pequeno, com o pixel primário, a rota de venda, o consentimento, a exportação e a tela.
//!
//! Rotas:
//! - `GET /r?u=<página>`: pixel primário. Com consentimento, grava a origem (UTM e clids) e o cookie de atribuição.
//! - `POST /evento`: recebe uma venda (formulário, JSON ou jsonW-texto) e monta o evento da Conversions API.
//!   Envia só com Pixel e token.
//! - `GET /consentimento?aceitar=sim|nao`: grava a escolha do visitante.
//! - `GET /registros/eventos` e `GET /registros/origens`: a lista guardada, em JSON, jsonW-texto ou arquivo `.jsw`.
//! - `GET /`: a tela, com as últimas origens e eventos.
//!
//! Formato da resposta: JSON por padrão (quem chama de fora). `?formato=jsonw` ou `Accept: text/jsonw` pedem
//! jsonW-texto; `?formato=jsw` ou `Accept: application/jsonw` pedem o arquivo `.jsw` (nas listas).
//!
//! Tudo fica em memória. O que passa do limite sai, e parar o programa apaga tudo, salvo com `RASTREIO_ARQUIVO`, que
//! grava os registros em arquivo `.jsw` a cada `RASTREIO_ARQUIVO_CADA` registros e ao sair pelo limite de pedidos.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use tiny_http::{Header, Method, Request, Response, Server};
use wortic_base::Formato;
use wortic_base::arquivo;
use wortic_base::formato::{TIPO_JSON, parametro}; // borda-json: tipo do JSON que sai para quem chama de fora

use crate::atribuicao::Atribuicao;
use crate::captura::{self, Captura, Plataforma};
use crate::consentimento;
use crate::cookie::{self, Cookie};
use crate::dados::Dados;
use crate::envio::{Config, Envio, Resultado};
use crate::esquemas;
use crate::evento::{self, Entrada, Evento};
use crate::exportar;
use crate::identificadores;
use crate::jsonw::{self, obj, texto};
use crate::painel::{self, Situacao};
use crate::saida;
use crate::venda::{self, buscar};

/// O cookie que guarda a escolha de consentimento do visitante.
pub const CONSENTIMENTO: &str = "rc_consent";
const LIMITE_CORPO: u64 = 16_384;
const LINHAS: usize = 4;
const GIF: &[u8] = &[
    0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x01, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0xff, 0xff, 0xff,
    0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44, 0x01, 0x00, 0x3b,
];
const DDI_PADRAO: &str = "55";

type Resp = Response<Cursor<Vec<u8>>>;

/// O que o modelo lê do ambiente. Segredo e token não aparecem em `Debug`, porque não há `Debug` aqui.
#[derive(Clone)]
pub struct Ajustes {
    /// O endereço de escuta (`127.0.0.1` por padrão).
    pub endereco: String,
    /// A porta (`8080` por padrão).
    pub porta: String,
    /// O segredo que assina o cookie. Sem ele, a chave é nova a cada início.
    pub segredo: Option<String>,
    /// O ID do Pixel, para o envio à Meta.
    pub pixel_id: Option<String>,
    /// O token da Conversions API. Sem ele, o modelo fica em modo demonstração.
    pub token: Option<String>,
    /// O código de teste da Events Manager, se houver.
    pub test_event_code: Option<String>,
    /// Base alternativa da API, só para ambiente de teste (https, ou http em loopback).
    pub api_base: Option<String>,
    /// Quantas origens e quantos eventos ficam na memória (200 por padrão).
    pub memoria: usize,
    /// Prazo do cookie de atribuição, em dias (90 por padrão).
    pub prazo_dias: u64,
    /// Se o site roda em HTTPS. Liga o atributo `Secure` no cookie.
    pub seguro: bool,
    /// O DDI que entra no telefone sem DDI explícito (`55`).
    pub ddi_padrao: String,
    /// O índice de subdomínio do `fbc` (1 para domínio comum).
    pub indice_subdominio: u8,
    /// Se IP e user agent devem ficar fora do envio à Meta.
    pub sem_ip_e_user_agent: bool,
    /// Se `WORTIC_API_KEY` está definida. Só a presença entra aqui.
    pub wortic_chave: bool,
    /// Para o programa depois de tantos pedidos. Serve a testes e a smoke tests.
    pub max_pedidos: Option<usize>,
    /// O arquivo `.jsw` onde os eventos são gravados. As origens vão para `origens.jsw` na mesma pasta. Sem ele, só memória.
    pub arquivo: Option<String>,
    /// Quantos registros novos entre uma gravação e a outra do arquivo (20 por padrão).
    pub arquivo_a_cada: usize,
}

impl Ajustes {
    /// Os ajustes do ambiente do processo.
    #[must_use]
    pub fn do_ambiente() -> Self {
        ajustes_de(|chave| std::env::var(chave).ok())
    }
}

/// Monta os ajustes a partir de uma leitura do ambiente. Valor vazio conta como ausente.
#[must_use]
pub fn ajustes_de(ambiente: impl Fn(&str) -> Option<String>) -> Ajustes {
    let texto = |chave: &str| {
        ambiente(chave)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    Ajustes {
        endereco: texto("ENDERECO").unwrap_or_else(|| "127.0.0.1".to_string()),
        porta: texto("PORTA").unwrap_or_else(|| "8080".to_string()),
        segredo: texto("RASTREIO_SEGREDO"),
        pixel_id: texto("RASTREIO_PIXEL_ID"),
        token: texto("RASTREIO_ACCESS_TOKEN"),
        test_event_code: texto("RASTREIO_TEST_EVENT_CODE"),
        api_base: texto("RASTREIO_API_BASE"),
        memoria: numero_ou(texto("RASTREIO_MEMORIA"), 200),
        prazo_dias: numero_ou(texto("RASTREIO_PRAZO_DIAS"), 90),
        seguro: texto("RASTREIO_HTTPS").as_deref() == Some("1"),
        ddi_padrao: texto("RASTREIO_DDI").unwrap_or_else(|| DDI_PADRAO.to_string()),
        indice_subdominio: numero_ou(
            texto("RASTREIO_INDICE_SUBDOMINIO"),
            identificadores::INDICE_PADRAO,
        ),
        sem_ip_e_user_agent: texto("RASTREIO_SEM_IP_UA").as_deref() == Some("1"),
        wortic_chave: texto("WORTIC_API_KEY").is_some(),
        max_pedidos: texto("RASTREIO_MAX_PEDIDOS").and_then(|v| v.parse().ok()),
        arquivo: texto("RASTREIO_ARQUIVO"),
        arquivo_a_cada: numero_ou(texto("RASTREIO_ARQUIVO_CADA"), 20),
    }
}

fn numero_ou<T: std::str::FromStr>(valor: Option<String>, padrao: T) -> T {
    valor.and_then(|v| v.parse().ok()).unwrap_or(padrao)
}

/// Uma origem: a página onde a visita trouxe UTM ou clid, e o que a URL trouxe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origem {
    /// Quando a visita aconteceu, em milissegundos.
    pub quando_ms: u64,
    /// A página, sem a consulta.
    pub pagina: String,
    /// O que a URL trouxe de rastreio.
    pub captura: Captura,
}

/// Um evento recebido em `/evento`, com o que aconteceu com ele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registro {
    /// Quando chegou, em milissegundos.
    pub quando_ms: u64,
    /// O nome do evento.
    pub evento: &'static str,
    /// O identificador do evento (pedido).
    pub event_id: String,
    /// O valor formatado, com a moeda, quando há.
    pub valor: Option<String>,
    /// O valor em centavos, quando há (vai para o `dinheiro` do jsonW).
    pub valor_centavos: Option<u32>,
    /// A moeda de três letras, quando o valor tem uma moeda válida.
    pub moeda: Option<String>,
    /// A origem da última visita do comprador, em texto.
    pub origem: String,
    /// O que aconteceu, em uma linha.
    pub situacao: String,
}

/// Uma foto da memória para a tela, do mais novo para o mais antigo.
pub struct Retrato {
    /// As últimas origens.
    pub origens: Vec<Origem>,
    /// Os últimos eventos.
    pub registros: Vec<Registro>,
}

#[derive(Default)]
struct Memoria {
    origens: VecDeque<Origem>,
    registros: VecDeque<Registro>,
    limite: usize,
    /// Quantos registros entraram desde a última gravação do arquivo.
    novos: usize,
}

/// O estado do modelo: o cookie, o envio (se houver Pixel e token), a memória e as opções.
pub struct Estado {
    cookie: Cookie,
    envio: Option<Envio>,
    memoria: Mutex<Memoria>,
    ddi_padrao: String,
    indice_subdominio: u8,
    seguro: bool,
    enviar_ip_e_user_agent: bool,
    wortic_chave: bool,
    /// O `.jsw` dos eventos (com as origens ao lado), se `RASTREIO_ARQUIVO` estiver definido.
    arquivo: Option<PathBuf>,
    arquivo_a_cada: usize,
    /// Uma gravação de cada vez: duas threads não disputam o mesmo arquivo.
    gravacao: Mutex<()>,
}

/// O que aconteceu com uma venda: ou um resultado do envio, ou o evento montado em modo demonstração.
enum Desfecho {
    Resultado(Resultado),
    Demonstracao(Box<Evento>),
}

impl Estado {
    /// Monta o estado a partir dos ajustes. Sem segredo, a chave é gerada agora.
    ///
    /// # Errors
    ///
    /// Devolve texto com o motivo quando o segredo é curto, ou quando o Pixel, o token ou a base não servem.
    pub fn de(ajustes: &Ajustes) -> Result<Self, String> {
        let chave = match &ajustes.segredo {
            Some(segredo) => segredo.as_bytes().to_vec(),
            None => chave_aleatoria(),
        };
        let cookie = Cookie::new(&chave, ajustes.prazo_dias)
            .map_err(|_| "RASTREIO_SEGREDO precisa ter pelo menos 32 caracteres".to_string())?;
        let envio = match (&ajustes.pixel_id, &ajustes.token) {
            (Some(pixel), Some(token)) => {
                let mut config = Config::nova(pixel, token);
                config.test_event_code.clone_from(&ajustes.test_event_code);
                if let Some(base) = &ajustes.api_base {
                    config.base.clone_from(base);
                }
                Some(Envio::nova(config).map_err(|erro| erro.to_string())?)
            }
            _ => None,
        };
        Ok(Self {
            cookie,
            envio,
            memoria: Mutex::new(Memoria {
                limite: ajustes.memoria.max(1),
                ..Memoria::default()
            }),
            ddi_padrao: ajustes.ddi_padrao.clone(),
            indice_subdominio: ajustes.indice_subdominio,
            seguro: ajustes.seguro,
            enviar_ip_e_user_agent: !ajustes.sem_ip_e_user_agent,
            wortic_chave: ajustes.wortic_chave,
            arquivo: ajustes.arquivo.as_deref().map(PathBuf::from),
            arquivo_a_cada: ajustes.arquivo_a_cada.max(1),
            gravacao: Mutex::new(()),
        })
    }

    /// Uma foto da memória para a tela.
    #[must_use]
    pub fn retrato(&self) -> Retrato {
        let memoria = self.trava();
        Retrato {
            origens: memoria.origens.iter().rev().cloned().collect(),
            registros: memoria.registros.iter().rev().cloned().collect(),
        }
    }

    /// As origens guardadas, da mais antiga para a mais nova (a ordem em que entraram).
    #[must_use]
    pub fn origens_guardadas(&self) -> Vec<Origem> {
        self.trava().origens.iter().cloned().collect()
    }

    /// Os eventos guardados, da mais antiga para a mais nova (a ordem em que entraram).
    #[must_use]
    pub fn registros_guardados(&self) -> Vec<Registro> {
        self.trava().registros.iter().cloned().collect()
    }

    fn trava(&self) -> MutexGuard<'_, Memoria> {
        self.memoria.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn guardar_origem(&self, origem: Origem) {
        let mut memoria = self.trava();
        let limite = memoria.limite;
        empurrar(&mut memoria.origens, origem, limite);
        memoria.novos += 1;
    }

    fn guardar_registro(&self, registro: Registro) {
        let mut memoria = self.trava();
        let limite = memoria.limite;
        empurrar(&mut memoria.registros, registro, limite);
        memoria.novos += 1;
    }

    /// Grava o arquivo se já entraram `RASTREIO_ARQUIVO_CADA` registros desde a última gravação.
    ///
    /// # Errors
    /// A pasta não deixa escrever, ou a publicação do `.jsw` falhou.
    pub fn gravar_se_preciso(&self) -> Result<(), String> {
        if self.arquivo.is_none() {
            return Ok(());
        }
        {
            let mut memoria = self.trava();
            if memoria.novos < self.arquivo_a_cada {
                return Ok(());
            }
            memoria.novos = 0;
        }
        self.gravar_arquivos()
    }

    /// Grava os eventos no caminho de `RASTREIO_ARQUIVO` e as origens em `origens.jsw` na mesma pasta. A troca é
    /// atômica (só o fechamento publica), então um leitor nunca vê o arquivo pela metade.
    ///
    /// # Errors
    /// A pasta não deixa escrever, ou a publicação falhou.
    pub fn gravar_arquivos(&self) -> Result<(), String> {
        let Some(caminho) = &self.arquivo else {
            return Ok(());
        };
        let _vez = self.gravacao.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(pasta) = caminho.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(pasta).map_err(|erro| {
                format!("não consegui criar a pasta {}: {erro}", pasta.display())
            })?;
        }
        let eventos = exportar::eventos(&self.registros_guardados());
        arquivo::gravar(
            caminho,
            esquemas::REGISTRO,
            eventos.iter().map(String::as_str),
        )
        .map_err(|r| jsonw::sem_trecho(&r))?;
        let origens = exportar::origens(&self.origens_guardadas());
        let do_lado = caminho.with_file_name("origens.jsw");
        arquivo::gravar(
            &do_lado,
            esquemas::ORIGEM,
            origens.iter().map(String::as_str),
        )
        .map_err(|r| jsonw::sem_trecho(&r))?;
        Ok(())
    }

    fn cookie_de_atribuicao(&self, atribuicao: &Atribuicao) -> Header {
        let valor = self.cookie.gravar(atribuicao);
        cabecalho("Set-Cookie", &self.cookie.cabecalho(&valor, self.seguro))
    }
}

fn empurrar<T>(fila: &mut VecDeque<T>, item: T, limite: usize) {
    fila.push_back(item);
    while fila.len() > limite {
        fila.pop_front();
    }
}

fn chave_aleatoria() -> Vec<u8> {
    let mut chave = [0u8; 32];
    File::open("/dev/urandom")
        .and_then(|mut arquivo| arquivo.read_exact(&mut chave))
        .expect("o sistema precisa de /dev/urandom");
    chave.to_vec()
}

/// Abre a porta no endereço. Endereço IPv6 entra entre colchetes. Se não abrir, devolve o motivo.
///
/// # Errors
///
/// Devolve texto quando a porta não pode ser aberta (porta inválida ou em uso).
pub fn abrir(endereco: &str, porta: &str) -> Result<Arc<Server>, String> {
    let host = if endereco.contains(':') {
        format!("[{endereco}]")
    } else {
        endereco.to_string()
    };
    wortic_base::http::servidor(format!("{host}:{porta}"))
        .map(Arc::new)
        .map_err(|erro| format!("não consegui abrir {host}:{porta}: {erro}"))
}

/// Estado e servidor prontos, sem atender nenhum pedido ainda.
///
/// # Errors
///
/// Devolve o motivo quando o estado ou a porta não servem.
pub fn preparar(ajustes: &Ajustes) -> Result<(Arc<Estado>, Arc<Server>), String> {
    let estado = Arc::new(Estado::de(ajustes)?);
    let servidor = abrir(&ajustes.endereco, &ajustes.porta)?;
    Ok((estado, servidor))
}

/// Sobe o modelo e atende pedidos. Sem `max_pedidos`, roda até o processo parar.
///
/// # Errors
///
/// Devolve o motivo quando o estado ou a porta não servem.
pub fn executar(ajustes: &Ajustes) -> Result<(), String> {
    let (estado, servidor) = preparar(ajustes)?;
    let endereco = servidor
        .server_addr()
        .to_ip()
        .map_or_else(String::new, |a| a.to_string());
    println!("Rastreio CAPI no ar em http://{endereco}");
    servir(&servidor, &estado, ajustes.max_pedidos);
    estado.gravar_arquivos()
}

/// Atende os pedidos com quatro threads. Volta quando o servidor é desligado ou quando atingiu `max_pedidos`.
pub fn servir(servidor: &Arc<Server>, estado: &Arc<Estado>, max_pedidos: Option<usize>) {
    let atendidos = Arc::new(AtomicUsize::new(0));
    let trabalhadores: Vec<_> = (0..LINHAS)
        .map(|_| {
            let (servidor, estado, atendidos) =
                (servidor.clone(), estado.clone(), atendidos.clone());
            thread::spawn(move || {
                for pedido in servidor.incoming_requests() {
                    atender(pedido, &estado);
                    if let Err(razao) = estado.gravar_se_preciso() {
                        eprintln!("rastreio: a gravação do arquivo falhou: {razao}");
                    }
                    let feitos = atendidos.fetch_add(1, Ordering::SeqCst) + 1;
                    if max_pedidos.is_some_and(|max| feitos >= max) {
                        // cada unblock acorda e para uma única thread de atendimento
                        for _ in 0..LINHAS {
                            servidor.unblock();
                        }
                    }
                }
            })
        })
        .collect();
    for trabalhador in trabalhadores {
        let _ = trabalhador.join();
    }
}

fn atender(mut pedido: Request, estado: &Estado) {
    let url = pedido.url().to_string();
    let (caminho, consulta) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let cookies = cookies_de(&pedido);
    let accept = cabecalho_de(&pedido, "Accept");
    let param = parametro(consulta, "formato");
    let formato = Formato::do_pedido(accept.as_deref(), param);
    // pediu formato (pelo parâmetro ou pelo Accept jsonw): o /consentimento responde com corpo, não com redirect
    let pediu = param.is_some()
        || accept
            .as_deref()
            .is_some_and(|a| a.to_ascii_lowercase().contains("jsonw"));
    let metodo = pedido.method().clone();
    let resposta = match (metodo, caminho) {
        (Method::Get, "/") => {
            let situacao = Situacao {
                consentido: consentido(&cookies),
                envio_ligado: estado.envio.is_some(),
                wortic_chave: estado.wortic_chave,
                arquivo: estado.arquivo.is_some(),
            };
            html(&painel::pagina(&estado.retrato(), &situacao))
        }
        (Method::Get, "/r") => pixel(estado, consulta, &cookies),
        (Method::Get, "/consentimento") => escolha(estado, consulta, pediu.then_some(formato)),
        (Method::Post, "/evento") => venda(estado, &mut pedido, &cookies, formato),
        (Method::Get, "/registros/eventos") => registros(
            formato,
            esquemas::REGISTRO,
            &exportar::eventos(&estado.registros_guardados()),
            "eventos.jsw",
        ),
        (Method::Get, "/registros/origens") => registros(
            formato,
            esquemas::ORIGEM,
            &exportar::origens(&estado.origens_guardadas()),
            "origens.jsw",
        ),
        (_, "/evento") => erro_json(formato, 405, "metodo_nao_aceito", "use POST em /evento"),
        _ => erro_json(formato, 404, "nao_existe", "rota não encontrada"),
    };
    let _ = pedido.respond(resposta);
}

/// Uma lista guardada no formato pedido. JSON, jsonW-texto ou o arquivo `.jsw` (com o nome de download).
fn registros(formato: Formato, esquema: &str, linhas: &[String], nome: &str) -> Resp {
    match jsonw::lista(formato, esquema, linhas) {
        Ok(corpo) => {
            let extras = if formato == Formato::Arquivo {
                vec![cabecalho(
                    "Content-Disposition",
                    &format!("attachment; filename=\"{nome}\""),
                )]
            } else {
                Vec::new()
            };
            resposta(200, corpo.tipo, corpo.bytes, extras)
        }
        Err(razao) => erro_json(formato, 500, "exportar_falhou", &razao),
    }
}

/// O pixel primário. Com consentimento e com UTM ou clid em `u`, grava a origem e atualiza o cookie.
fn pixel(estado: &Estado, consulta: &str, cookies: &[(String, String)]) -> Resp {
    let pares = captura::pares(consulta);
    let mut extras = Vec::new();
    if consentido(cookies)
        && let Some(pagina) = buscar(&pares, "u")
    {
        let agora = agora_ms();
        let mut atribuicao = atribuicao_do_visitante(estado, cookies, agora);
        let captura = captura::de_url(pagina);
        if atribuicao.registrar(captura.clone(), agora) {
            extras.push(estado.cookie_de_atribuicao(&atribuicao));
            estado.guardar_origem(Origem {
                quando_ms: agora,
                pagina: pagina_segura(pagina),
                captura,
            });
        }
    }
    resposta(200, "image/gif", GIF.to_vec(), extras)
}

/// Grava a escolha do visitante. `aceitar=sim` liga; qualquer outra coisa desliga. Sem formato pedido, redireciona
/// para a tela (o navegador). Com formato pedido, responde 200 com `consentimento` no formato.
fn escolha(estado: &Estado, consulta: &str, pedido: Option<Formato>) -> Resp {
    let pares = captura::pares(consulta);
    let sim = buscar(&pares, "aceitar") == Some("sim");
    let valor = if sim { "1" } else { "0" };
    let seguro = if estado.seguro { "; Secure" } else { "" };
    let linha = format!(
        "{CONSENTIMENTO}={valor}; Path=/; Max-Age=31536000; HttpOnly; SameSite=Lax{seguro}"
    );
    match pedido {
        None => resposta(
            303,
            "text/plain; charset=utf-8",
            Vec::new(),
            vec![cabecalho("Location", "/"), cabecalho("Set-Cookie", &linha)],
        ),
        Some(formato) => {
            let corpo = obj(&[("consentimento", texto(if sim { "sim" } else { "nao" }))]);
            resposta_no_formato(formato, 200, &corpo, vec![cabecalho("Set-Cookie", &linha)])
        }
    }
}

/// A venda: lê o corpo (formulário, jsonW-texto ou JSON, pelo `Content-Type`), confere os campos e manda (ou só monta)
/// o evento. A resposta sai no formato pedido.
fn venda(
    estado: &Estado,
    pedido: &mut Request,
    cookies: &[(String, String)],
    formato: Formato,
) -> Resp {
    let mut corpo = String::new();
    let _ = pedido
        .as_reader()
        .take(LIMITE_CORPO)
        .read_to_string(&mut corpo);
    let ip = pedido.remote_addr().map(|e| e.ip().to_string());
    let user_agent = cabecalho_de(pedido, "User-Agent");
    let tipo = cabecalho_de(pedido, "Content-Type")
        .unwrap_or_default()
        .to_ascii_lowercase();
    // jsonW-texto, ou JSON (que também é jsonW): a mesma conferência do esquema da venda
    let campos = if tipo.contains("text/jsonw") || tipo.contains("application/json") {
        match venda::campos_de(esquemas::VENDA, &corpo) {
            Ok(campos) => campos,
            Err(erro) => return erro_json(formato, erro.status, erro.codigo, &erro.mensagem),
        }
    } else {
        captura::pares(&corpo)
    };
    venda_com(
        estado,
        &campos,
        cookies,
        agora_ms(),
        ip.as_deref(),
        user_agent.as_deref(),
        formato,
    )
}

fn venda_com(
    estado: &Estado,
    campos: &[(String, String)],
    cookies: &[(String, String)],
    agora_ms: u64,
    ip: Option<&str>,
    user_agent: Option<&str>,
    formato: Formato,
) -> Resp {
    let pedido = match venda::pedido(campos) {
        Ok(pedido) => pedido,
        Err(erro) => return erro_json(formato, erro.status, erro.codigo, &erro.mensagem),
    };
    let atribuicao = atribuicao_do_visitante(estado, cookies, agora_ms);
    let (clid_meta, quando_clique) = atribuicao.ultimo.as_ref().map_or((None, agora_ms), |t| {
        (t.captura.clid(Plataforma::Meta), t.quando_ms)
    });
    let fbc = identificadores::fbc_escolhido(
        buscar(cookies, "_fbc"),
        clid_meta,
        quando_clique,
        estado.indice_subdominio,
    );
    let fbp = buscar(cookies, "_fbp").and_then(identificadores::fbp_valido);
    let dados = Dados {
        email: buscar(campos, "email"),
        telefone: buscar(campos, "telefone"),
        nome: buscar(campos, "nome"),
        sobrenome: buscar(campos, "sobrenome"),
        cidade: buscar(campos, "cidade"),
        estado: buscar(campos, "uf"),
        cep: buscar(campos, "cep"),
        pais: buscar(campos, "pais"),
        nascimento: buscar(campos, "nascimento"),
        externo: buscar(campos, "cliente"),
    };
    let entrada = Entrada {
        nome: pedido.nome,
        event_id: pedido.pedido,
        momento_s: agora_ms / 1000,
        agora_s: agora_ms / 1000,
        pagina: pedido.pagina,
        dados,
        ip,
        user_agent,
        fbc: fbc.as_deref(),
        fbp,
        valor_centavos: pedido.valor,
        moeda: Some(pedido.moeda),
        ddi_padrao: &estado.ddi_padrao,
        enviar_ip_e_user_agent: estado.enviar_ip_e_user_agent,
    };
    let desfecho = despachar(estado, &entrada, consentido(cookies));
    estado.guardar_registro(Registro {
        quando_ms: agora_ms,
        evento: pedido.nome.como_texto(),
        event_id: pedido.pedido.to_string(),
        valor: pedido.valor.map(|c| formatar_valor(c, pedido.moeda)),
        valor_centavos: pedido.valor,
        moeda: evento::moeda_valida(pedido.moeda),
        origem: origem_do_visitante(&atribuicao),
        situacao: resumo(&desfecho),
    });
    let (status, corpo) = resposta_da_venda(&desfecho);
    resposta_no_formato(formato, status, &corpo, Vec::new())
}

fn despachar(estado: &Estado, entrada: &Entrada, consentido: bool) -> Desfecho {
    if let Err(motivo) = consentimento::checar(consentido) {
        return Desfecho::Resultado(Resultado::Recusado(motivo));
    }
    match evento::montar(entrada) {
        Err(erro) => Desfecho::Resultado(Resultado::Invalido(erro)),
        Ok(evento) => match &estado.envio {
            Some(envio) => Desfecho::Resultado(envio.transmitir(std::slice::from_ref(&evento))),
            None => Desfecho::Demonstracao(Box::new(evento)),
        },
    }
}

fn resumo(desfecho: &Desfecho) -> String {
    match desfecho {
        Desfecho::Resultado(Resultado::Enviado { tentativas, .. }) => {
            format!("enviado em {tentativas} tentativa(s)")
        }
        Desfecho::Resultado(Resultado::Recusado(motivo)) => {
            format!("recusado: {}", motivo.codigo())
        }
        Desfecho::Resultado(Resultado::Invalido(erro)) => format!("inválido: {erro}"),
        Desfecho::Resultado(Resultado::Falhou { motivo, .. }) => format!("falhou: {motivo}"),
        Desfecho::Demonstracao(_) => "demonstração: nada foi enviado".to_string(),
    }
}

/// A resposta da venda em jsonW-texto, com o status HTTP. Cada campo sai só quando tem valor.
fn resposta_da_venda(desfecho: &Desfecho) -> (u16, String) {
    match desfecho {
        Desfecho::Resultado(Resultado::Enviado {
            tentativas,
            recebidos,
        }) => (
            200,
            obj(&[
                ("resultado", texto("enviado")),
                ("tentativas", tentativas.to_string()),
                ("recebidos", opcional_numero(recebidos.as_ref())),
            ]),
        ),
        Desfecho::Resultado(Resultado::Recusado(motivo)) => (
            200,
            obj(&[
                ("resultado", texto("recusado")),
                ("motivo", texto(motivo.codigo())),
                ("descricao", texto(&motivo.to_string())),
            ]),
        ),
        Desfecho::Resultado(Resultado::Invalido(erro)) => (
            422,
            obj(&[
                ("resultado", texto("invalido")),
                ("motivo", texto(&erro.to_string())),
            ]),
        ),
        Desfecho::Resultado(Resultado::Falhou { tentativas, motivo }) => (
            502,
            obj(&[
                ("resultado", texto("falhou")),
                ("tentativas", tentativas.to_string()),
                ("motivo", texto(motivo)),
            ]),
        ),
        Desfecho::Demonstracao(evento) => (
            200,
            obj(&[
                ("resultado", texto("demonstracao")),
                (
                    "motivo",
                    texto("sem RASTREIO_PIXEL_ID e RASTREIO_ACCESS_TOKEN: nada foi enviado à Meta"),
                ),
                ("evento", saida::evento(evento)),
            ]),
        ),
    }
}

fn opcional_numero<T: ToString>(valor: Option<&T>) -> String {
    valor.map_or_else(|| "null".to_string(), ToString::to_string)
}

fn formatar_valor(centavos: u32, moeda: &str) -> String {
    format!(
        "{},{:02} {}",
        centavos / 100,
        centavos % 100,
        moeda.to_uppercase()
    )
}

fn origem_do_visitante(atribuicao: &Atribuicao) -> String {
    let Some(toque) = &atribuicao.ultimo else {
        return "sem origem".to_string();
    };
    let utm = &toque.captura.utm;
    let partes: Vec<&str> = [&utm.source, &utm.medium, &utm.campaign]
        .into_iter()
        .filter_map(|parte| parte.as_deref())
        .collect();
    if partes.is_empty() {
        "clid, sem UTM".to_string()
    } else {
        partes.join(" / ")
    }
}

fn atribuicao_do_visitante(
    estado: &Estado,
    cookies: &[(String, String)],
    agora: u64,
) -> Atribuicao {
    buscar(cookies, cookie::NOME)
        .and_then(|valor| estado.cookie.ler(valor, agora))
        .unwrap_or_default()
}

fn consentido(cookies: &[(String, String)]) -> bool {
    buscar(cookies, CONSENTIMENTO) == Some("1")
}

fn pagina_segura(pagina: &str) -> String {
    captura::url_sem_consulta(pagina).unwrap_or_else(|| "(página inválida)".to_string())
}

fn cookies_de(pedido: &Request) -> Vec<(String, String)> {
    cabecalho_de(pedido, "Cookie").map_or_else(Vec::new, |texto| {
        texto
            .split(';')
            .filter_map(|par| {
                let (nome, valor) = par.trim().split_once('=')?;
                Some((nome.to_string(), valor.to_string()))
            })
            .collect()
    })
}

fn cabecalho_de(pedido: &Request, nome: &str) -> Option<String> {
    pedido
        .headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(nome))
        .map(|h| h.value.as_str().to_string())
}

fn cabecalho(nome: &str, valor: &str) -> Header {
    Header::from_bytes(nome.as_bytes(), valor.as_bytes()).expect("cabeçalho válido")
}

fn resposta(status: u16, tipo: &str, corpo: Vec<u8>, extras: Vec<Header>) -> Resp {
    let base = Response::from_data(corpo)
        .with_status_code(status)
        .with_header(cabecalho("Content-Type", tipo))
        .with_header(cabecalho("Cache-Control", "no-store"));
    extras.into_iter().fold(base, Response::with_header)
}

fn html(corpo: &str) -> Resp {
    resposta(
        200,
        "text/html; charset=utf-8",
        corpo.as_bytes().to_vec(),
        Vec::new(),
    )
}

/// Uma resposta (um objeto jsonW já escrito) no formato pedido. JSON só sai aqui, na borda, para quem chama de fora;
/// o corpo é conferido pelo esquema da resposta. jsonW-texto e o `.jsw` (que, num objeto só, cai no texto) saem como estão.
fn resposta_no_formato(formato: Formato, status: u16, corpo: &str, extras: Vec<Header>) -> Resp {
    match formato {
        Formato::Json => match jsonw::como_json(esquemas::RESPOSTA, corpo) {
            // borda-json: resposta a quem chama de fora
            Ok(json) => resposta(status, TIPO_JSON, json.into_bytes(), extras), // borda-json: resposta a quem chama de fora
            Err(razao) => resposta(
                500,
                Formato::Texto.tipo_do_corpo(),
                texto_de_erro(&razao),
                Vec::new(),
            ),
        },
        Formato::Texto | Formato::Arquivo => resposta(
            status,
            formato.tipo_do_corpo(),
            corpo.as_bytes().to_vec(),
            extras,
        ),
    }
}

fn texto_de_erro(razao: &str) -> Vec<u8> {
    obj(&[
        ("resultado", texto("erro_interno")),
        ("motivo", texto(razao)),
    ])
    .into_bytes()
}

fn erro_json(formato: Formato, status: u16, codigo: &str, mensagem: &str) -> Resp {
    resposta_no_formato(
        formato,
        status,
        &obj(&[("resultado", texto(codigo)), ("motivo", texto(mensagem))]),
        Vec::new(),
    )
}

fn agora_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::suporte::{falso, pedir_com, pedir_form, pedir_get, resp};

    fn pasta_de_teste(nome: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rastreio-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    /// Um servidor de verdade (porta livre) com o estado de teste. Ao sair, desliga e espera as threads.
    struct Local {
        estado: Arc<Estado>,
        endereco: String,
        servidor: Arc<Server>,
        fio: Option<thread::JoinHandle<()>>,
    }

    impl Drop for Local {
        fn drop(&mut self) {
            for _ in 0..LINHAS {
                self.servidor.unblock();
            }
            let _ = self.fio.take().map(thread::JoinHandle::join);
        }
    }

    fn ajustes_base() -> Ajustes {
        let mut ajustes = ajustes_de(|_| None);
        ajustes.segredo = Some("0123456789abcdef0123456789abcdef".to_string());
        ajustes
    }

    fn com_envio(base: &str) -> Ajustes {
        let mut ajustes = ajustes_base();
        ajustes.pixel_id = Some("123456789012345".to_string());
        ajustes.token = Some("TOKEN_DE_TESTE".to_string());
        ajustes.api_base = Some(base.to_string());
        ajustes
    }

    fn sobe(ajustes: &Ajustes) -> Local {
        let estado = Arc::new(Estado::de(ajustes).expect("estado de teste"));
        let servidor = abrir("127.0.0.1", "0").expect("porta livre");
        let endereco = format!("http://{}", servidor.server_addr().to_ip().expect("ip"));
        let (s2, e2) = (servidor.clone(), estado.clone());
        let fio = thread::spawn(move || servir(&s2, &e2, None));
        Local {
            estado,
            endereco,
            servidor,
            fio: Some(fio),
        }
    }

    fn cookie_de(valor: &str) -> String {
        valor.to_string()
    }

    #[test]
    fn ajustes_padrao_quando_o_ambiente_esta_vazio() {
        let a = ajustes_de(|_| None);
        assert_eq!(a.endereco, "127.0.0.1");
        assert_eq!(a.porta, "8080");
        assert!(a.segredo.is_none() && a.pixel_id.is_none() && a.token.is_none());
        assert_eq!(a.memoria, 200);
        assert_eq!(a.prazo_dias, 90);
        assert!(!a.seguro);
        assert_eq!(a.ddi_padrao, "55");
        assert_eq!(a.indice_subdominio, 1);
        assert!(!a.sem_ip_e_user_agent && !a.wortic_chave);
        assert!(a.max_pedidos.is_none());
    }

    #[test]
    fn ajustes_leem_o_ambiente_e_ignoram_vazio_e_numero_ruim() {
        let ambiente: std::collections::HashMap<&str, &str> = [
            ("ENDERECO", "0.0.0.0"),
            ("PORTA", "9000"),
            ("RASTREIO_SEGREDO", "  segredo  "),
            ("RASTREIO_PIXEL_ID", "   "),
            ("RASTREIO_ACCESS_TOKEN", "tok"),
            ("RASTREIO_TEST_EVENT_CODE", "TEST1"),
            ("RASTREIO_API_BASE", "http://127.0.0.1:9"),
            ("RASTREIO_MEMORIA", "abc"),
            ("RASTREIO_PRAZO_DIAS", "30"),
            ("RASTREIO_HTTPS", " 1 "),
            ("RASTREIO_DDI", "1 "),
            ("RASTREIO_INDICE_SUBDOMINIO", "2"),
            ("RASTREIO_SEM_IP_UA", "1"),
            ("WORTIC_API_KEY", "chave"),
            ("RASTREIO_MAX_PEDIDOS", "3"),
        ]
        .into_iter()
        .collect();
        let a = ajustes_de(|chave| ambiente.get(chave).map(ToString::to_string));
        assert_eq!(a.endereco, "0.0.0.0");
        assert_eq!(a.porta, "9000");
        assert_eq!(a.segredo.as_deref(), Some("segredo"));
        assert!(a.pixel_id.is_none());
        assert_eq!(a.token.as_deref(), Some("tok"));
        assert_eq!(a.test_event_code.as_deref(), Some("TEST1"));
        assert_eq!(a.api_base.as_deref(), Some("http://127.0.0.1:9"));
        assert_eq!(a.memoria, 200, "número inválido cai no padrão");
        assert_eq!(a.prazo_dias, 30);
        assert!(a.seguro && a.sem_ip_e_user_agent && a.wortic_chave);
        assert_eq!(a.ddi_padrao, "1");
        assert_eq!(a.indice_subdominio, 2);
        assert_eq!(a.max_pedidos, Some(3));
    }

    #[test]
    fn do_ambiente_le_o_processo_sem_quebrar() {
        // só confere que a leitura do processo monta os ajustes; não depende de variável definida
        let a = Ajustes::do_ambiente();
        assert_ne!(a.ddi_padrao, "");
    }

    #[test]
    fn segredo_curto_e_recusado_com_a_mensagem() {
        let mut ajustes = ajustes_base();
        ajustes.segredo = Some("curto".to_string());
        let erro = Estado::de(&ajustes).err().expect("erro");
        assert!(erro.contains("32 caracteres"));
    }

    #[test]
    fn pixel_sem_token_fica_em_demonstracao() {
        let mut ajustes = ajustes_base();
        ajustes.pixel_id = Some("123".to_string());
        let estado = Estado::de(&ajustes).expect("estado");
        assert!(estado.envio.is_none());
    }

    #[test]
    fn pixel_invalido_com_token_e_recusado() {
        let mut ajustes = ajustes_base();
        ajustes.pixel_id = Some("abc".to_string());
        ajustes.token = Some("t".to_string());
        assert!(Estado::de(&ajustes).is_err());
    }

    #[test]
    fn com_pixel_e_token_monta_o_envio_e_sem_segredo_gera_chave() {
        let estado = Estado::de(&com_envio("https://graph.facebook.com")).expect("estado");
        assert!(estado.envio.is_some());
        let mut ajustes = ajustes_base();
        ajustes.segredo = None;
        assert!(Estado::de(&ajustes).is_ok());
        assert_eq!(chave_aleatoria().len(), 32);
    }

    #[test]
    fn com_base_ruim_o_envio_nao_e_montado() {
        let mut ajustes = com_envio("http://exemplo.com");
        ajustes.api_base = Some("http://exemplo.com".to_string());
        assert!(Estado::de(&ajustes).is_err());
    }

    #[test]
    fn abrir_com_porta_invalida_devolve_o_motivo() {
        let erro = abrir("127.0.0.1", "abc").err().expect("erro");
        assert!(erro.contains("não consegui abrir"));
    }

    #[test]
    fn abrir_aceita_porta_livre_e_ipv6() {
        assert!(abrir("127.0.0.1", "0").is_ok());
        assert!(abrir("::1", "0").is_ok() || abrir("127.0.0.1", "0").is_ok());
    }

    #[test]
    fn preparar_com_porta_invalida_para_antes_do_servidor() {
        let mut ajustes = ajustes_base();
        ajustes.porta = "abc".to_string();
        assert!(preparar(&ajustes).is_err());
    }

    #[test]
    fn executar_com_ajuste_invalido_devolve_erro() {
        let mut ajustes = ajustes_base();
        ajustes.porta = "abc".to_string();
        assert!(executar(&ajustes).is_err());
    }

    #[test]
    fn empurrar_respeita_o_limite() {
        let mut fila = VecDeque::new();
        for i in 0..5 {
            empurrar(&mut fila, i, 3);
        }
        assert_eq!(fila.iter().copied().collect::<Vec<_>>(), vec![2, 3, 4]);
    }

    #[test]
    fn pagina_inicial_mostra_o_estado_sem_dado_do_visitante() {
        let l = sobe(&ajustes_base());
        let r = pedir_get(&format!("{}/", l.endereco), &[]);
        assert_eq!(r.status, 200);
        assert!(r.corpo.contains("Modo demonstração"));
        assert!(r.corpo.contains("Nenhuma origem ainda"));
        assert!(r.corpo.contains("Nenhum evento ainda"));
        assert!(r.corpo.contains("Consentimento não concedido"));
        assert!(
            r.tipo
                .as_deref()
                .is_some_and(|t| t.starts_with("text/html"))
        );
    }

    #[test]
    fn pixel_sem_consentimento_nao_grava_nada() {
        let l = sobe(&ajustes_base());
        let r = pedir_get(
            &format!(
                "{}/r?u=https%3A%2F%2Floja.com%2F%3Futm_source%3Dfb",
                l.endereco
            ),
            &[],
        );
        assert_eq!(r.status, 200);
        assert_eq!(r.tipo.as_deref(), Some("image/gif"));
        assert!(r.cookie_rc_atrib().is_none());
        assert_eq!(l.estado.retrato().origens.len(), 0);
    }

    #[test]
    fn pixel_com_consentimento_grava_origem_e_cookie() {
        let l = sobe(&ajustes_base());
        let url = format!(
            "{}/r?u={}",
            l.endereco,
            codificar(
                "https://loja.com/oferta?utm_source=facebook&utm_medium=cpc&utm_campaign=verao&fbclid=ABC123"
            )
        );
        let r = pedir_get(&url, &[&cookie_de(&format!("{CONSENTIMENTO}=1"))]);
        assert_eq!(r.status, 200);
        assert!(r.cookie_rc_atrib().is_some(), "cookie de atribuição");
        let linha = r.cabecalho("set-cookie").expect("Set-Cookie");
        assert!(
            linha.contains("HttpOnly")
                && linha.contains("SameSite=Lax")
                && !linha.contains("Secure")
        );
        let retrato = l.estado.retrato();
        assert_eq!(retrato.origens.len(), 1);
        assert_eq!(retrato.origens[0].pagina, "https://loja.com/oferta");
        assert_eq!(
            retrato.origens[0].captura.utm.source.as_deref(),
            Some("facebook")
        );
    }

    #[test]
    fn segundo_toque_mantem_o_primeiro_e_troca_o_ultimo() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let primeiro = pedir_get(
            &format!(
                "{}/r?u={}",
                l.endereco,
                codificar("https://loja.com/?utm_source=google")
            ),
            &[&consent],
        );
        let cookie1 = primeiro.cookie_rc_atrib().expect("cookie 1");
        let segundo = pedir_get(
            &format!(
                "{}/r?u={}",
                l.endereco,
                codificar("https://loja.com/?utm_source=tiktok")
            ),
            &[&consent, &cookie_de(&format!("rc_atrib={cookie1}"))],
        );
        let cookie2 = segundo.cookie_rc_atrib().expect("cookie 2");
        let atrib = l.estado.cookie.ler(&cookie2, agora_ms()).expect("lido");
        assert_eq!(
            atrib
                .primeiro
                .expect("primeiro")
                .captura
                .utm
                .source
                .as_deref(),
            Some("google")
        );
        assert_eq!(
            atrib.ultimo.expect("ultimo").captura.utm.source.as_deref(),
            Some("tiktok")
        );
    }

    #[test]
    fn pixel_sem_u_ou_sem_rastreio_nao_mexe_no_cookie() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let sem_u = pedir_get(&format!("{}/r", l.endereco), &[&consent]);
        assert!(sem_u.cookie_rc_atrib().is_none());
        let sem_rastreio = pedir_get(
            &format!(
                "{}/r?u={}",
                l.endereco,
                codificar("https://loja.com/pagina-normal")
            ),
            &[&consent],
        );
        assert!(sem_rastreio.cookie_rc_atrib().is_none());
        assert_eq!(l.estado.retrato().origens.len(), 0);
    }

    #[test]
    fn pagina_invalida_e_guardada_com_marcador() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        pedir_get(
            &format!(
                "{}/r?u={}",
                l.endereco,
                codificar("javascript:alert(1)?utm_source=x")
            ),
            &[&consent],
        );
        assert_eq!(l.estado.retrato().origens[0].pagina, "(página inválida)");
    }

    #[test]
    fn consentimento_grava_a_escolha_e_volta_para_a_tela() {
        let l = sobe(&ajustes_base());
        let sim = pedir_get(&format!("{}/consentimento?aceitar=sim", l.endereco), &[]);
        assert_eq!(sim.status, 303);
        assert_eq!(sim.cabecalho("location").as_deref(), Some("/"));
        assert!(
            sim.cookie_por_nome(CONSENTIMENTO)
                .is_some_and(|c| c.starts_with("rc_consent=1"))
        );
        let nao = pedir_get(&format!("{}/consentimento?aceitar=nao", l.endereco), &[]);
        assert!(
            nao.cookie_por_nome(CONSENTIMENTO)
                .is_some_and(|c| c.starts_with("rc_consent=0"))
        );
    }

    #[test]
    fn consentimento_com_secure_quando_https() {
        let mut ajustes = ajustes_base();
        ajustes.seguro = true;
        let l = sobe(&ajustes);
        let r = pedir_get(&format!("{}/consentimento?aceitar=sim", l.endereco), &[]);
        assert!(
            r.cookie_por_nome(CONSENTIMENTO)
                .is_some_and(|c| c.contains("Secure"))
        );
    }

    #[test]
    fn rota_desconhecida_da_404_em_json() {
        let l = sobe(&ajustes_base());
        let r = pedir_get(&format!("{}/nada", l.endereco), &[]);
        assert_eq!(r.status, 404);
        assert!(r.corpo.contains("nao_existe"));
    }

    #[test]
    fn evento_por_get_da_405() {
        let l = sobe(&ajustes_base());
        let r = pedir_get(&format!("{}/evento", l.endereco), &[]);
        assert_eq!(r.status, 405);
        assert!(r.corpo.contains("metodo_nao_aceito"));
    }

    #[test]
    fn venda_sem_consentimento_e_recusada_e_nao_monta_nada() {
        let l = sobe(&ajustes_base());
        let r = pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=Purchase&pedido=1001&valor=99,90&u=https%3A%2F%2Floja.com%2F&email=ana%40exemplo.com",
            &[],
        );
        assert_eq!(r.status, 200);
        assert!(r.corpo.contains("\"resultado\":\"recusado\""));
        assert!(r.corpo.contains("sem_consentimento"));
        assert!(!r.corpo.contains("ana"));
        let registros = l.estado.retrato().registros;
        assert_eq!(registros.len(), 1);
        assert!(registros[0].situacao.starts_with("recusado"));
    }

    #[test]
    fn venda_em_demonstracao_monta_o_evento_com_hash_e_nao_envia() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let r = pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=Purchase&pedido=1001&valor=99,90&moeda=brl&u=https%3A%2F%2Floja.com%2Fobrigado%3Fpedido%3D1001&email=John_Smith%40gmail.com&telefone=(650)555-1212&nome=Mary",
            &[&consent],
        );
        assert_eq!(r.status, 200);
        assert!(r.corpo.contains("\"resultado\":\"demonstracao\""));
        assert!(
            r.corpo
                .contains("62a14e44f765419d10fea99367361a727c12365e2520f32218d505ed9aa0f62f")
        );
        assert!(
            r.corpo
                .contains("\"event_source_url\":\"https://loja.com/obrigado\"")
        );
        assert!(!r.corpo.contains("John_Smith@gmail.com"));
        assert!(
            !r.corpo.contains("pedido=1001&"),
            "a consulta da página não sai"
        );
        let registros = l.estado.retrato().registros;
        assert_eq!(registros[0].valor.as_deref(), Some("99,90 BRL"));
        assert!(registros[0].situacao.starts_with("demonstração"));
    }

    #[test]
    fn venda_com_envio_manda_ao_servidor_falso_com_test_event_code() {
        let falso = falso(vec![resp(200, "{\"events_received\":1}")]);
        let mut ajustes = com_envio(&falso.base);
        ajustes.test_event_code = Some("TEST77".to_string());
        let l = sobe(&ajustes);
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let r = pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=Lead&pedido=abc&u=https%3A%2F%2Floja.com%2F&email=ana%40exemplo.com",
            &[&consent],
        );
        assert_eq!(r.status, 200);
        assert!(r.corpo.contains("\"resultado\":\"enviado\""));
        let pedidos = falso.pedidos.lock().unwrap();
        assert_eq!(pedidos.len(), 1);
        assert!(pedidos[0].1.contains("\"test_event_code\":\"TEST77\""));
        assert!(pedidos[0].1.contains("\"event_name\":\"Lead\""));
        assert!(!pedidos[0].1.contains("ana@exemplo.com"));
    }

    #[test]
    fn venda_com_falha_da_meta_responde_502() {
        let falso = falso(vec![resp(
            400,
            "{\"error\":{\"message\":\"token inválido\"}}",
        )]);
        let l = sobe(&com_envio(&falso.base));
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let r = pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=Purchase&pedido=9&valor=10&u=https%3A%2F%2Floja.com%2F&email=ana%40exemplo.com",
            &[&consent],
        );
        assert_eq!(r.status, 502);
        assert!(r.corpo.contains("falhou"));
        assert!(!r.corpo.contains("TOKEN_DE_TESTE"));
    }

    #[test]
    fn venda_sem_identificador_e_invalida() {
        // com IP e user agent ligados, a Meta aceita esses dois como identificador; sem eles, não há nada
        let mut ajustes = ajustes_base();
        ajustes.sem_ip_e_user_agent = true;
        let l = sobe(&ajustes);
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let r = pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=Purchase&pedido=9&u=https%3A%2F%2Floja.com%2F",
            &[&consent],
        );
        assert_eq!(r.status, 422);
        assert!(r.corpo.contains("nenhum dado do cliente"));
    }

    #[test]
    fn venda_com_campos_ruins_da_400() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let url = format!("{}/evento", l.endereco);
        let casos = [
            (
                "evento=Nada&pedido=1&u=https%3A%2F%2Fa.com%2F",
                "evento_invalido",
            ),
            ("evento=Lead&u=https%3A%2F%2Fa.com%2F", "pedido_faltando"),
            ("evento=Lead&pedido=1", "pagina_faltando"),
            (
                "evento=Lead&pedido=1&u=https%3A%2F%2Fa.com%2F&valor=1,234",
                "valor_invalido",
            ),
        ];
        for (corpo, esperado) in casos {
            let r = pedir_form(&url, corpo, &[&consent]);
            assert_eq!(r.status, 400, "{corpo}");
            assert!(r.corpo.contains(esperado), "{corpo}: {}", r.corpo);
        }
    }

    #[test]
    fn venda_usa_o_fbc_do_cookie_e_o_fbp_valido() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let r = pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=PageView&pedido=p1&u=https%3A%2F%2Fa.com%2F&email=ana%40exemplo.com",
            &[&format!(
                "{consent}; _fbc=fb.1.1554763741205.AbCd; _fbp=fb.1.1558571054389.1098115397"
            )],
        );
        assert_eq!(r.status, 200);
        assert!(r.corpo.contains("\"fbc\":\"fb.1.1554763741205.AbCd\""));
        assert!(
            r.corpo
                .contains("\"fbp\":\"fb.1.1558571054389.1098115397\"")
        );
    }

    #[test]
    fn venda_monta_o_fbc_do_fbclid_do_cookie_de_atribuicao() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let pixel = pedir_get(
            &format!(
                "{}/r?u={}",
                l.endereco,
                codificar("https://loja.com/?fbclid=CLIQUE123")
            ),
            &[&consent],
        );
        let cookie = pixel.cookie_rc_atrib().expect("cookie");
        let r = pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=Purchase&pedido=7&valor=5&u=https%3A%2F%2Floja.com%2F&email=ana%40exemplo.com",
            &[&consent, &cookie_de(&format!("rc_atrib={cookie}"))],
        );
        assert!(r.corpo.contains("\"fbc\":\"fb."), "{}", r.corpo);
        assert!(r.corpo.contains("CLIQUE123"));
        let registro = &l.estado.retrato().registros[0];
        assert_eq!(registro.origem, "clid, sem UTM");
    }

    #[test]
    fn venda_sem_ip_e_user_agent_quando_a_opcao_manda() {
        let mut ajustes = ajustes_base();
        ajustes.sem_ip_e_user_agent = true;
        let l = sobe(&ajustes);
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let r = pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=Purchase&pedido=7&u=https%3A%2F%2Floja.com%2F&email=ana%40exemplo.com",
            &[&consent],
        );
        assert!(!r.corpo.contains("client_ip_address"), "{}", r.corpo);
        assert!(!r.corpo.contains("client_user_agent"), "{}", r.corpo);
    }

    #[test]
    fn json_de_borda_nao_traz_campo_sem_valor() {
        let l = sobe(&ajustes_base());
        let r = pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=Lead&pedido=L1&u=https%3A%2F%2Floja.com%2F&email=ana%40exemplo.com",
            &[&cookie_de(&format!("{CONSENTIMENTO}=1"))],
        );
        assert!(
            !r.corpo.contains("null"),
            "o JSON de fora não leva nulo: {}",
            r.corpo
        );
        assert!(
            r.corpo.contains("\"resultado\":\"demonstracao\""),
            "{}",
            r.corpo
        );
    }

    #[test]
    fn retrato_mostra_do_mais_novo_para_o_mais_antigo_e_respeita_o_limite() {
        let mut ajustes = ajustes_base();
        ajustes.memoria = 2;
        let l = sobe(&ajustes);
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        for origem in ["a", "b", "c"] {
            pedir_get(
                &format!(
                    "{}/r?u={}",
                    l.endereco,
                    codificar(&format!("https://loja.com/?utm_source={origem}"))
                ),
                &[&consent],
            );
        }
        let retrato = l.estado.retrato();
        assert_eq!(retrato.origens.len(), 2);
        assert_eq!(retrato.origens[0].captura.utm.source.as_deref(), Some("c"));
    }

    #[test]
    fn resumo_e_resposta_cobrem_cada_desfecho() {
        use crate::consentimento::Motivo;
        use crate::envio::Resultado as R;
        let enviado = Desfecho::Resultado(R::Enviado {
            tentativas: 2,
            recebidos: Some(1),
        });
        assert_eq!(resumo(&enviado), "enviado em 2 tentativa(s)");
        assert_eq!(resposta_da_venda(&enviado).0, 200);
        let recusado = Desfecho::Resultado(R::Recusado(Motivo::SemConsentimento));
        assert_eq!(resumo(&recusado), "recusado: sem_consentimento");
        let invalido = Desfecho::Resultado(R::Invalido(evento::ErroEvento::Antigo));
        assert!(resumo(&invalido).starts_with("inválido:"));
        assert_eq!(resposta_da_venda(&invalido).0, 422);
        let falhou = Desfecho::Resultado(R::Falhou {
            tentativas: 3,
            motivo: "HTTP 500".to_string(),
        });
        assert_eq!(resumo(&falhou), "falhou: HTTP 500");
        assert_eq!(resposta_da_venda(&falhou).0, 502);
        assert!(
            resposta_da_venda(&recusado)
                .1
                .contains("resultado:\"recusado\"")
        );
    }

    #[test]
    fn origem_do_visitante_cobre_os_casos() {
        assert_eq!(origem_do_visitante(&Atribuicao::default()), "sem origem");
        let captura = captura::de_url("https://a.com/?utm_source=fb&utm_medium=cpc&utm_campaign=x");
        let mut atrib = Atribuicao::default();
        atrib.registrar(captura, 1);
        assert_eq!(origem_do_visitante(&atrib), "fb / cpc / x");
    }

    #[test]
    fn formatar_valor_e_buscar_e_cookies() {
        assert_eq!(formatar_valor(1234, "brl"), "12,34 BRL");
        assert_eq!(formatar_valor(5, "usd"), "0,05 USD");
        let pares = vec![
            ("a".to_string(), "  ".to_string()),
            ("b".to_string(), "1".to_string()),
        ];
        assert!(buscar(&pares, "a").is_none());
        assert_eq!(buscar(&pares, "b"), Some("1"));
        assert!(!consentido(&[]));
    }

    #[test]
    fn pagina_segura_troca_url_ruim() {
        assert_eq!(pagina_segura("https://a.com/x?y=1"), "https://a.com/x");
        assert_eq!(pagina_segura("nao é url"), "(página inválida)");
    }

    #[test]
    fn valor_do_cookie_de_atribuicao_com_lixo_nao_passa() {
        let l = sobe(&ajustes_base());
        let r = pedir_get(
            &format!(
                "{}/r?u={}",
                l.endereco,
                codificar("https://a.com/?utm_source=x")
            ),
            &[&cookie_de(&format!(
                "{CONSENTIMENTO}=1; rc_atrib=lixo.lixo"
            ))],
        );
        assert!(
            r.cookie_rc_atrib().is_some(),
            "cookie lixo é ignorado e um novo é gravado"
        );
    }

    #[test]
    fn servir_volta_quando_atinge_o_maximo_de_pedidos() {
        let estado = Arc::new(Estado::de(&ajustes_base()).expect("estado"));
        let servidor = abrir("127.0.0.1", "0").expect("porta");
        let endereco = format!("http://{}", servidor.server_addr().to_ip().expect("ip"));
        let (s2, e2) = (servidor.clone(), estado.clone());
        let fio = thread::spawn(move || servir(&s2, &e2, Some(1)));
        let r = pedir_get(&format!("{endereco}/"), &[]);
        assert_eq!(r.status, 200);
        fio.join().expect("servir terminou");
    }

    /// Codifica para URL: tudo que não é letra, número ou `-_.~` vira `%XX`.
    fn codificar(texto: &str) -> String {
        texto
            .bytes()
            .map(|b| {
                if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02X}")
                }
            })
            .collect()
    }

    #[test]
    fn helper_de_codificacao_e_respostas_falsas_existem() {
        assert_eq!(codificar("a b"), "a%20b");
        assert_eq!(resp(200, "{}").status, 200);
    }

    #[test]
    fn venda_em_jsonw_entra_e_responde_em_jsonw() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let corpo = r#"{evento:"Purchase",pedido:"1001",u:"https://loja.com/obrigado",valor:99.90 BRL,email:"John_Smith@gmail.com"}"#;
        let r = pedir_com(
            "POST",
            &format!("{}/evento", l.endereco),
            Some(corpo),
            &[("Content-Type", "text/jsonw"), ("Accept", "text/jsonw")],
            &[&consent],
        );
        assert_eq!(r.status, 200, "{}", r.corpo);
        assert!(
            r.tipo
                .as_deref()
                .is_some_and(|t| t.starts_with("text/jsonw"))
        );
        assert!(
            r.corpo.contains("resultado:\"demonstracao\""),
            "{}",
            r.corpo
        );
        assert!(!r.corpo.contains("John_Smith"), "o e-mail não volta");
        let registro = &l.estado.retrato().registros[0];
        assert_eq!(registro.valor.as_deref(), Some("99,90 BRL"));
        assert_eq!(registro.valor_centavos, Some(9990));
        assert_eq!(registro.moeda.as_deref(), Some("BRL"));
    }

    #[test]
    fn venda_em_jsonw_que_nao_casa_da_400_sem_eco_do_dado() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let r = pedir_com(
            "POST",
            &format!("{}/evento", l.endereco),
            Some(r#"{pedido:"1",u:"https://a.com/",cartao:"4111111111111111"}"#),
            &[("Content-Type", "text/jsonw")],
            &[&consent],
        );
        assert_eq!(r.status, 400);
        assert!(
            r.corpo.contains("\"resultado\":\"jsonw_invalido\""),
            "{}",
            r.corpo
        );
        assert!(!r.corpo.contains("4111"), "o dado não volta: {}", r.corpo);
    }

    #[test]
    fn venda_com_corpo_json_tambem_entra() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        let corpo =
            r#"{"evento":"Lead","pedido":"L1","u":"https://a.com/","email":"ana@exemplo.com"}"#;
        let r = pedir_com(
            "POST",
            &format!("{}/evento", l.endereco),
            Some(corpo),
            &[("Content-Type", "application/json")],
            &[&consent],
        );
        assert_eq!(r.status, 200, "{}", r.corpo);
        assert!(
            r.corpo.contains("\"resultado\":\"demonstracao\""),
            "{}",
            r.corpo
        );
        assert!(!r.corpo.contains("ana@"));
    }

    #[test]
    fn registros_saem_em_json_jsonw_e_jsw() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        pedir_get(
            &format!(
                "{}/r?u={}",
                l.endereco,
                codificar("https://loja.com/?utm_source=facebook&fbclid=CLIQUE123")
            ),
            &[&consent],
        );
        pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=Purchase&pedido=1001&valor=99,90&u=https%3A%2F%2Floja.com%2Fobrigado&email=ana%40exemplo.com",
            &[&consent],
        );
        let json = pedir_get(&format!("{}/registros/eventos", l.endereco), &[]);
        assert_eq!(json.status, 200);
        assert!(json.corpo.starts_with('['), "{}", json.corpo);
        assert!(json.corpo.contains("\"pedido\":\"1001\""), "{}", json.corpo);
        let texto_jsonw = pedir_com(
            "GET",
            &format!("{}/registros/eventos?formato=jsonw", l.endereco),
            None,
            &[],
            &[],
        );
        assert!(
            texto_jsonw.corpo.contains("pedido:\"1001\""),
            "{}",
            texto_jsonw.corpo
        );
        assert!(
            texto_jsonw.corpo.contains("valor:99.90 BRL"),
            "{}",
            texto_jsonw.corpo
        );
        let jsw = pedir_com(
            "GET",
            &format!("{}/registros/eventos", l.endereco),
            None,
            &[("Accept", "application/jsonw")],
            &[],
        );
        assert!(jsw.bytes.starts_with(b"JSW1"), "marca do .jsw");
        assert!(
            jsw.tipo
                .as_deref()
                .is_some_and(|t| t.starts_with("application/jsonw"))
        );
        assert!(
            jsw.cabecalho("content-disposition")
                .is_some_and(|c| c.contains("eventos.jsw"))
        );
    }

    #[test]
    fn clid_nao_sai_na_exportacao_so_o_hash() {
        let l = sobe(&ajustes_base());
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        pedir_get(
            &format!(
                "{}/r?u={}",
                l.endereco,
                codificar("https://loja.com/?utm_source=facebook&fbclid=CLIQUE123")
            ),
            &[&consent],
        );
        let r = pedir_com(
            "GET",
            &format!("{}/registros/origens?formato=jsonw", l.endereco),
            None,
            &[],
            &[],
        );
        assert_eq!(r.status, 200);
        assert!(
            !r.corpo.contains("CLIQUE123"),
            "o clid não sai: {}",
            r.corpo
        );
        assert!(
            r.corpo.contains(&format!(
                "fbclid_hash:\"{}\"",
                crate::dados::sha256_hex("CLIQUE123")
            )),
            "{}",
            r.corpo
        );
    }

    #[test]
    fn consentimento_com_formato_responde_o_corpo_e_grava_o_cookie() {
        let l = sobe(&ajustes_base());
        let r = pedir_com(
            "GET",
            &format!("{}/consentimento?aceitar=sim&formato=jsonw", l.endereco),
            None,
            &[],
            &[],
        );
        assert_eq!(r.status, 200);
        assert!(r.corpo.contains("consentimento:\"sim\""), "{}", r.corpo);
        assert!(
            r.cookie_por_nome(CONSENTIMENTO)
                .is_some_and(|c| c.starts_with("rc_consent=1"))
        );
    }

    #[test]
    fn rota_desconhecida_responde_no_formato_pedido() {
        let l = sobe(&ajustes_base());
        let r = pedir_com(
            "GET",
            &format!("{}/nada?formato=jsonw", l.endereco),
            None,
            &[],
            &[],
        );
        assert_eq!(r.status, 404);
        assert!(r.corpo.contains("resultado:\"nao_existe\""), "{}", r.corpo);
    }

    #[test]
    fn gravar_arquivos_publica_eventos_e_origens_que_a_libjsonw_relê() {
        let p = pasta_de_teste("arquivos");
        let mut ajustes = ajustes_base();
        ajustes.arquivo = Some(p.join("registros.jsw").display().to_string());
        let l = sobe(&ajustes);
        let consent = cookie_de(&format!("{CONSENTIMENTO}=1"));
        pedir_get(
            &format!(
                "{}/r?u={}",
                l.endereco,
                codificar("https://loja.com/?utm_source=fb")
            ),
            &[&consent],
        );
        pedir_form(
            &format!("{}/evento", l.endereco),
            "evento=Purchase&pedido=7&valor=5&u=https%3A%2F%2Floja.com%2F&email=ana%40exemplo.com",
            &[&consent],
        );
        l.estado.gravar_arquivos().expect("grava");
        let eventos =
            wortic_base::arquivo::ler(&p.join("registros.jsw"), true).expect("relê eventos");
        assert_eq!(eventos.len(), 1);
        assert!(eventos[0].contains("pedido:\"7\""), "{}", eventos[0]);
        let origens =
            wortic_base::arquivo::ler(&p.join("origens.jsw"), true).expect("relê origens");
        assert_eq!(origens.len(), 1);
        assert!(origens[0].contains("fonte:\"fb\""), "{}", origens[0]);
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn gravacao_periodica_so_grava_depois_de_n_registros() {
        // sem servidor: o estado recebe as origens direto, e a gravação é decidida aqui, sem corrida com outra thread
        let p = pasta_de_teste("cada");
        let mut ajustes = ajustes_base();
        ajustes.arquivo = Some(p.join("registros.jsw").display().to_string());
        ajustes.arquivo_a_cada = 2;
        let estado = Estado::de(&ajustes).expect("estado");
        let origem = |fonte: &str| Origem {
            quando_ms: 1_700_000_000_000,
            pagina: "https://loja.com/".to_string(),
            captura: captura::de_url(&format!("https://loja.com/?utm_source={fonte}")),
        };
        estado.guardar_origem(origem("a"));
        estado.gravar_se_preciso().expect("ok");
        assert!(!p.join("origens.jsw").exists(), "um registro não basta");
        estado.guardar_origem(origem("b"));
        estado.gravar_se_preciso().expect("ok");
        let origens = wortic_base::arquivo::ler(&p.join("origens.jsw"), true).expect("relê");
        assert_eq!(origens.len(), 2);
        let _ = std::fs::remove_dir_all(&p);
    }
}
