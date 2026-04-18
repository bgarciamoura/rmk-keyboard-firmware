# F2 — Dongle como projeto Rust (plano detalhado)

Pré-condição: F1 verde no CI (`dongle.bin` sendo produzido pelo `user_build.yml` upstream, ESP32-S3 enumerando como HID).

Objetivo do F2: **substituir** o pipeline config-only do dongle por um projeto Rust próprio, usando RMK como library. Isso habilita F3+ (tasks Embassy paralelas para display JD9853, touch AXS5106, WiFi NTP, UI).

## Decisões já tomadas

| Decisão | Escolha | Rationale |
|--|--|--|
| Versão do RMK | `rmk = "0.7"` crates.io | Release estável. Se faltar feature ESP32-S3 split central, pinar em commit git de `haobogu/rmk`. |
| Config do keymap | Manter `keyboard.toml` + macro `#[rmk_keyboard]` | Preserva os 6 layers traduzidos do `.vil`. Vial continua editável em runtime. |
| HAL | `esp-hal` (bare-metal) — **não** `esp-idf-hal` | RMK no ESP32-S3 é construído sobre `esp-hal`. Usar `esp-idf-hal` exigiria fork do RMK. |
| Toolchain | esp-rs channel `esp` (Xtensa nightly) | Target `xtensa-esp32s3-none-elf`. Instalado via GH Action `esp-rs/xtensa-toolchain`. |
| UI/gráficos (F3+) | `embedded-graphics` + `tinygif` | Leve, Rust-native, sem FreeRTOS. Drivers C do mimiclaw servem de referência de init. |
| Fonte de hora (F5) | NTP via Wi-Fi (`esp-wifi` + `sntpc`) | ESP32-S3 tem Wi-Fi; evita app companion no host. |

## Arquivos a criar

### `dongle/Cargo.toml`

```toml
[package]
name = "rmk-dongle"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"

[dependencies]
rmk = { version = "0.7", features = ["esp32s3_ble", "split", "storage", "vial"] }

# ESP-rs bare-metal stack
esp-hal = { version = "0.22", features = ["esp32s3", "unstable"] }
esp-radio = { version = "0.1", features = ["esp32s3", "ble"] }
esp-alloc = "0.5"
esp-backtrace = { version = "0.14", features = ["esp32s3", "panic-handler", "exception-handler", "println"] }
esp-println = { version = "0.12", features = ["esp32s3", "log"] }

# Embassy
embassy-executor = { version = "0.6", features = ["arch-xtensa", "executor-thread", "integrated-timers", "nightly"] }
embassy-time = { version = "0.3", features = ["generic-queue-8"] }
embassy-sync = "0.6"

# Bluetooth HCI
bt-hci = "0.1"
trouble-host = { version = "0.1", features = ["derive"] }

# Logging
log = "0.4"

[profile.dev]
opt-level = "s"
debug = 2

[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
debug = 2  # útil para panic backtrace

# Verificar na primeira compilação:
# - Se o RMK 0.7 fixa esp-hal em versão diferente, alinhar aqui.
# - Se faltar alguma feature de esp-radio, ver examples/use_rust/esp32s3_ble no rmk upstream.
```

### `dongle/src/main.rs`

Versão minimalista via macro (F2 foca em só rodar o RMK; tasks customizadas entram em F3+):

```rust
#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use rmk::rmk_keyboard;

// A macro lê o ../keyboard.toml do mesmo diretório de Cargo.toml em tempo de
// compilação, gera boilerplate de split central, USB HID, BLE peripheral (host-side),
// e o Embassy executor. Nenhuma configuração adicional é necessária em F2.
#[rmk_keyboard]
mod keyboard {}
```

Em F3 vamos precisar expandir a macro manualmente (padrão `use_rust`) para podermos spawnar nossas próprias tasks Embassy ao lado do RMK. Há dois caminhos possíveis em F3:

1. **Macro + task hook**: se RMK expuser um hook tipo `#[rmk_keyboard(extra_tasks = [display_task, touch_task])]`, usar esse caminho.
2. **Expansão manual**: copiar a expansão gerada pela macro como ponto de partida e adaptar para spawn de múltiplas tasks. Mais trabalho, mais controle.

A decisão fica para F3 depois de inspecionar o código gerado.

### `dongle/.cargo/config.toml`

```toml
[build]
target = "xtensa-esp32s3-none-elf"

[target.xtensa-esp32s3-none-elf]
runner = "espflash flash --monitor"
rustflags = [
  "-C", "force-frame-pointers",
  "-C", "link-arg=-Tlinkall.x",
  "-C", "link-arg=-Trom_functions.x",
]

[env]
ESP_LOG = "info"

[unstable]
build-std = ["alloc", "core"]
```

### `dongle/rust-toolchain.toml`

```toml
[toolchain]
channel = "esp"
components = ["rust-src"]
```

(O canal `esp` é instalado pelo `espup install`; a GH Action `esp-rs/xtensa-toolchain` faz isso automaticamente.)

### `dongle/partitions.csv`

Layout reservando espaço para dados do display (GIFs, fonts) a partir de F5:

```csv
# Name,   Type, SubType, Offset,  Size, Flags
nvs,      data, nvs,     0x9000,  0x6000,
phy_init, data, phy,     0xf000,  0x1000,
factory,  app,  factory, 0x10000, 2M,
storage,  data, spiffs,  ,        12M,
```

ESP32-S3 padrão tem 8 MB de flash; se a placa tiver 16 MB (como mimiclaw), aumentar `storage` para ~13 MB. Verificar primeiro no datasheet da placa.

### `dongle/build.rs` (opcional em F2, mas útil)

Se precisarmos de linker script custom ou embedar recursos depois:

