//! Migração do jeito antigo (TOML) para o jsonW-texto: o roteiro e a configuração. O original nunca se apaga (vai para
//! `<nome>.toml.antes`), a configuração nova sai com permissão 600 e o aviso nunca mostra a chave.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use chatbot_prospeccao::configuracao::{ARQUIVO, Configuracao};
use chatbot_prospeccao::migracao;
use chatbot_prospeccao::provedor::{Segredo, Tipo};
use chatbot_prospeccao::roteiro::Roteiro;
use chatbot_prospeccao::servidor;

const ROTEIRO_ANTIGO: &str = r#"# roteiro antigo, em TOML
[empresa]
nome = "Mercado Antigo"
o_que_vende = "Sistema de caixa para padarias"
para_quem = "Padarias de bairro"
tom = "direto"

[qualificacao]
perguntas = ["Quantas pessoas trabalham aqui?", "Quem decide a compra?"]

[humano]
a_partir_da_nota = 60
quando = ["o visitante pedir para falar com uma pessoa"]
"#;

const CHAVE_ANTIGA: &str = "chave-de-teste-789";

fn pasta(nome: &str) -> PathBuf {
    let caminho = std::env::temp_dir().join(format!("chatbot-mig-{nome}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&caminho);
    fs::create_dir_all(&caminho).unwrap();
    caminho
}

fn ambiente(dados: &std::path::Path) -> HashMap<String, String> {
    HashMap::from([("CHATBOT_DADOS".to_string(), dados.display().to_string())])
}

#[test]
fn roteiro_toml_antigo_vira_jsonw_e_o_original_fica_antes() {
    let raiz = pasta("roteiro");
    fs::write(raiz.join("roteiro.toml"), ROTEIRO_ANTIGO).unwrap();
    let (usado, avisos) = migracao::roteiro(&raiz.join("roteiro.jsw.txt")).unwrap();
    assert_eq!(usado, raiz.join("roteiro.jsw.txt"));
    assert_eq!(avisos.len(), 1, "{avisos:?}");
    assert!(avisos[0].contains("migrei"), "{avisos:?}");

    let novo = fs::read_to_string(raiz.join("roteiro.jsw.txt")).unwrap();
    assert!(novo.starts_with("// Convertido de roteiro.toml"), "{novo}");
    assert!(novo.contains("nome: \"Mercado Antigo\""), "{novo}");
    assert!(!novo.contains("\"nome\":"), "chave sem aspas: {novo}");
    let roteiro = Roteiro::ler(&novo).unwrap();
    assert_eq!(roteiro.empresa.tom, "direto");
    assert_eq!(roteiro.humano.a_partir_da_nota, 60);

    assert!(
        !raiz.join("roteiro.toml").exists(),
        "o TOML saiu do caminho"
    );
    let antes = fs::read_to_string(raiz.join("roteiro.toml.antes")).unwrap();
    assert_eq!(antes, ROTEIRO_ANTIGO, "o original fica intacto");
}

#[test]
fn chamar_de_novo_nao_migra_outra_vez() {
    let raiz = pasta("roteiro-duas-vezes");
    fs::write(raiz.join("roteiro.toml"), ROTEIRO_ANTIGO).unwrap();
    migracao::roteiro(&raiz.join("roteiro.jsw.txt")).unwrap();
    let (usado, avisos) = migracao::roteiro(&raiz.join("roteiro.jsw.txt")).unwrap();
    assert_eq!(usado, raiz.join("roteiro.jsw.txt"));
    assert_eq!(avisos, Vec::<String>::new());
    assert!(raiz.join("roteiro.toml.antes").exists());
}

#[test]
fn caminho_toml_explicito_troca_para_o_jsw_txt() {
    let raiz = pasta("roteiro-explicito");
    fs::write(raiz.join("outro.toml"), ROTEIRO_ANTIGO).unwrap();
    let (usado, avisos) = migracao::roteiro(&raiz.join("outro.toml")).unwrap();
    assert_eq!(usado, raiz.join("outro.jsw.txt"));
    assert_eq!(avisos.len(), 1);
    assert!(raiz.join("outro.jsw.txt").exists());
    assert!(raiz.join("outro.toml.antes").exists());
}

#[test]
fn toml_com_o_jsw_txt_ao_lado_fica_de_lado_e_avisa() {
    let raiz = pasta("roteiro-ao-lado");
    fs::write(raiz.join("roteiro.toml"), ROTEIRO_ANTIGO).unwrap();
    fs::write(raiz.join("roteiro.jsw.txt"), "{}").unwrap();
    let (usado, avisos) = migracao::roteiro(&raiz.join("roteiro.toml")).unwrap();
    assert_eq!(usado, raiz.join("roteiro.jsw.txt"));
    assert!(avisos[0].contains("ficou de lado"), "{avisos:?}");
    assert!(
        raiz.join("roteiro.toml").exists(),
        "nada apagado nem movido"
    );
}

#[test]
fn toml_que_nao_passa_na_validacao_nao_migra_e_diz_o_campo() {
    let raiz = pasta("roteiro-invalido");
    let ruim = ROTEIRO_ANTIGO.replace("nome = \"Mercado Antigo\"", "nome = \"  \"");
    fs::write(raiz.join("roteiro.toml"), ruim).unwrap();
    let erro = migracao::roteiro(&raiz.join("roteiro.jsw.txt")).unwrap_err();
    assert!(erro.contains("empresa.nome"), "{erro}");
    assert!(
        !raiz.join("roteiro.jsw.txt").exists(),
        "nada gravado pela metade"
    );
    assert!(
        raiz.join("roteiro.toml").exists(),
        "o original fica onde estava"
    );
}

#[test]
fn caminho_toml_que_nao_existe_e_nome_que_nao_e_roteiro_nao_migram() {
    let raiz = pasta("roteiro-sem-migrar");
    let sumido = raiz.join("sumido.toml");
    let (usado, avisos) = migracao::roteiro(&sumido).unwrap();
    assert_eq!(usado, sumido);
    assert_eq!(avisos, Vec::<String>::new());
    let outro = raiz.join("roteiro.txt");
    fs::write(&outro, ROTEIRO_ANTIGO).unwrap();
    let (usado, avisos) = migracao::roteiro(&outro).unwrap();
    assert_eq!(usado, outro);
    assert_eq!(avisos, Vec::<String>::new());
    assert!(outro.exists(), "nome que não é roteiro fica como está");
}

#[test]
fn antes_ocupado_recusa_a_troca_e_o_original_fica_no_lugar() {
    let raiz = pasta("roteiro-antes-ocupado");
    fs::write(raiz.join("roteiro.toml"), ROTEIRO_ANTIGO).unwrap();
    fs::create_dir_all(raiz.join("roteiro.toml.antes")).unwrap();
    let erro = migracao::roteiro(&raiz.join("roteiro.jsw.txt")).unwrap_err();
    assert!(erro.contains("não consegui guardar"), "{erro}");
    assert!(
        raiz.join("roteiro.toml").exists(),
        "o original não some numa troca que falhou"
    );
}

#[test]
fn configuracao_toml_que_nao_e_toml_e_recusada_sem_gravar_nada() {
    let dados = pasta("configuracao-ruim");
    fs::write(dados.join("configuracao.toml"), "provedor = [").unwrap();
    let erro = migracao::configuracao(&dados).unwrap_err();
    assert!(erro.contains("não é TOML válido"), "{erro}");
    assert!(!dados.join(ARQUIVO).exists(), "nada gravado pela metade");
    assert!(
        dados.join("configuracao.toml").exists(),
        "o original fica onde estava"
    );
}

#[test]
fn toml_quebrado_diz_o_arquivo() {
    let raiz = pasta("roteiro-quebrado");
    fs::write(raiz.join("roteiro.toml"), "empresa = [").unwrap();
    let erro = migracao::roteiro(&raiz.join("roteiro.jsw.txt")).unwrap_err();
    assert!(erro.contains("roteiro.toml"), "{erro}");
    assert!(erro.contains("não é TOML válido"), "{erro}");
}

#[test]
fn configuracao_toml_vira_jsonw_com_600_e_o_antigo_fica_antes() {
    let dados = pasta("configuracao");
    fs::write(
        dados.join("configuracao.toml"),
        format!("provedor = \"groq\"\nmodelo = \"llama-x\"\nchave = \"{CHAVE_ANTIGA}\"\n"),
    )
    .unwrap();
    let avisos = migracao::configuracao(&dados).unwrap();
    assert_eq!(avisos.len(), 1, "{avisos:?}");
    assert!(avisos[0].contains("migrei"), "{avisos:?}");
    assert!(
        !avisos[0].contains(CHAVE_ANTIGA),
        "o aviso nunca mostra a chave: {avisos:?}"
    );

    let novo = dados.join(ARQUIVO);
    let modo = fs::metadata(&novo).unwrap().permissions().mode() & 0o777;
    assert_eq!(modo, 0o600, "a configuração nova sai com 600");
    let texto = fs::read_to_string(&novo).unwrap();
    assert!(texto.contains("provedor:\"groq\""), "{texto}");
    assert!(
        texto.contains(&format!("chave:\"{CHAVE_ANTIGA}\"")),
        "{texto}"
    );

    assert!(!dados.join("configuracao.toml").exists());
    let antes = fs::read_to_string(dados.join("configuracao.toml.antes")).unwrap();
    assert!(antes.contains(CHAVE_ANTIGA), "o original fica intacto");

    let c = Configuracao::ler(&ambiente(&dados)).unwrap();
    assert_eq!(c.provedor, Tipo::Groq);
    assert_eq!(c.modelo, "llama-x");
    assert_eq!(c.chave.as_ref().map(Segredo::expor), Some(CHAVE_ANTIGA));
}

#[test]
fn configuracao_ja_migrada_ou_sem_antigo_nao_faz_nada() {
    let dados = pasta("configuracao-nada");
    assert_eq!(
        migracao::configuracao(&dados).unwrap(),
        Vec::<String>::new()
    );
    fs::write(dados.join("configuracao.toml"), "provedor = \"groq\"\n").unwrap();
    migracao::configuracao(&dados).unwrap();
    assert_eq!(
        migracao::configuracao(&dados).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn pasta_migra_roteiro_e_configuracao_de_uma_vez() {
    let raiz = pasta("projeto");
    fs::write(raiz.join("roteiro.toml"), ROTEIRO_ANTIGO).unwrap();
    fs::create_dir_all(raiz.join("dados")).unwrap();
    fs::write(
        raiz.join("dados").join("configuracao.toml"),
        format!("chave = \"{CHAVE_ANTIGA}\"\n"),
    )
    .unwrap();
    let avisos = migracao::pasta(&raiz).unwrap();
    assert_eq!(avisos.len(), 2, "{avisos:?}");
    assert!(raiz.join("roteiro.jsw.txt").exists());
    assert!(raiz.join("dados").join("configuracao.jsw.txt").exists());
    assert_eq!(
        migracao::pasta(&raiz).unwrap(),
        Vec::<String>::new(),
        "segunda vez não repete"
    );
}

#[test]
fn partida_com_roteiro_toml_explicito_usa_o_convertido() {
    let raiz = pasta("partida");
    let antigo = raiz.join("roteiro.toml");
    fs::write(&antigo, ROTEIRO_ANTIGO).unwrap();
    let configuracao = Configuracao {
        provedor: Tipo::OpenAi,
        modelo: "m".into(),
        chave: None,
        base: None,
        origens: Vec::new(),
        porta: 0,
        endereco: "127.0.0.1".into(),
        dados: raiz.clone(),
        roteiro: antigo,
    };
    let preparado = servidor::preparar(configuracao).unwrap();
    let todos = preparado.avisos.join("\n");
    assert!(todos.contains("migrei"), "{todos}");
    assert!(
        !todos.contains("roteiro de exemplo"),
        "o convertido é usado, não o exemplo: {todos}"
    );
    assert!(raiz.join("roteiro.jsw.txt").exists());
    preparado.servidor.unblock();
}
