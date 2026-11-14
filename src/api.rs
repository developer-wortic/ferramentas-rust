//! A conversa com a API aberta da Wortic (https://cnpj.wortic.com), com guarda em memória.
//! Sem chave valem 60 consultas por minuto por IP. Com WORTIC_API_KEY vale o limite do seu plano.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const BASE: &str = "https://cnpj.wortic.com";
const GUARDA: Duration = Duration::from_secs(24 * 3600);

/// O que voltou da API: o código HTTP, o corpo como veio e, no 429, quantos segundos esperar.
pub struct Resposta {
    pub status: u16,
    pub corpo: String,
    pub espere: Option<String>,
}

pub struct Api {
    chave: Option<String>,
    guarda: Mutex<HashMap<String, (Instant, String)>>,
}

impl Api {
    pub fn nova() -> Self {
        let chave = std::env::var("WORTIC_API_KEY").ok().filter(|c| !c.is_empty());
        Api { chave, guarda: Mutex::new(HashMap::new()) }
    }

    /// GET no caminho pedido. Resposta 200 fica guardada por um dia: a segunda consulta nem sai daqui.
    pub fn buscar(&self, caminho: &str) -> Resposta {
        if let Some((quando, corpo)) = self.guarda.lock().unwrap().get(caminho) {
            if quando.elapsed() < GUARDA {
                return Resposta { status: 200, corpo: corpo.clone(), espere: None };
            }
        }
        let mut pedido = ureq::get(&format!("{BASE}{caminho}")).timeout(Duration::from_secs(10));
        if let Some(chave) = &self.chave {
            pedido = pedido.set("x-api-key", chave);
        }
        match pedido.call() {
            Ok(r) => {
                let corpo = r.into_string().unwrap_or_default();
                self.guarda.lock().unwrap().insert(caminho.to_string(), (Instant::now(), corpo.clone()));
                Resposta { status: 200, corpo, espere: None }
            }
            Err(ureq::Error::Status(status, r)) => {
                let espere = r.header("retry-after").map(str::to_string);
                Resposta { status, corpo: r.into_string().unwrap_or_default(), espere }
            }
            Err(_) => Resposta {
                status: 502,
                corpo: r#"{"erro":{"codigo":"sem_rede","mensagem":"Não consegui falar com a API agora."}}"#.to_string(),
                espere: None,
            },
        }
    }
}
