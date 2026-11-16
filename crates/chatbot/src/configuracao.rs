//! Configuração do bot. Variáveis de ambiente têm prioridade sobre o arquivo `dados/configuracao.jsw.txt` (jsonW-texto),
//! onde a tela de administração guarda a chave, com permissão 600 (só o dono lê). O arquivo passa pelo esquema da
//! libjsonw (`wortic_base::config`), e a gravação é troca atômica.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::provedor::{Segredo, Tipo};

/// Nome do arquivo de configuração dentro da pasta de dados (jsonW-texto).
pub const ARQUIVO: &str = "configuracao.jsw.txt";

/// Esquema do arquivo de configuração. Tudo opcional: o ambiente ou o padrão completam o que faltar.
pub const ESQUEMA: &str = "provedor:texto?,modelo:texto?,chave:texto?";

/// Tudo o que o processo precisa para subir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuracao {
    pub provedor: Tipo,
    pub modelo: String,
    pub chave: Option<Segredo>,
    /// Endereço base alternativo. Só para testes e para proxies próprios.
    pub base: Option<String>,
    pub origens: Vec<String>,
    pub porta: u16,
    pub endereco: String,
    pub dados: PathBuf,
    pub roteiro: PathBuf,
}

/// O conteúdo do arquivo de configuração. Também é o que a migração do TOML antigo lê.
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Arquivo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provedor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modelo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chave: Option<String>,
}

impl Configuracao {
    /// Pasta de dados: `CHATBOT_DADOS`, ou `dados` no diretório atual.
    #[must_use]
    pub fn pasta_de_dados(ambiente: &HashMap<String, String>) -> PathBuf {
        PathBuf::from(
            ambiente
                .get("CHATBOT_DADOS")
                .map_or("dados", String::as_str),
        )
    }

    /// Lê o ambiente dado e o arquivo de dados. Sem variável, usa o padrão.
    ///
    /// # Errors
    ///
    /// Devolve a mensagem quando o provedor não existe, a porta é inválida ou o arquivo não lê.
    pub fn ler(ambiente: &HashMap<String, String>) -> Result<Self, String> {
        let dados = Self::pasta_de_dados(ambiente);
        let arquivo = ler_arquivo(&dados.join(ARQUIVO))?;
        let codigo = ambiente
            .get("CHATBOT_PROVEDOR")
            .cloned()
            .or(arquivo.provedor)
            .unwrap_or_else(|| "openai".to_string());
        let provedor =
            Tipo::de_codigo(&codigo).ok_or_else(|| format!("provedor desconhecido: {codigo}"))?;
        let modelo = ambiente
            .get("CHATBOT_MODELO")
            .cloned()
            .or(arquivo.modelo)
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| provedor.modelo_padrao().to_string());
        let chave = ambiente
            .get("CHATBOT_CHAVE")
            .cloned()
            .or(arquivo.chave)
            .filter(|c| !c.trim().is_empty())
            .map(Segredo::new);
        let base = ambiente
            .get("CHATBOT_BASE_URL")
            .cloned()
            .filter(|b| !b.trim().is_empty());
        let origens = ambiente
            .get("CHATBOT_ORIGENS")
            .map_or_else(Vec::new, |lista| {
                lista
                    .split(',')
                    .map(str::trim)
                    .filter(|o| !o.is_empty())
                    .map(String::from)
                    .collect()
            });
        let porta = match ambiente.get("PORTA") {
            Some(texto) => texto.parse().map_err(|_| {
                "não consegui abrir a porta: use um número de 1 a 65535".to_string()
            })?,
            None => 8080,
        };
        let endereco = ambiente
            .get("ENDERECO")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let roteiro = PathBuf::from(
            ambiente
                .get("CHATBOT_ROTEIRO")
                .map_or("roteiro.jsw.txt", String::as_str),
        );
        Ok(Self {
            provedor,
            modelo,
            chave,
            base,
            origens,
            porta,
            endereco,
            dados,
            roteiro,
        })
    }

    /// Grava a escolha do provedor, o modelo e a chave no arquivo jsonW-texto, com permissão 600.
    ///
    /// # Errors
    ///
    /// Devolve a mensagem quando a pasta ou o arquivo não podem ser escritos.
    pub fn salvar_escolha(
        dados: &Path,
        provedor: Tipo,
        modelo: &str,
        chave: &str,
    ) -> Result<(), String> {
        fs::create_dir_all(dados)
            .map_err(|e| format!("não consegui criar a pasta de dados: {e}"))?;
        let arquivo = Arquivo {
            provedor: Some(provedor.codigo().to_string()),
            modelo: Some(modelo.to_string()),
            chave: Some(chave.to_string()),
        };
        gravar_arquivo(&dados.join(ARQUIVO), &arquivo)
    }
}

