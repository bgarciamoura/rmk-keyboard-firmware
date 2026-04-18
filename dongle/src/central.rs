// F2 bring-up do dongle.
//
// Estratégia: usar a macro `#[rmk_keyboard]` que lê o `keyboard.toml` adjacente
// e expande para um main completo com USB + BLE + split central. Se a macro não
// cobrir split central em ESP32-S3 (risco listado em docs/F2-plan.md), caímos
// para expansão manual ao estilo do exemplo upstream `use_rust/esp32s3_ble`.
//
// Em F3 vamos precisar abrir esse arquivo para spawnar tasks Embassy próprias
// (display JD9853, touch AXS5106, WiFi NTP, UI). Nesse ponto trocamos a macro
// por expansão manual.

#![no_std]
#![no_main]

// `use esp_backtrace as _;` registra o #[panic_handler] via link — sem a
// referência, o crate não é incluído e o compilador reclama de falta de handler.
use esp_backtrace as _;

// rmk reexporta o crate de macros como `rmk::macros`; a macro em si se chama
// `rmk_keyboard`. Era `rmk::rmk_keyboard` em versões antigas.
use rmk::macros::rmk_keyboard;

#[rmk_keyboard]
mod keyboard {}
