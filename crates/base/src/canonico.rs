//! jsonW-texto canônico em memória: o mesmo texto que a libjsonw escreve para uma linha de `.jsw` (`Leitor::texto`),
//! sem passar por arquivo. A libjsonw confere o registro contra o esquema ([`crate::texto::para_json`]) e devolve o
//! JSON normalizado (dinheiro `"1234.50 BRL"`, decimal na escala, tempo em microssegundos); daqui sai a forma texto
//! de cada tipo: `1234.50 BRL`, `10.50d`, `t"2026-11-16T10:00:00.000000Z"`, `u"123e4567-..."`, `3.5 km`.
//!
//! Esquema com tipo que este escritor não conhece (vetor, tensor, mídia, bytes, referência) cai para o caminho do
//! arquivo ([`crate::arquivo::texto`]), que é o da própria libjsonw. Os testes provam, tipo a tipo, que os dois
//! caminhos dão o mesmo texto, byte a byte.

use crate::texto;

/// O registro (JSON ou jsonW-texto) em jsonW-texto canônico, conferido contra o esquema.
///
/// # Errors
/// O registro não casa com o esquema, com a razão da libjsonw.
pub fn canonico(esquema: &str, registro: &str) -> Result<String, String> {
    let Some(tipo) = Tipo::do_esquema(esquema) else {
        return crate::arquivo::texto(esquema, [registro]).map(|t| t.trim_end().to_string());
    };
    let json = texto::para_json(esquema, registro)?;
    let mut leitor = Leitor {
        b: json.as_bytes(),
        s: &json,
        i: 0,
    };
    let mut saida = String::with_capacity(json.len());
    leitor.valor(&tipo, &mut saida)?;
    Ok(saida)
}

/// O tipo de um campo, só no que muda a forma texto.
#[derive(Debug, Clone, PartialEq)]
enum Tipo {
    /// Sai como no JSON (texto, número, bool, `qualquer`), com as chaves sem aspas.
    Bruto,
    /// Texto do JSON sem as aspas: dinheiro e medida.
    SemAspas,
    /// Número com o sufixo `d`.
    Decimal,
    /// Texto com o prefixo `t`.
    Tempo,
    /// Texto com o prefixo `u`.
    Uuid,
    Obj(Vec<(String, Tipo)>),
    Lista(Box<Tipo>),
    Mapa(Box<Tipo>),
}

impl Tipo {
    /// O esquema de um registro (`a:texto,b:dinheiro?,...`) como objeto. `None` se algum tipo não é conhecido aqui.
    fn do_esquema(esquema: &str) -> Option<Self> {
        let mut campos = Vec::new();
        for campo in partes(esquema) {
            let (nome, tipo) = campo.split_once(':')?;
            campos.push((nome.trim().to_string(), Self::do_tipo(tipo)?));
        }
        Some(Self::Obj(campos))
    }

    fn do_tipo(tipo: &str) -> Option<Self> {
        let mut t = tipo.trim();
        // marcas de permissão e de normalização (`+pessoal`, `+escopo(x)`, `+tenant`, `+nfc`) não mudam a forma
        if let Some(p) = posicao_fora(t, b'+') {
            t = &t[..p];
        }
        let t = t.trim_end_matches('?');
        if let Some(dentro) = t.strip_prefix("obj{").and_then(|r| r.strip_suffix('}')) {
            return Self::do_esquema(dentro);
        }
        if let Some(dentro) = t.strip_prefix("lista<").and_then(|r| r.strip_suffix('>')) {
            return Some(Self::Lista(Box::new(Self::do_tipo(dentro)?)));
        }
        if let Some(dentro) = t.strip_prefix("mapa<").and_then(|r| r.strip_suffix('>')) {
            let p = posicao_fora(dentro, b',')?;
            return Some(Self::Mapa(Box::new(Self::do_tipo(&dentro[p + 1..])?)));
        }
        let base = t.split('[').next().unwrap_or(t);
        match base {
            "dinheiro" | "medida" => Some(Self::SemAspas),
            "decimal" => Some(Self::Decimal),
            "tempo" => Some(Self::Tempo),
            "uuid" => Some(Self::Uuid),
            "codigo" | "texto" | "dic" | "url" | "bool" | "i64" | "i32" | "i16" | "i8" | "u8"
            | "f64" | "f32" | "f16" | "bf16" | "qualquer" => Some(Self::Bruto),
            _ => None,
        }
    }
}