/// Grava o arquivo de configuração com troca atômica e permissão 600 (a base cuida das duas coisas).
///
/// # Errors
///
/// Devolve a mensagem quando o arquivo não pode ser gravado.
pub(crate) fn gravar_arquivo(caminho: &Path, arquivo: &Arquivo) -> Result<(), String> {
    let json = serde_json::to_string(arquivo).expect("a configuração sempre vira JSON"); // borda-json: forma interna, sem JSON para fora
    let texto = format!(
        "// Configuração do chatbot. A administração (painel) escreve este arquivo. A chave fica aqui, com permissão 600.\n{}\n",
        wortic_base::texto::de_json(&json)
    );
    wortic_base::config::gravar(caminho, &texto, true)
        .map_err(|e| format!("não consegui gravar a configuração: {e}"))
}

/// Lê o arquivo pelo esquema. Ausente vale o padrão; qualquer erro (sintaxe, tipo, leitura) vira `configuração ilegível`.
fn ler_arquivo(caminho: &Path) -> Result<Arquivo, String> {
    match wortic_base::config::ler(caminho, ESQUEMA) {
        Ok(None) => Ok(Arquivo::default()),
        Ok(Some(json)) => {
            serde_json::from_str(&json) // borda-json: registro que a libjsonw já conferiu
                .map_err(|e| format!("configuração ilegível: {e}"))
        }
        Err(erro) => Err(format!("configuração ilegível: {erro}")),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn ambiente(pares: &[(&str, &str)]) -> HashMap<String, String> {
        pares
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn pasta_temporaria(nome: &str) -> PathBuf {
        let pasta = std::env::temp_dir().join(format!("chatbot-cfg-{nome}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&pasta);
        pasta
    }

    #[test]
    fn padroes_sem_nada_configurado() {
        let pasta = pasta_temporaria("padroes");
        let c =
            Configuracao::ler(&ambiente(&[("CHATBOT_DADOS", pasta.to_str().unwrap())])).unwrap();
        assert_eq!(c.provedor, Tipo::OpenAi);
        assert_eq!(c.modelo, Tipo::OpenAi.modelo_padrao());
        assert!(c.chave.is_none());
        assert_eq!(c.porta, 8080);
        assert_eq!(c.endereco, "127.0.0.1");
        assert_eq!(c.origens, Vec::<String>::new());
        assert_eq!(c.roteiro, PathBuf::from("roteiro.jsw.txt"));
        assert!(c.base.is_none());
    }

    #[test]
    fn ambiente_vence_o_arquivo_e_o_arquivo_vale_sem_ambiente() {
        let pasta = pasta_temporaria("prioridade");
        Configuracao::salvar_escolha(&pasta, Tipo::Groq, "modelo-do-arquivo", "chave-do-arquivo")
            .unwrap();
        let so_arquivo =
            Configuracao::ler(&ambiente(&[("CHATBOT_DADOS", pasta.to_str().unwrap())])).unwrap();
        assert_eq!(so_arquivo.provedor, Tipo::Groq);
        assert_eq!(so_arquivo.modelo, "modelo-do-arquivo");
        assert_eq!(
            so_arquivo.chave.as_ref().map(Segredo::expor),
            Some("chave-do-arquivo")
        );
        let com_ambiente = Configuracao::ler(&ambiente(&[
            ("CHATBOT_DADOS", pasta.to_str().unwrap()),
            ("CHATBOT_PROVEDOR", "anthropic"),
            ("CHATBOT_MODELO", "modelo-do-ambiente"),
            ("CHATBOT_CHAVE", "chave-do-ambiente"),
            ("CHATBOT_BASE_URL", "http://127.0.0.1:9"),
            (
                "CHATBOT_ORIGENS",
                " https://a.exemplo , ,https://b.exemplo ",
            ),
            ("PORTA", "9100"),
            ("ENDERECO", "0.0.0.0"),
            ("CHATBOT_ROTEIRO", "outro.jsw.txt"),
        ]))
        .unwrap();
        assert_eq!(com_ambiente.provedor, Tipo::Anthropic);
        assert_eq!(com_ambiente.modelo, "modelo-do-ambiente");
        assert_eq!(
            com_ambiente.chave.as_ref().map(Segredo::expor),
            Some("chave-do-ambiente")
        );
        assert_eq!(com_ambiente.base.as_deref(), Some("http://127.0.0.1:9"));
        assert_eq!(
            com_ambiente.origens,
            vec!["https://a.exemplo", "https://b.exemplo"]
        );
        assert_eq!(com_ambiente.porta, 9100);
        assert_eq!(com_ambiente.endereco, "0.0.0.0");
        assert_eq!(com_ambiente.roteiro, PathBuf::from("outro.jsw.txt"));
        let _ = fs::remove_dir_all(&pasta);
    }

    #[test]
    fn modelo_e_chave_em_branco_voltam_ao_padrao() {
        let pasta = pasta_temporaria("brancos");
        let c = Configuracao::ler(&ambiente(&[
            ("CHATBOT_DADOS", pasta.to_str().unwrap()),
            ("CHATBOT_PROVEDOR", "gemini"),
            ("CHATBOT_MODELO", "  "),
            ("CHATBOT_CHAVE", "   "),
            ("CHATBOT_BASE_URL", " "),
        ]))
        .unwrap();
        assert_eq!(c.modelo, Tipo::Gemini.modelo_padrao());
        assert!(c.chave.is_none());
        assert!(c.base.is_none());
    }

    #[test]
    fn provedor_desconhecido_e_porta_invalida_sao_recusados() {
        let pasta = pasta_temporaria("erros");
        let dados = pasta.to_str().unwrap();
        let provedor = Configuracao::ler(&ambiente(&[
            ("CHATBOT_DADOS", dados),
            ("CHATBOT_PROVEDOR", "nenhum"),
        ]));
        assert!(
            provedor
                .unwrap_err()
                .contains("provedor desconhecido: nenhum")
        );
        let porta = Configuracao::ler(&ambiente(&[("CHATBOT_DADOS", dados), ("PORTA", "abc")]));
        assert!(porta.unwrap_err().contains("não consegui abrir a porta"));
    }

    #[test]
    fn pasta_de_dados_vem_do_ambiente_ou_do_padrao() {
        assert_eq!(
            Configuracao::pasta_de_dados(&ambiente(&[])),
            PathBuf::from("dados")
        );
        assert_eq!(
            Configuracao::pasta_de_dados(&ambiente(&[("CHATBOT_DADOS", "outra")])),
            PathBuf::from("outra")
        );
    }

    #[test]
    fn arquivo_quebrado_e_recusado() {
        let pasta = pasta_temporaria("quebrado");
        fs::create_dir_all(&pasta).unwrap();
        fs::write(pasta.join(ARQUIVO), "provedor = [").unwrap();
        let erro = Configuracao::ler(&ambiente(&[("CHATBOT_DADOS", pasta.to_str().unwrap())]))
            .unwrap_err();
        assert!(erro.contains("configuração ilegível"));
        let _ = fs::remove_dir_all(&pasta);
    }

    #[test]
    fn arquivo_com_tipo_errado_e_recusado_pela_libjsonw() {
        let pasta = pasta_temporaria("tipo-errado");
        fs::create_dir_all(&pasta).unwrap();
        fs::write(pasta.join(ARQUIVO), "{provedor: 7}").unwrap();
        let erro = Configuracao::ler(&ambiente(&[("CHATBOT_DADOS", pasta.to_str().unwrap())]))
            .unwrap_err();
        assert!(erro.contains("configuração ilegível"), "{erro}");
        let _ = fs::remove_dir_all(&pasta);
    }

    #[test]
    fn arquivo_que_nao_abre_e_recusado() {
        let pasta = pasta_temporaria("pasta-no-lugar");
        fs::create_dir_all(pasta.join(ARQUIVO)).unwrap();
        let erro = Configuracao::ler(&ambiente(&[("CHATBOT_DADOS", pasta.to_str().unwrap())]))
            .unwrap_err();
        assert!(erro.contains("não consegui ler"), "{erro}");
        let _ = fs::remove_dir_all(&pasta);
    }

    #[test]
    fn salvar_escolha_grava_com_permissao_600() {
        use std::os::unix::fs::PermissionsExt;
        let pasta = pasta_temporaria("permissao");
        Configuracao::salvar_escolha(
            &pasta,
            Tipo::OpenRouter,
            "modelo-x",
            "chave-secreta-de-teste",
        )
        .unwrap();
        Configuracao::salvar_escolha(
            &pasta,
            Tipo::OpenRouter,
            "modelo-x",
            "chave-secreta-de-teste",
        )
        .unwrap();
        let modo = fs::metadata(pasta.join(ARQUIVO))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(modo, 0o600);
        let _ = fs::remove_dir_all(&pasta);
    }

    #[test]
    fn regravar_sobre_arquivo_com_permissao_aberta_volta_para_600() {
        use std::os::unix::fs::PermissionsExt;
        let pasta = pasta_temporaria("regravar");
        fs::create_dir_all(&pasta).unwrap();
        let antigo = pasta.join(ARQUIVO);
        fs::write(&antigo, "{provedor:\"openai\"}").unwrap();
        fs::set_permissions(&antigo, fs::Permissions::from_mode(0o644)).unwrap();
        Configuracao::salvar_escolha(&pasta, Tipo::Groq, "m", "chave-de-teste-456").unwrap();
        let modo = fs::metadata(&antigo).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo, 0o600);
        let texto = fs::read_to_string(&antigo).unwrap();
        assert!(texto.starts_with("// Configuração do chatbot"), "{texto}");
        assert!(texto.contains("chave:\"chave-de-teste-456\""), "{texto}");
        let _ = fs::remove_dir_all(&pasta);
    }

    #[test]
    fn salvar_em_lugar_impossivel_e_recusado() {
        let arquivo_no_caminho =
            std::env::temp_dir().join(format!("chatbot-cfg-arquivo-{}", std::process::id()));
        fs::write(&arquivo_no_caminho, "x").unwrap();
        let erro =
            Configuracao::salvar_escolha(&arquivo_no_caminho, Tipo::OpenAi, "m", "c").unwrap_err();
        assert!(erro.contains("não consegui criar a pasta"));
        let _ = fs::remove_file(&arquivo_no_caminho);
    }

    #[test]
    fn gravar_com_arquivo_que_e_pasta_mostra_o_erro() {
        let pasta = pasta_temporaria("gravar-pasta");
        fs::create_dir_all(pasta.join(ARQUIVO)).unwrap();
        let erro = Configuracao::salvar_escolha(&pasta, Tipo::OpenAi, "m", "c").unwrap_err();
        assert!(erro.contains("não consegui gravar"), "{erro}");
        let _ = fs::remove_dir_all(&pasta);
    }
}
