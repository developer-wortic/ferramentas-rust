//! Os esquemas do jsonW desta ferramenta, declarados como constantes públicas. A libjsonw confere cada registro contra o
//! esquema (tipo, campo obrigatório, `?` opcional). `+pessoal` esconde a coluna de quem não tem o escopo; por isso os
//! dados do cliente só entram nos registros em hash.
//!
//! Tipos: `tempo` (µs UTC), `dinheiro` (`12.50 BRL`, com a moeda no valor), `codigo` (bytes exatos), `dic` (texto
//! repetido), `texto`, `i64`, `f64`, `lista<..>` e `obj{..}`.

/// Os campos do `user_data` em hash, como vão para a Meta (cada dado hasheado é uma lista com um hash).
macro_rules! usuario_campos {
    () => {
        "em:lista<codigo>?,ph:lista<codigo>?,fn:lista<codigo>?,ln:lista<codigo>?,db:lista<codigo>?,ct:lista<codigo>?,st:lista<codigo>?,zp:lista<codigo>?,country:lista<codigo>?,external_id:lista<codigo>?,client_ip_address:texto?,client_user_agent:texto?,fbc:texto?,fbp:texto?"
    };
}

/// Os campos de um evento montado para a Conversions API (sem o envelope `data` do lote).
macro_rules! evento_campos {
    () => {
        concat!(
            "event_name:dic,event_time:i64,event_id:codigo,action_source:dic,event_source_url:texto,user_data:obj{",
            usuario_campos!(),
            "},custom_data:obj{value:f64,currency:dic}?"
        )
    };
}

/// Uma origem: a página com UTM ou clid, como sai em `/registros/origens`, no `.jsw` e no CLI. Os clids saem em hash.
pub const ORIGEM: &str = "tempo:tempo,pagina:texto,fonte:dic?,meio:dic?,campanha:texto?,conteudo:texto?,termo:texto?,fbclid_hash:codigo+pessoal?,gclid_hash:codigo+pessoal?,ttclid_hash:codigo+pessoal?,msclkid_hash:codigo+pessoal?";

/// Um evento guardado (o que `/evento` respondeu): quando chegou, qual evento, o pedido, o valor, a origem e a situação.
pub const REGISTRO: &str =
    "tempo:tempo,evento:dic,pedido:codigo,valor:dinheiro?,origem:texto,situacao:texto";

/// A venda que entra por `POST /evento` em jsonW-texto ou em JSON. Os campos são os do formulário; o valor leva a moeda.
pub const VENDA: &str = "evento:dic?,pedido:codigo,u:texto,valor:dinheiro?,email:texto?,telefone:texto?,nome:texto?,sobrenome:texto?,cidade:texto?,uf:texto?,cep:texto?,pais:texto?,nascimento:texto?,cliente:texto?";

/// A entrada de `wortic rastreio evento`: a venda, mais o que a Meta pede (IP, user agent, fbc, fbp), o DDI e o momento
/// (segundos desde a época Unix; sem ele, agora).
pub const EVENTO: &str = "evento:dic?,pedido:codigo,u:texto,valor:dinheiro?,email:texto?,telefone:texto?,nome:texto?,sobrenome:texto?,cidade:texto?,uf:texto?,cep:texto?,pais:texto?,nascimento:texto?,cliente:texto?,ip:texto?,user_agent:texto?,fbc:texto?,fbp:texto?,ddi:texto?,momento:i64?";

/// A entrada de `wortic rastreio dados`: os dados do cliente em claro. Só o hash sai.
pub const DADOS: &str = "email:texto?,telefone:texto?,nome:texto?,sobrenome:texto?,cidade:texto?,uf:texto?,cep:texto?,pais:texto?,nascimento:texto?,cliente:texto?,ddi:texto?";

/// O `user_data` em hash (`wortic rastreio dados`).
pub const DADOS_HASH: &str = usuario_campos!();

/// Um evento montado para a Conversions API (`wortic rastreio evento`).
pub const EVENTO_MONTADO: &str = evento_campos!();

/// A resposta de `/evento` e de `/consentimento`, e o erro de qualquer rota. Quem não tem o campo, não o recebe.
pub const RESPOSTA: &str = concat!(
    "resultado:texto,motivo:texto?,tentativas:i64?,recebidos:i64?,descricao:texto?,consentimento:texto?,evento:obj{",
    evento_campos!(),
    "}?"
);

/// A captura de uma URL (`wortic rastreio captura`): a página e o que ela trouxe, com cada clid mascarado.
pub const CAPTURA: &str = "pagina:texto,fonte:dic?,meio:dic?,campanha:texto?,conteudo:texto?,termo:texto?,fbclid:texto?,gclid:texto?,ttclid:texto?,msclkid:texto?";