/// As partes separadas por vírgula fora de `<>`, `{}`, `[]` e `()`.
fn partes(s: &str) -> Vec<&str> {
    let mut saida = Vec::new();
    let mut ini = 0;
    let mut fundo = 0i32;
    for (i, c) in s.bytes().enumerate() {
        match c {
            b'<' | b'{' | b'[' | b'(' => fundo += 1,
            b'>' | b'}' | b']' | b')' => fundo -= 1,
            b',' if fundo == 0 => {
                saida.push(&s[ini..i]);
                ini = i + 1;
            }
            _ => {}
        }
    }
    if ini < s.len() {
        saida.push(&s[ini..]);
    }
    saida
}

/// A primeira posição do byte fora de `<>`, `{}`, `[]` e `()`.
fn posicao_fora(s: &str, alvo: u8) -> Option<usize> {
    let mut fundo = 0i32;
    for (i, c) in s.bytes().enumerate() {
        match c {
            b'<' | b'{' | b'[' | b'(' => fundo += 1,
            b'>' | b'}' | b']' | b')' => fundo -= 1,
            c if c == alvo && fundo == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Percorre o JSON compacto que a libjsonw devolve.
struct Leitor<'a> {
    b: &'a [u8],
    s: &'a str,
    i: usize,
}

impl Leitor<'_> {
    fn erro(&self) -> String {
        format!("JSON inesperado da libjsonw na posição {}", self.i)
    }

    fn pular_espaco(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\n' | b'\r' | b'\t') {
            self.i += 1;
        }
    }

    fn espera(&mut self, c: u8) -> Result<(), String> {
        self.pular_espaco();
        if self.b.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            Err(self.erro())
        }
    }

    fn ve(&mut self) -> Option<u8> {
        self.pular_espaco();
        self.b.get(self.i).copied()
    }

    /// O texto entre aspas, com as aspas (o índice fica depois da aspa final).
    fn texto(&mut self) -> Result<&str, String> {
        self.pular_espaco();
        let ini = self.i;
        if self.b.get(ini) != Some(&b'"') {
            return Err(self.erro());
        }
        self.i += 1;
        while self.i < self.b.len() && self.b[self.i] != b'"' {
            if self.b[self.i] == b'\\' {
                self.i += 1;
            }
            self.i += 1;
        }
        self.i += 1;
        if self.i > self.b.len() {
            return Err(self.erro());
        }
        Ok(&self.s[ini..self.i])
    }

    /// Um valor qualquer, como está (o índice fica depois dele).
    fn bruto(&mut self) -> Result<&str, String> {
        self.pular_espaco();
        let ini = self.i;
        match self.b.get(self.i) {
            Some(b'"') => {
                self.texto()?;
            }
            Some(b'{' | b'[') => {
                let mut fundo = 0i32;
                while self.i < self.b.len() {
                    match self.b[self.i] {
                        b'"' => {
                            self.texto()?;
                            continue;
                        }
                        b'{' | b'[' => fundo += 1,
                        b'}' | b']' => {
                            fundo -= 1;
                            if fundo == 0 {
                                self.i += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    self.i += 1;
                }
            }
            Some(_) => {
                while self.i < self.b.len() && !matches!(self.b[self.i], b',' | b'}' | b']') {
                    self.i += 1;
                }
            }
            None => return Err(self.erro()),
        }
        Ok(self.s[ini..self.i].trim_end())
    }

    fn chave(&mut self, saida: &mut String) -> Result<String, String> {
        let com_aspas = self.texto()?;
        let dentro = &com_aspas[1..com_aspas.len() - 1];
        // mesma regra do de_json: chave que é identificador perde as aspas
        saida.push_str(if texto::identificador(dentro) {
            dentro
        } else {
            com_aspas
        });
        Ok(dentro.to_string())
    }

    fn valor(&mut self, tipo: &Tipo, saida: &mut String) -> Result<(), String> {
        if self.ve() == Some(b'n') {
            saida.push_str(self.bruto()?);
            return Ok(());
        }
        match tipo {
            Tipo::Bruto => saida.push_str(&texto::de_json(self.bruto()?)),
            Tipo::SemAspas => {
                let t = self.texto()?;
                saida.push_str(&t[1..t.len() - 1]);
            }
            Tipo::Decimal => {
                saida.push_str(self.bruto()?);
                saida.push('d');
            }
            Tipo::Tempo => {
                saida.push('t');
                saida.push_str(self.texto()?);
            }
            Tipo::Uuid => {
                saida.push('u');
                saida.push_str(self.texto()?);
            }
            Tipo::Obj(campos) => {
                self.espera(b'{')?;
                saida.push('{');
                let mut primeiro = true;
                while self.ve() != Some(b'}') {
                    if !primeiro {
                        self.espera(b',')?;
                        saida.push(',');
                    }
                    primeiro = false;
                    let nome = self.chave(saida)?;
                    self.espera(b':')?;
                    saida.push(':');
                    let t = campos
                        .iter()
                        .find(|(n, _)| *n == nome)
                        .map_or(&Tipo::Bruto, |(_, t)| t);
                    self.valor(t, saida)?;
                }
                self.espera(b'}')?;
                saida.push('}');
            }
            Tipo::Lista(t) => {
                self.espera(b'[')?;
                saida.push('[');
                let mut primeiro = true;
                while self.ve() != Some(b']') {
                    if !primeiro {
                        self.espera(b',')?;
                        saida.push(',');
                    }
                    primeiro = false;
                    self.valor(t, saida)?;
                }
                self.espera(b']')?;
                saida.push(']');
            }
            Tipo::Mapa(t) => {
                self.espera(b'{')?;
                saida.push('{');
                let mut primeiro = true;
                while self.ve() != Some(b'}') {
                    if !primeiro {
                        self.espera(b',')?;
                        saida.push(',');
                    }
                    primeiro = false;
                    self.chave(saida)?;
                    self.espera(b':')?;
                    saida.push(':');
                    self.valor(t, saida)?;
                }
                self.espera(b'}')?;
                saida.push('}');
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// O mesmo texto que a libjsonw escreve para o registro gravado num `.jsw`.
    fn pela_libjsonw(esquema: &str, registro: &str) -> String {
        crate::arquivo::texto(esquema, [registro])
            .unwrap()
            .trim_end()
            .to_string()
    }

    fn igual(esquema: &str, registro: &str) {
        assert_eq!(
            canonico(esquema, registro).unwrap(),
            pela_libjsonw(esquema, registro),
            "{esquema}"
        );
    }

    #[test]
    fn todos_os_tipos_saem_iguais_aos_da_libjsonw() {
        igual(
            "a:codigo,b:texto?,c:dinheiro?,d:decimal[2]?,e:decimal[4]?,t:tempo?,i:i64?,f:f64?,g:f32?,u:url?,id:uuid?,\
             m:medida?,k:medida[kg]?,x:dic?,l:lista<texto>?,o:obj{p:dinheiro,q:texto}?,z:qualquer?,bo:bool?,n:i64?",
            "{a:\"x\\\"y\",b:\"çã 😀\\n\",c:1.234,5 BRL,d:10,5d,e:3d,t:t\"2026-11-16T10:00:00Z\",i:-7,f:12,g:1.5,\
             u:\"https://a.b/c?d=1\",id:\"123e4567-e89b-12d3-a456-426614174000\",m:3,5 km,k:2500 g,x:\"dic\",l:[\"a\"],\
             o:{p:2 USD,q:\"w\"},z:{k:[1,2.5,\"s\"]},bo:true}",
        );
    }

    #[test]
    fn listas_e_mapas_tipados_e_nulos() {
        igual(
            "v:lista<dinheiro>,w:mapa<texto,dinheiro>?,y:lista<obj{t:tempo}>?",
            "{v:[1 BRL,2,5 BRL],w:{a:3 BRL}}",
        );
        igual(
            "v:lista<dinheiro>,w:mapa<texto,qualquer>?,y:lista<obj{t:tempo}>?",
            "{v:[],w:{PORTA:8081,\"com espaço\":\"x\"},y:[{t:t\"2026-01-01T00:00:00Z\"}]}",
        );
        igual("a:texto?,b:dinheiro?,c:tempo?,d:decimal[2]?", "{}");
    }

    #[test]
    fn marcas_de_permissao_nao_mudam_a_forma() {
        igual(
            "email:texto+pessoal,valor:dinheiro+escopo(financeiro),tenant:i64+tenant",
            "{email:\"a@b.c\",valor:9,90 BRL,tenant:3}",
        );
    }

    #[test]
    fn registro_em_json_tambem_vale() {
        igual(
            "nome:texto,valor:dinheiro",
            r#"{"nome":"Ana","valor":"12.50 BRL"}"#,
        );
    }

    #[test]
    fn tipo_desconhecido_cai_para_o_arquivo() {
        igual("v:vf32[3],n:texto", "{v:[1,2,3],n:\"x\"}");
    }

    #[test]
    fn fora_do_esquema_da_a_razao() {
        assert!(canonico("a:i64", "{a:\"x\"}").is_err());
    }
}
