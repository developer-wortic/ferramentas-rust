//! A ABI C pública do jsonW (`terceiros/jsonw/sdk/jsonw.h`), com `Leitor` e `Escritor` seguros por cima.
//! A libjsonw entra sem modificação; daqui só sai o que as ferramentas usam.

use std::ffi::{CStr, CString, c_char, c_int, c_void};

unsafe extern "C" {
    fn jsonw_versao() -> *const c_char;
    fn jsonw_abrir(caminho: *const c_char, erro: *mut c_char, nerro: usize) -> *mut c_void;
    fn jsonw_abrir_memoria(
        bytes: *const c_void,
        n: usize,
        erro: *mut c_char,
        nerro: usize,
    ) -> *mut c_void;
    fn jsonw_fechar(a: *mut c_void);
    fn jsonw_linhas(a: *const c_void) -> u64;
    fn jsonw_linha_json(a: *mut c_void, linha: u64, saida: *mut c_char, cap: usize) -> i64;
    fn jsonw_linha_texto(a: *mut c_void, linha: u64, saida: *mut c_char, cap: usize) -> i64;
    fn jsonw_conferir(a: *mut c_void, erro: *mut c_char, nerro: usize) -> c_int;
    fn jsonw_criar(
        caminho: *const c_char,
        esquema: *const c_char,
        erro: *mut c_char,
        nerro: usize,
    ) -> *mut c_void;
    fn jsonw_opcoes(w: *mut c_void, nivel: c_int, hash: c_int, bloco_kb: c_int) -> c_int;
    fn jsonw_gravar_json(
        w: *mut c_void,
        json: *const c_char,
        n: usize,
        erro: *mut c_char,
        nerro: usize,
    ) -> c_int;
    fn jsonw_fechar_escritor(w: *mut c_void) -> c_int;
    fn jsonw_abandonar(w: *mut c_void) -> c_int;
}

const TAMANHO_ERRO: usize = 320;

fn cstring(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|_| "texto com byte nulo".to_string())
}

/// Chama com um buffer de erro e devolve o retorno e o texto do erro.
fn com_erro<T>(chamada: impl FnOnce(*mut c_char, usize) -> T) -> (T, String) {
    let mut erro = [0u8; TAMANHO_ERRO];
    let r = chamada(erro.as_mut_ptr().cast(), TAMANHO_ERRO);
    let n = erro.iter().position(|&b| b == 0).unwrap_or(erro.len());
    (r, String::from_utf8_lossy(&erro[..n]).into_owned())
}

/// Buffer que cresce até caber (-2 = curto na ABI).
fn texto_crescente(mut chamada: impl FnMut(*mut c_char, usize) -> i64) -> Result<String, String> {
    let mut cap = 1usize << 14;
    loop {
        let mut b = vec![0u8; cap];
        match chamada(b.as_mut_ptr().cast(), cap) {
            -2 => cap *= 4,
            -3 => return Err("linha oculta pela visão".into()),
            ..0 => return Err("linha ilegível".into()),
            n => {
                b.truncate(usize::try_from(n).unwrap_or(0));
                return String::from_utf8(b).map_err(|_| "linha fora de UTF-8".into());
            }
        }
    }
}

