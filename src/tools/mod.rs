//! Apps do próprio KalangOS que não são parte do shell.
//!
//! - [`expr`]: avaliador de expressões (reais e inteiras) da Calculadora.
//! - [`codec`]: ferramentas de programador (Base64, URL, hashes, JSON...).
//! - [`calculator`]: ligação da Calculadora (`--calculadora`) com a UI.

pub mod calculator;
pub mod codec;
pub mod expr;
