//! Os esquemas jsonW do Pix e a saída de um registro no formato pedido. O dado entre as partes do programa é jsonW:
//! cada registro nasce como jsonW-texto, a libjsonw confere e tipa pelo esquema, e só na borda sai JSON, para a tela
//! e para quem não pede jsonW.

use wortic_base::{Formato, canonico, texto};

/// Uma cobrança estática: o copia e cola, o QR (`null` quando não foi pedido), o tipo e o valor da chave, o recebedor,
/// o valor e o txid. O valor é `dinheiro` (BRL, a moeda 986 do BR Code): o jsonW leva a moeda junto e o JSON sai como
/// texto "10.50 BRL", sem perder os centavos. `decimal[2]` foi descartado: a libjsonw o devolve como número JSON
/// (10.5 na tela), que perde o zero e quebra a conta do navegador.
pub const ESQUEMA_COBRANCA: &str = "codigo:texto,qr_svg:texto?,tipo_chave:texto,chave:texto,nome:texto,cidade:texto,valor:dinheiro?,txid:texto?,descricao:texto?";

/// Um copia e cola lido: a chave, o valor, o recebedor, o txid e todos os campos do primeiro nível. O valor fica como
/// texto ("10.50"), como o código o diz e como `/api/ler` sempre entregou: a API pública não muda de forma.
pub const ESQUEMA_LEITURA: &str = "chave:texto,descricao:texto?,valor:texto?,moeda:texto,pais:texto,nome:texto,cidade:texto,categoria:texto,txid:texto?,campos:lista<obj{id:texto,valor:texto}>";

/// Uma chave conferida: o valor normalizado e o tipo (CPF, CNPJ, e-mail, telefone ou chave aleatória).
pub const ESQUEMA_CHAVE: &str = "chave:texto,tipo_chave:texto";

/// Uma linha do lote: o que a pessoa escreve para gerar uma cobrança.
pub const ESQUEMA_ENTRADA: &str =
    "chave:texto,nome:texto,cidade:texto,valor:dinheiro?,txid:texto?,descricao:texto?";

/// Um erro: a linha do lote (`null` quando não é de uma linha só) e a mensagem em português.
pub const ESQUEMA_ERRO: &str = "erro:obj{linha:i64?,mensagem:texto}";

/// A configuração da API: se há `WORTIC_API_KEY`. A chave em si nunca sai.
pub const ESQUEMA_CONFIG: &str = "chave:bool";

/// O resumo de um lote gravado: quantos registros e onde ficou o arquivo.
pub const ESQUEMA_LOTE: &str = "registros:i64,arquivo:texto";

/// O registro (jsonW-texto) no formato pedido. jsonW-texto sai tipado pela libjsonw; JSON sai da mesma conferência,
/// só para a borda.
///
/// # Errors
///
/// O registro não casa com o esquema. Isso é erro de quem montou o registro, não de quem pediu.
pub fn registro(formato: Formato, esquema: &str, registro: &str) -> Result<String, String> {
    match formato {
        // borda-json: navegador. A tela embutida lê JSON, e quem não pede jsonW também recebe JSON.
        Formato::Json => texto::para_json(esquema, registro),
        Formato::Texto | Formato::Arquivo => canonico::canonico(esquema, registro)
            .map(|linha| linha.trim_end().to_string())
            .map_err(razao),
    }
}