/// O arquivo de configuração (`RASTREIO_CONFIG` ou `--config`): as mesmas chaves das variáveis de ambiente.
/// Porta, memória, prazo e limites são números; o resto é texto, como na variável.
pub const CONFIG: &str = "ENDERECO:texto?,PORTA:i64?,RASTREIO_SEGREDO:texto?,RASTREIO_PIXEL_ID:texto?,RASTREIO_ACCESS_TOKEN:texto?,RASTREIO_TEST_EVENT_CODE:texto?,RASTREIO_API_BASE:texto?,RASTREIO_MEMORIA:i64?,RASTREIO_PRAZO_DIAS:i64?,RASTREIO_HTTPS:texto?,RASTREIO_DDI:texto?,RASTREIO_INDICE_SUBDOMINIO:i64?,RASTREIO_SEM_IP_UA:texto?,WORTIC_API_KEY:texto?,RASTREIO_MAX_PEDIDOS:i64?,RASTREIO_ARQUIVO:texto?,RASTREIO_ARQUIVO_CADA:i64?";

#[cfg(test)]
mod testes {
    use super::*;
    use crate::jsonw;

    /// Ida e volta pela libjsonw: o texto conferido, mandado de novo pelo mesmo esquema, sai igual (ponto fixo).
    fn ida_e_volta(esquema: &str, registro: &str) -> String {
        let primeiro =
            jsonw::conferir(esquema, registro).unwrap_or_else(|r| panic!("{r}\n{registro}"));
        let segundo = jsonw::conferir(esquema, &primeiro).expect("o canônico passa de novo");
        assert_eq!(primeiro, segundo);
        primeiro
    }

    #[test]
    fn captura_volta_igual() {
        let canonico = ida_e_volta(
            CAPTURA,
            r#"{pagina:"https://loja.com/",fonte:"facebook",meio:null,campanha:"verao",fbclid:"AbCdEf…",gclid:null}"#,
        );
        let campos = jsonw::campos(&canonico).expect("campos");
        assert!(
            campos
                .iter()
                .any(|(k, v)| k == "fbclid" && v.como_texto() == Some("AbCdEf…"))
        );
    }

    #[test]
    fn dados_e_dados_hash_voltam_iguais() {
        ida_e_volta(
            DADOS,
            r#"{email:"Ana@Exemplo.com",telefone:"(11) 91234-5678",uf:"SP",cliente:"42"}"#,
        );
        ida_e_volta(
            DADOS_HASH,
            r#"{em:["62a14e44f765419d10fea99367361a727c12365e2520f32218d505ed9aa0f62f"],ph:null,fn:null,ln:null,db:null,ct:null,st:null,zp:null,country:null,external_id:null,client_ip_address:"203.0.113.7",client_user_agent:null,fbc:"fb.1.1700000000000.AbC",fbp:null}"#,
        );
    }

    #[test]
    fn evento_montado_volta_igual_com_e_sem_valor() {
        ida_e_volta(
            EVENTO_MONTADO,
            r#"{event_name:"Purchase",event_time:1700000000,event_id:"pedido-1001",action_source:"website",event_source_url:"https://loja.com/obrigado",user_data:{em:["62a1"]},custom_data:{value:99.9,currency:"BRL"}}"#,
        );
        ida_e_volta(
            EVENTO_MONTADO,
            r#"{event_name:"Lead",event_time:1700000000,event_id:"l-1",action_source:"website",event_source_url:"https://loja.com/",user_data:{ph:["aa"]},custom_data:null}"#,
        );
    }

    #[test]
    fn resposta_aceita_o_evento_aninhado() {
        ida_e_volta(
            RESPOSTA,
            r#"{resultado:"demonstracao",motivo:"sem envio",evento:{event_name:"Purchase",event_time:1700000000,event_id:"p1",action_source:"website",event_source_url:"https://loja.com/",user_data:{fbc:"fb.1.2.AbC"}}}"#,
        );
    }

    #[test]
    fn venda_leva_o_valor_em_dinheiro_com_a_moeda() {
        let canonico = ida_e_volta(
            VENDA,
            r#"{evento:"Purchase",pedido:"1001",u:"https://loja.com/obrigado",valor:99.90 BRL,email:"ana@exemplo.com"}"#,
        );
        assert!(canonico.contains("valor:99.90 BRL"), "{canonico}");
    }

    #[test]
    fn venda_sem_pedido_e_recusada_pelo_esquema() {
        assert!(jsonw::conferir(VENDA, r#"{u:"https://loja.com/"}"#).is_err());
    }

    #[test]
    fn config_aceita_numero_onde_e_numero_e_recusa_chave_que_nao_existe() {
        assert!(jsonw::conferir(CONFIG, "{PORTA:9000,RASTREIO_MEMORIA:200}").is_ok());
        assert!(jsonw::conferir(CONFIG, "{PORTAS:9000}").is_err());
    }

    #[test]
    fn origem_e_registro_voltam_iguais() {
        ida_e_volta(
            ORIGEM,
            r#"{tempo:1700000000000000,pagina:"https://loja.com/",fonte:"facebook",meio:null,campanha:"verao",conteudo:null,termo:null,fbclid_hash:"ab12",gclid_hash:null,ttclid_hash:null,msclkid_hash:null}"#,
        );
        ida_e_volta(
            REGISTRO,
            r#"{tempo:1700000000000000,evento:"Purchase",pedido:"1001",valor:"99.90 BRL",origem:"facebook / cpc / verao",situacao:"demonstração: nada foi enviado"}"#,
        );
    }
}
