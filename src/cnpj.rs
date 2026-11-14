//! O CNPJ: limpar o que a pessoa digitou e conferir os dígitos verificadores.
//! Vale para o CNPJ de números e para o alfanumérico (letras nas 12 primeiras posições).

/// Tira pontuação e espaços e põe as letras em maiúsculas.
pub fn limpar(texto: &str) -> String {
    texto.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

/// Confere tamanho, caracteres e os dois dígitos verificadores.
pub fn valido(cnpj: &str) -> bool {
    let b = cnpj.as_bytes();
    if b.len() != 14 {
        return false;
    }
    let corpo_ok = b[..12].iter().all(|c| c.is_ascii_digit() || c.is_ascii_uppercase());
    let dv_ok = b[12..].iter().all(|c| c.is_ascii_digit());
    if !corpo_ok || !dv_ok || b.iter().all(|c| *c == b[0]) {
        return false;
    }
    const PESOS: [u32; 13] = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    for n in [12usize, 13] {
        let soma: u32 = (0..n).map(|i| (b[i] as u32 - 48) * PESOS[13 - n + i]).sum();
        let resto = soma % 11;
        let esperado = if resto < 2 { 0 } else { 11 - resto };
        if b[n] as u32 - 48 != esperado {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn aceita_cnpj_real_com_e_sem_pontuacao() {
        assert!(valido(&limpar("37.853.407/0001-71")));
        assert!(valido("37853407000171"));
    }

    #[test]
    fn recusa_digito_errado_tamanho_errado_e_todos_iguais() {
        assert!(!valido("37853407000172"));
        assert!(!valido("123"));
        assert!(!valido("11111111111111"));
    }

    #[test]
    fn aceita_o_alfanumerico_do_exemplo_da_receita() {
        // exemplo público da Receita Federal para o CNPJ alfanumérico
        assert!(valido(&limpar("12.ABC.345/01DE-35")));
    }
}

/// Desfaz o %XX de um trecho de endereço (o navegador manda "/" como %2F).
pub fn sem_porcento(texto: &str) -> String {
    let b = texto.as_bytes();
    let mut saida = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&texto[i + 1..i + 3], 16) {
                saida.push(v);
                i += 3;
                continue;
            }
        }
        saida.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&saida).into_owned()
}

#[cfg(test)]
mod testes_endereco {
    use super::*;

    #[test]
    fn cnpj_com_barra_codificada_vale() {
        assert!(valido(&limpar(&sem_porcento("37.853.407%2F0001-71"))));
        assert_eq!(sem_porcento("abc%"), "abc%");
        assert_eq!(sem_porcento("%zz1"), "%zz1");
    }
}
