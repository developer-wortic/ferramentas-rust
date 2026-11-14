//! Cadastro de clientes que se preenche pelo CNPJ: modelo da Wortic em Rust.
//!
//! Um binário só. Serve a tela (static/index.html, embutida) e três rotas:
//!   GET /api/empresa/{cnpj}   ficha da empresa (confere o CNPJ aqui e só então consulta a API)
//!   GET /api/socios/{cnpj}    sócios da empresa
//!   GET /api/empresas?nome=   empresas pelo nome (4 letras ou mais)
//! Rode com `cargo run --release` e abra http://127.0.0.1:8080
//! Variáveis: PORTA (padrão 8080), WORTIC_API_KEY (opcional, para volume acima do uso aberto).

mod api;
mod cnpj;

use std::sync::Arc;
use tiny_http::{Header, Response, Server};

const TELA: &str = include_str!("../static/index.html");

fn cabecalho(nome: &str, valor: &str) -> Header {
    Header::from_bytes(nome.as_bytes(), valor.as_bytes()).expect("cabeçalho válido")
}

fn responder_api(pedido: tiny_http::Request, r: api::Resposta) {
    let mut resposta = Response::from_string(r.corpo)
        .with_status_code(r.status)
        .with_header(cabecalho("content-type", "application/json; charset=utf-8"));
    if let Some(segundos) = r.espere {
        resposta = resposta.with_header(cabecalho("retry-after", &segundos));
    }
    let _ = pedido.respond(resposta);
}

fn erro(pedido: tiny_http::Request, status: u16, codigo: &str, mensagem: &str) {
    let corpo = format!(r#"{{"erro":{{"codigo":"{codigo}","mensagem":"{mensagem}"}}}}"#);
    responder_api(pedido, api::Resposta { status, corpo, espere: None });
}

fn atender(pedido: tiny_http::Request, api: &api::Api) {
    let url = pedido.url().to_string();
    let (caminho, consulta) = url.split_once('?').unwrap_or((url.as_str(), ""));
    if caminho == "/" {
        let _ = pedido.respond(Response::from_string(TELA).with_header(cabecalho("content-type", "text/html; charset=utf-8")));
    } else if let Some(resto) = caminho.strip_prefix("/api/empresa/").or_else(|| caminho.strip_prefix("/api/socios/")) {
        let numero = cnpj::limpar(&cnpj::sem_porcento(resto));
        if !cnpj::valido(&numero) {
            return erro(pedido, 400, "cnpj_invalido", "CNPJ inválido: confira os dígitos.");
        }
        let fim = if caminho.starts_with("/api/socios/") { "/socios" } else { "" };
        responder_api(pedido, api.buscar(&format!("/cnpj/{numero}{fim}")));
    } else if caminho == "/api/empresas" {
        // só as letras, números, espaço e os sinais comuns passam adiante
        let limpa: String = consulta.chars().filter(|c| c.is_ascii_alphanumeric() || "=&%+-._".contains(*c)).collect();
        responder_api(pedido, api.buscar(&format!("/empresas?{limpa}")));
    } else {
        erro(pedido, 404, "nao_existe", "Rota não encontrada.");
    }
}

fn main() {
    let porta = std::env::var("PORTA").unwrap_or_else(|_| "8080".to_string());
    let servidor = Arc::new(Server::http(format!("127.0.0.1:{porta}")).expect("não consegui abrir a porta"));
    let api = Arc::new(api::Api::nova());
    println!("Cadastro pelo CNPJ no ar em http://127.0.0.1:{porta}");
    let mut linhas = Vec::new();
    for _ in 0..4 {
        let (servidor, api) = (servidor.clone(), api.clone());
        linhas.push(std::thread::spawn(move || {
            for pedido in servidor.incoming_requests() {
                atender(pedido, &api);
            }
        }));
    }
    for linha in linhas {
        let _ = linha.join();
    }
}