/// Tira a posição que a ABI põe na frente da razão (`registro 0: ...`), para a mensagem sair limpa.
#[must_use]
pub fn razao(erro: String) -> String {
    match erro
        .strip_prefix("registro ")
        .and_then(|resto| resto.split_once(": "))
    {
        Some((n, resto)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
            resto.to_string()
        }
        _ => erro,
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::chave::Chave;
    use crate::cobranca::Cobranca;
    use crate::{leitura, registro as reg};
    use wortic_base::arquivo;

    fn exemplo_cobranca() -> String {
        let c = Cobranca::nova("123.456.789-09", "Loja Exemplo", "Sao Paulo")
            .unwrap()
            .com_valor("10,50")
            .unwrap()
            .com_txid("PED42")
            .unwrap()
            .com_descricao("Pedido 42")
            .unwrap();
        reg::cobranca(&c, Some("<svg/>")).unwrap()
    }

    /// A ida e a volta pela libjsonw: o jsonW vira JSON (`para_json`) e o JSON volta para o mesmo jsonW tipado.
    fn ida_e_volta(esquema: &str, registro: &str) {
        let json = texto::para_json(esquema, registro).unwrap();
        let antes = arquivo::texto(esquema, [registro]).unwrap();
        let depois = arquivo::texto(esquema, [json.as_str()]).unwrap();
        assert_eq!(antes, depois, "jsonW: {registro}");
        assert_eq!(texto::para_json(esquema, &antes).unwrap(), json);
    }

    #[test]
    fn cobranca_faz_ida_e_volta_pela_libjsonw() {
        ida_e_volta(ESQUEMA_COBRANCA, &exemplo_cobranca());
    }

    #[test]
    fn cobranca_sem_valor_nem_qr_faz_ida_e_volta() {
        let c = Cobranca::nova("pix@exemplo.com.br", "Ana", "Recife").unwrap();
        let registro = reg::cobranca(&c, None).unwrap();
        assert!(registro.contains("valor:null"));
        ida_e_volta(ESQUEMA_COBRANCA, &registro);
    }

    #[test]
    fn leitura_faz_ida_e_volta_pela_libjsonw() {
        let codigo = Cobranca::nova("123.456.789-09", "Ana", "Recife")
            .unwrap()
            .com_valor("25,90")
            .unwrap()
            .codigo()
            .unwrap();
        let registro = reg::leitura(&leitura::ler(&codigo).unwrap());
        ida_e_volta(ESQUEMA_LEITURA, &registro);
    }

    #[test]
    fn chave_erro_config_e_lote_fazem_ida_e_volta() {
        let chave = Chave::interpretar("contato@exemplo.com.br").unwrap();
        ida_e_volta(ESQUEMA_CHAVE, &reg::chave(&chave));
        ida_e_volta(ESQUEMA_ERRO, &reg::erro(Some(3), "linha ruim."));
        ida_e_volta(ESQUEMA_ERRO, &reg::erro(None, "sem linha."));
        ida_e_volta(ESQUEMA_CONFIG, &reg::config(true));
        ida_e_volta(ESQUEMA_CONFIG, &reg::config(false));
        ida_e_volta(ESQUEMA_LOTE, &reg::lote(2, "saida.jsw"));
    }

    #[test]
    fn json_e_borda_e_jsonw_tira_as_aspas() {
        let registro = exemplo_cobranca();
        let json = registro_json(&registro);
        assert!(json.starts_with("{\"codigo\":\"000201"), "{json}");
        assert!(json.contains("\"valor\":\"10.50 BRL\""), "{json}");
        let texto_jsonw = registro_texto(&registro);
        assert!(texto_jsonw.starts_with("{codigo:\"000201"), "{texto_jsonw}");
        assert!(texto_jsonw.contains("valor:10.50 BRL"), "{texto_jsonw}");
        assert!(!texto_jsonw.ends_with('\n'));
    }

    #[test]
    fn arquivo_pedido_num_registro_cai_para_jsonw_texto() {
        let registro = exemplo_cobranca();
        assert_eq!(
            super::registro(Formato::Arquivo, ESQUEMA_COBRANCA, &registro).unwrap(),
            registro_texto(&registro)
        );
    }

    #[test]
    fn registro_fora_do_esquema_da_razao_sem_posicao() {
        let erro = super::registro(
            Formato::Texto,
            ESQUEMA_CHAVE,
            "{chave:1,tipo_chave:\"CPF\"}",
        )
        .unwrap_err();
        assert!(!erro.starts_with("registro "), "{erro}");
        assert!(super::registro(Formato::Json, ESQUEMA_CHAVE, "{chave:1}").is_err());
    }

    #[test]
    fn razao_tira_so_a_posicao_da_abi() {
        assert_eq!(
            razao("registro 0: nota não é número".into()),
            "nota não é número"
        );
        assert_eq!(razao("registro 12: x: y".into()), "x: y");
        assert_eq!(razao("registro x: y".into()), "registro x: y");
        assert_eq!(razao("sem posição".into()), "sem posição");
    }

    fn registro_json(registro: &str) -> String {
        super::registro(Formato::Json, ESQUEMA_COBRANCA, registro).unwrap()
    }

    fn registro_texto(registro: &str) -> String {
        super::registro(Formato::Texto, ESQUEMA_COBRANCA, registro).unwrap()
    }
}