```rust
fn main() {
    println!("cargo:rerun-if-changed=keyboard.toml");
    println!("cargo:rerun-if-changed=vial.json");
}
```

## Refator do CI

Substituir `.github/workflows/build.yml`:

```yaml
name: Build RMK firmware
on:
  workflow_dispatch:
  push:
    paths: ["**"]

jobs:
  # Peripherals continuam no pipeline config-only do upstream. Desabilitado
  # até o wiring físico chegar. Para reativar: remover o `if: false`.
  build-peripherals:
    if: false
    strategy:
      fail-fast: false
      matrix:
        include:
          - role: left
            toml: left/keyboard.toml
          - role: right
            toml: right/keyboard.toml
    name: build-${{ matrix.role }}
    uses: haobogu/rmk/.github/workflows/user_build.yml@main
    with:
      keyboard_toml_path: ${{ matrix.toml }}
      vial_json_path: dongle/vial.json

  build-dongle:
    name: build-dongle
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: dongle
    steps:
      - uses: actions/checkout@v4

      - name: Cache cargo registry & target
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: dongle -> target
          shared-key: esp32s3
          cache-on-failure: true

      - name: Install esp Xtensa toolchain
        uses: esp-rs/xtensa-toolchain@v1.5
        with:
          ldproxy: true
          default: true
          buildtargets: esp32s3

      - name: Install espflash
        run: cargo install espflash --locked --version ^3

      - name: Build firmware (release)
        run: cargo build --release

      - name: Produce flashable image
        run: |
          espflash save-image \
            --chip esp32s3 \
            --merge \
            --partition-table partitions.csv \
            target/xtensa-esp32s3-none-elf/release/rmk-dongle \
            dongle.bin

      - name: Upload dongle.bin
        uses: actions/upload-artifact@v4
        with:
          name: dongle
          path: dongle/dongle.bin
```

## Ordem de execução (quando F1 estiver verde)

1. Gerar `dongle/Cargo.toml`, `dongle/rust-toolchain.toml`, `dongle/.cargo/config.toml`, `dongle/partitions.csv`, `dongle/src/main.rs`, `dongle/build.rs`.
2. **Localmente** (se o usuário quiser validar antes do CI): instalar esp-rs toolchain (`espup install`), rodar `cargo check` dentro de `dongle/`. Diagnostica problemas de deps rápido.
3. Reescrever `.github/workflows/build.yml` com a versão acima.
4. Commit + push → esperar `build-dongle` passar.
5. Validar que o `dongle.bin` novo:
   - Flasha sem erro (`espflash flash dongle.bin`).
   - ESP32-S3 enumera como HID Keyboard + Mouse (mesmo comportamento do F1).
   - Não há regressão funcional.
6. Marcar F2 como concluído em `TODO.md`.

## Riscos e mitigação

| Risco | Probabilidade | Mitigação |
|--|--|--|
| `rmk 0.7` crates.io não tem ESP32-S3 split central | Média | Trocar para `rmk = { git = "https://github.com/haobogu/rmk", rev = "<commit>" }`. Ver exemplo `examples/use_config/esp32_ble_split` no upstream. |
| Versões de `esp-hal`/`esp-radio`/`embassy-*` listadas aqui estão desatualizadas | Alta | Copiar as versões exatas do `Cargo.toml` do exemplo `use_rust/esp32s3_ble` no upstream no momento da execução. |
| Macro `#[rmk_keyboard]` não suporta split central em ESP32-S3 | Média | Cair para expansão manual: copiar `src/main.rs` do exemplo `use_rust/esp32s3_ble` + adicionar `run_peripheral_manager` para peripheral 0 e 1. |
| GH Action `esp-rs/xtensa-toolchain` demora ~5 min em cold cache | Baixa | `Swatinem/rust-cache` cobre. Primeiro build sempre vai ser lento. |
| `espflash save-image` não aceita `--merge` com `--partition-table` na versão estável | Baixa | Remover `--merge` ou gerar sem partition table no início; ajustar quando tiver SPIFFS em F5. |
| Conflito de memória: RMK + BLE stack + heap do Embassy | Média | Usar `esp-alloc` com heap explícito em PSRAM (`heap_caps_malloc(..., MALLOC_CAP_SPIRAM)` equivalente Rust). Só vira problema em F3+. |

## Verificação de sucesso (definition of done do F2)

- [ ] `cargo build --release` local passa sem erro
- [ ] Job `build-dongle` passa no GH Actions
- [ ] `dongle.bin` produzido tem tamanho razoável (~500 KB – 1.5 MB)
- [ ] Flash no ESP32-S3 concluído sem erro
- [ ] Host reconhece como HID Keyboard + HID Mouse
- [ ] Log serial via `espflash monitor` mostra RMK inicializando e rodando
- [ ] `TODO.md` F2 marcado como `[x]` e F3 desbloqueado

## Rollback

Se F2 causar regressão, reverter é trivial: `git revert` do commit que introduziu Cargo.toml/main.rs/.cargo/, reativar a matrix config-only no `build.yml`. A TOML do dongle (`dongle/keyboard.toml`) volta a ser a fonte da verdade.

## Referências upstream (consultar no momento da execução)

- Exemplo ESP32-S3 puro Rust: https://github.com/HaoboGu/rmk/tree/main/examples/use_rust/esp32s3_ble
- Exemplo ESP32 split config: https://github.com/HaoboGu/rmk/tree/main/examples/use_config/esp32_ble_split
- Macro `#[rmk_keyboard]`: https://docs.rs/rmk-macro/latest/rmk_macro/
- Enum `KeyCode`: https://docs.rs/rmk/latest/rmk/keycode/enum.KeyCode.html
- esp-rs toolchain: https://docs.esp-rs.org/book/installation/index.html
