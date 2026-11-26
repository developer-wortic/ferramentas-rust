//! Pix estático em Rust: monta, lê e confere o BR Code (EMV QRCPS-MPM, padrão do Banco Central),
//! valida as chaves Pix e gera o QR Code em SVG.
//!
//! Sem rede e sem banco. Cobrança dinâmica com vencimento, confirmação automática de pagamento,
//! conciliação e checkout ficam fora desta biblioteca.

pub mod chave;
pub mod cli;
pub mod cobranca;
pub mod crc;
pub mod erro;
pub mod jsonw;
pub mod leitura;
pub mod lote;
pub mod qr;
pub mod registro;
pub mod servidor;
pub mod texto;
pub mod tlv;
pub mod valor;
pub mod web;

pub use cobranca::Cobranca;
pub use erro::Erro;
pub use leitura::Leitura;
