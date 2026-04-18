// build.rs do dongle.
//
// Duas responsabilidades:
// 1. Gerar `config_generated.rs` em $OUT_DIR com `VIAL_KEYBOARD_DEF` (vial.json
//    comprimido em XZ) e `VIAL_KEYBOARD_ID` (identificador hardcoded de 8 bytes).
//    A macro #[rmk_keyboard] inclui esse arquivo via include!() para compor o
//    handshake do Vial.
// 2. Adicionar o linker script `linkall.x` que esp-hal espera.
//
// Adaptado de rmk/examples/use_rust/esp32s3_ble/build.rs.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::{env, fs};

use const_gen::*;
use xz2::read::XzEncoder;

fn main() {
    println!("cargo:rerun-if-changed=vial.json");
    println!("cargo:rerun-if-changed=keyboard.toml");
    generate_vial_config();

    // Linker script de esp-hal — disponível via crate após patch.
    println!("cargo:rustc-link-arg-bins=-Tlinkall.x");
}

fn generate_vial_config() {
    let out_file = Path::new(&env::var_os("OUT_DIR").unwrap()).join("config_generated.rs");

    let vial_json_path = Path::new("vial.json");
    let mut content = String::new();
    match File::open(vial_json_path) {
        Ok(mut file) => {
            file.read_to_string(&mut content).expect("Cannot read vial.json");
        }
        Err(e) => panic!("Cannot find vial.json at {vial_json_path:?}: {e}"),
    }

    let vial_cfg = json::stringify(json::parse(&content).expect("vial.json is not valid JSON"));
    let mut keyboard_def_compressed: Vec<u8> = Vec::new();
    XzEncoder::new(vial_cfg.as_bytes(), 6)
        .read_to_end(&mut keyboard_def_compressed)
        .expect("XZ compression of vial config failed");

    // 8-byte ID arbitrário — mesmo do exemplo upstream. O Vial host-side
    // usa isso como handshake para confirmar que está falando com o RMK certo.
    let keyboard_id: Vec<u8> = vec![0xB9, 0xBC, 0x09, 0xB2, 0x9D, 0x37, 0x4C, 0xEA];

    let const_declarations = [
        const_declaration!(pub VIAL_KEYBOARD_DEF = keyboard_def_compressed),
        const_declaration!(pub VIAL_KEYBOARD_ID = keyboard_id),
    ]
    .map(|s| "#[allow(clippy::redundant_static_lifetimes)]\n".to_owned() + s.as_str())
    .join("\n");

    fs::write(out_file, const_declarations).expect("Failed to write config_generated.rs");
}