/// Versão da libjsonw.
#[must_use]
pub fn versao() -> String {
    // SAFETY: a ABI devolve um texto estático terminado em 0.
    let p = unsafe { jsonw_versao() };
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// Um `.jsw` aberto para leitura (arquivo em `mmap` ou bytes copiados).
pub struct Leitor {
    a: *mut c_void,
    /// Quantas linhas o arquivo tem.
    pub linhas: u64,
}

impl Leitor {
    /// # Errors
    /// O arquivo não existe, não é jsonW ou está corrompido.
    pub fn abrir(caminho: &str) -> Result<Self, String> {
        let c = cstring(caminho)?;
        // SAFETY: caminho terminado em 0 e buffer de erro do tamanho passado.
        let (a, erro) = com_erro(|e, n| unsafe { jsonw_abrir(c.as_ptr(), e, n) });
        Self::de(a, erro)
    }

    /// # Errors
    /// Os bytes não são um `.jsw` íntegro.
    pub fn de_bytes(bytes: &[u8]) -> Result<Self, String> {
        // SAFETY: a libjsonw copia os bytes; o slice vive durante a chamada.
        let (a, erro) = com_erro(|e, n| unsafe {
            jsonw_abrir_memoria(bytes.as_ptr().cast(), bytes.len(), e, n)
        });
        Self::de(a, erro)
    }

    fn de(a: *mut c_void, erro: String) -> Result<Self, String> {
        if a.is_null() {
            return Err(if erro.is_empty() {
                "não é jsonW".into()
            } else {
                erro
            });
        }
        // SAFETY: handle válido devolvido pela ABI.
        let linhas = unsafe { jsonw_linhas(a) };
        Ok(Self { a, linhas })
    }

    /// A linha `i` em JSON.
    ///
    /// # Errors
    /// Linha fora do arquivo ou ilegível.
    pub fn json(&self, i: u64) -> Result<String, String> {
        // SAFETY: handle vivo; buffer do tamanho passado.
        texto_crescente(|b, cap| unsafe { jsonw_linha_json(self.a, i, b, cap) })
    }

    /// A linha `i` em jsonW-texto canônico.
    ///
    /// # Errors
    /// Linha fora do arquivo ou ilegível.
    pub fn texto(&self, i: u64) -> Result<String, String> {
        // SAFETY: handle vivo; buffer do tamanho passado.
        texto_crescente(|b, cap| unsafe { jsonw_linha_texto(self.a, i, b, cap) })
    }

    /// Confere a custódia inteira (crc32c, hash de cada bloco e a cadeia).
    ///
    /// # Errors
    /// O arquivo foi alterado ou está cortado.
    pub fn conferir(&self) -> Result<(), String> {
        // SAFETY: handle vivo; buffer de erro do tamanho passado.
        let (r, erro) = com_erro(|e, n| unsafe { jsonw_conferir(self.a, e, n) });
        if r == 0 { Ok(()) } else { Err(erro) }
    }
}

impl Drop for Leitor {
    fn drop(&mut self) {
        // SAFETY: handle devolvido por jsonw_abrir*, fechado uma vez só.
        unsafe { jsonw_fechar(self.a) };
    }
}

/// Um `.jsw` em gravação. Só [`Escritor::fechar`] publica (troca atômica); largar sem fechar desiste.
pub struct Escritor {
    w: *mut c_void,
}

impl Escritor {
    /// # Errors
    /// Esquema inválido ou pasta sem permissão.
    pub fn criar(caminho: &str, esquema: &str) -> Result<Self, String> {
        let c = cstring(caminho)?;
        let e = cstring(esquema)?;
        // SAFETY: textos terminados em 0 e buffer de erro do tamanho passado.
        let (w, erro) = com_erro(|b, n| unsafe { jsonw_criar(c.as_ptr(), e.as_ptr(), b, n) });
        if w.is_null() {
            return Err(if erro.is_empty() {
                "não consegui criar o arquivo".into()
            } else {
                erro
            });
        }
        Ok(Self { w })
    }

    /// Compressão 0..2, hash 0 (SHA-256) ou 1 (BLAKE3), bloco em KiB (0 = padrão).
    pub fn opcoes(&mut self, compressao: i32, hash: i32, bloco_kb: i32) {
        // SAFETY: handle vivo.
        unsafe { jsonw_opcoes(self.w, compressao, hash, bloco_kb) };
    }

    /// Uma linha em JSON ou jsonW-texto.
    ///
    /// # Errors
    /// A linha não casa com o esquema, com a razão.
    pub fn gravar(&mut self, linha: &str) -> Result<(), String> {
        // SAFETY: handle vivo; a linha vive durante a chamada.
        let (r, erro) = com_erro(|e, n| unsafe {
            jsonw_gravar_json(self.w, linha.as_ptr().cast(), linha.len(), e, n)
        });
        if r == 0 { Ok(()) } else { Err(erro) }
    }

    /// Fecha e publica o arquivo no lugar.
    ///
    /// # Errors
    /// Erro de disco na publicação.
    pub fn fechar(mut self) -> Result<(), String> {
        let w = std::mem::replace(&mut self.w, std::ptr::null_mut());
        // SAFETY: handle vivo, fechado uma vez só (o Drop vê nulo).
        if unsafe { jsonw_fechar_escritor(w) } == 0 {
            Ok(())
        } else {
            Err("não consegui publicar o arquivo".into())
        }
    }
}

impl Drop for Escritor {
    fn drop(&mut self) {
        if !self.w.is_null() {
            // SAFETY: handle vivo que não foi publicado: desiste sem tocar no destino.
            unsafe { jsonw_abandonar(self.w) };
        }
    }
}
