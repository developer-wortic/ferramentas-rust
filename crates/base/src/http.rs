//! Servidor HTTP comum das ferramentas: o `tiny_http` com `TCP_NODELAY`.
//!
//! O `tiny_http` escreve o cabeçalho e o corpo em duas escritas. Com o algoritmo de Nagle ligado, a segunda escrita
//! espera o ACK da primeira, e o cliente atrasa esse ACK em até 40 ms: toda resposta maior que o buffer (a tela, o
//! widget, o QR) levava ~41 ms. Ligando `TCP_NODELAY` no socket que escuta, as conexões aceitas herdam a opção
//! (Linux e BSD) e a resposta sai na hora. Medido na bancada (`bancada/medir.sh`).

use std::error::Error;
use std::net::{TcpListener, ToSocketAddrs};

/// Igual a `tiny_http::Server::http`, com `TCP_NODELAY` nas conexões.
///
/// # Errors
/// O endereço não abre (porta ocupada, sem permissão, endereço inválido).
pub fn servidor<A: ToSocketAddrs>(
    endereco: A,
) -> Result<tiny_http::Server, Box<dyn Error + Send + Sync + 'static>> {
    let ouvinte = TcpListener::bind(endereco)?;
    sem_nagle(&ouvinte);
    tiny_http::Server::from_listener(ouvinte, None)
}

#[cfg(unix)]
fn sem_nagle(ouvinte: &TcpListener) {
    use std::ffi::{c_int, c_void};
    use std::os::fd::AsRawFd;
    unsafe extern "C" {
        fn setsockopt(
            fd: c_int,
            nivel: c_int,
            nome: c_int,
            valor: *const c_void,
            tamanho: u32,
        ) -> c_int;
    }
    const IPPROTO_TCP: c_int = 6;
    const TCP_NODELAY: c_int = 1;
    let um: c_int = 1;
    // SAFETY: descritor vivo do ouvinte; o valor aponta para um c_int do tamanho passado. Falhar só deixa o Nagle.
    unsafe {
        setsockopt(
            ouvinte.as_raw_fd(),
            IPPROTO_TCP,
            TCP_NODELAY,
            (&raw const um).cast(),
            u32::try_from(std::mem::size_of::<c_int>()).unwrap_or(4),
        );
    }
}

#[cfg(not(unix))]
fn sem_nagle(_ouvinte: &TcpListener) {}

#[cfg(test)]
mod testes {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Instant;

    #[test]
    fn resposta_grande_sai_sem_esperar_o_ack_atrasado() {
        let servidor = super::servidor("127.0.0.1:0").unwrap();
        let porta = servidor.server_addr().to_ip().unwrap().port();
        let corpo = "x".repeat(20_000);
        let fio = std::thread::spawn(move || {
            for _ in 0..20 {
                let pedido = servidor.recv().unwrap();
                pedido
                    .respond(tiny_http::Response::from_string(corpo.clone()))
                    .unwrap();
            }
        });
        let mut s = TcpStream::connect(("127.0.0.1", porta)).unwrap();
        s.set_nodelay(true).unwrap();
        let inicio = Instant::now();
        let mut buf = vec![0u8; 64 * 1024];
        for _ in 0..20 {
            s.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut resposta = Vec::new();
            loop {
                let n = s.read(&mut buf).unwrap();
                assert!(n > 0, "conexão fechada no meio");
                resposta.extend_from_slice(&buf[..n]);
                let cabeca = resposta.windows(4).position(|w| w == b"\r\n\r\n");
                if cabeca.is_some_and(|p| resposta.len() >= p + 4 + 20_000) {
                    break;
                }
            }
        }
        // com Nagle seriam 20 x ~40 ms = 800 ms
        assert!(inicio.elapsed().as_millis() < 400, "{:?}", inicio.elapsed());
        fio.join().unwrap();
    }
}
