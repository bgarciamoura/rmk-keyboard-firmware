//! Biblioteca compartilhada entre os bins do dongle.
//!
//! Expõe módulos reutilizáveis (hoje: `drivers::jd9853`) consumidos por
//! `central.rs` (firmware de produção) e pelos bins de diagnóstico em
//! `src/bin/*.rs`.

#![no_std]

pub mod drivers;
