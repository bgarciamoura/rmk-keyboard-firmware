# TODO — Charybdis 3x6 Wireless (RMK)

Lista ordenada por **fase**. F1 está em execução; F2+ são projetos separados.

---

## F1 — Build do dongle verde no CI (em andamento)

Objetivo: `git push` produzir um `dongle.bin` flasheável sem erros, via upstream `user_build.yml`.

- [x] `.github/workflows/build.yml` buildando só o dongle (left/right comentados)
- [x] `split.central` com `rows=1, cols=1` defensivo (fallback se RMK rejeitar 0x0)
- [x] 6 layers traduzidos do `.vil` no `dongle/keyboard.toml`
- [x] Behavior: home-row mods, layer-taps, macros M0/M1, perfil HRM
- [ ] **Commit + push + ver o job passando no GitHub Actions**
- [ ] Baixar o `dongle.bin` do artifact e fazer flash no ESP32-S3 com `espflash flash` ou `esptool.py`
- [ ] Confirmar enumeração USB: o host deve reconhecer como HID Keyboard + HID Mouse
- [ ] **Se falhar no build**: ler o log, consertar (provável candidato: nome de keycode rejeitado, ou split.central 1x1 também não aceito → usar `[split.central.matrix]` dummy com pinos não-usados)
- [ ] Validar nomes de keycode RMK: `MouseBtn1..5`, `MouseWheelUp/Down`, `VolumeUp/Down`, `MediaPlayPause`, `MediaNextTrack`, `MediaPrevTrack`, `BrightnessUp/Down`, `Mute`, `KpPlus`, `Macro0/1`. Consultar https://docs.rs/rmk/latest/rmk/keycode/enum.KeyCode.html

---

## F2 — Dongle vira projeto Rust (habilita display)

**Plano detalhado**: [`docs/F2-plan.md`](docs/F2-plan.md) contém o conteúdo completo de cada arquivo, ordem de execução, riscos e definition of done.

Resumo:

- [ ] Gerar `dongle/Cargo.toml`, `dongle/src/main.rs`, `dongle/.cargo/config.toml`, `dongle/rust-toolchain.toml`, `dongle/partitions.csv`, `dongle/build.rs`
- [ ] Reescrever `.github/workflows/build.yml` com job `build-dongle` custom (espup + cargo + espflash save-image)
- [ ] Peripherals continuam com `if: false` até P-pendentes de wiring resolverem
- [ ] Validar que o `dongle.bin` novo enumera igual ao F1 (sem regressão)

---

## F3 — Display driver (JD9853 via SPI)

Objetivo: tela acende e imprime "Hello RMK". Reaproveitar sequências de init de `C:\Users\bgarciamoura\projects\mimiclaw\components\esp_lcd_jd9853\esp_lcd_jd9853.c` (460 LoC C) como referência de port.

- [ ] Identificar pinos SPI do display na placa (SCK, MOSI, DC, CS, RST, BL) — consultar datasheet/schematic
- [ ] Novo módulo `dongle/src/drivers/jd9853.rs` implementando driver em Rust bare-metal (embedded-hal async)
- [ ] Task Embassy `display_task` que inicializa driver e faz um fill com cor fixa
- [ ] Integrar `embedded-graphics` para desenhar texto
- [ ] Validar: tela mostra "Hello RMK" na primeira boot

---

## F4 — Touch driver (AXS5106 via I2C)

Objetivo: coordenadas de toque entrando no runtime. Referência: `mimiclaw/components/esp_lcd_touch_axs5106/esp_lcd_touch_axs5106.c` (260 LoC).

- [ ] Identificar pinos I2C do touch (SDA, SCL, INT, RST)
- [ ] Novo módulo `dongle/src/drivers/axs5106.rs` (embedded-hal async I2C)
- [ ] Task `touch_task` que faz polling/interrupt e envia eventos via channel para `ui_task`
- [ ] Log de toques no serial para validação

---

## F5 — UI + WiFi + GIFs + relógio

Objetivo: dashboard completo. Se dividir se ficar pesado.

- [ ] Config WiFi via NVS (similar ao captive portal do mimiclaw) + `esp-wifi` crate
- [ ] NTP client (`sntpc` ou similar) → atualiza RTC interno do ESP32-S3
- [ ] Channel RMK → UI expondo: layer atual, bateria left/right, status BLE de cada peripheral
- [ ] Widget "status bar": relógio + data + ícone WiFi + ícone BLE
- [ ] Widget "layer indicator": nome do layer ativo em destaque
- [ ] Widget "battery gauges": dois arcos ou barras, uma por metade
- [ ] Storage de GIFs em SPIFFS/LittleFS (partição reservada em F2)
- [ ] GIF decoder (`tinygif` ou port manual) + loop de playback em task dedicada
- [ ] Touch-to-UI: trocar entre telas (dashboard / galeria de GIFs / config)

---

## P-pendentes — Hardware / wiring (paralelo a F2+)

Ainda bloqueia F2 final das metades. Pode ser feito em qualquer momento.

- [ ] **Mapear pinos da matriz esquerda** em `left/keyboard.toml` (4 rows × 6 cols)
- [ ] **Mapear pinos da matriz direita** em `right/keyboard.toml`
- [ ] **Mapear pinos SPI da trackball** PMW3360DM em `right/keyboard.toml` (`sck`, `mosi`, `miso`, `cs`, `cpi`)
- [ ] Decidir diodo (`row2col = true` ou não) conforme PCB
- [ ] Re-habilitar jobs `left`/`right` em `.github/workflows/build.yml`

---

## Pendências menores (qualquer fase)

- [ ] Migrar TD0 (BTN1/BTN2 tap/doubletap, 100ms) e TD1 (Quote, 220ms) para `[behavior.morse]` — ou descartar se forem vestigiais
- [ ] Decidir destino do combo BTN1+MS_U → VolumeUp do `.vil` (parece vestigial)
- [ ] Refinar `dongle/vial.json` com stagger real de coluna do Charybdis
- [ ] Pinar `rmk_version` em commit específico quando estável (hoje está `main`)
- [ ] Adicionar badge de build ou `README.md`
