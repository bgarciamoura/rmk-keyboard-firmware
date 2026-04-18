# TODO — Charybdis 3x6 Wireless (RMK)

Lista ordenada por **fase**. F1 está em execução; F2+ são projetos separados.

---

## F1 — Build do dongle verde no CI ✅

Objetivo alcançado em run `24594377654` (commit `fe383a8`): `firmware_bin` artifact produzido, 3m59s.

- [x] `.github/workflows/build.yml` buildando só o dongle (left/right comentados)
- [x] `split.central` com `rows=1, cols=1` defensivo
- [x] 6 layers traduzidos do `.vil` no `dongle/keyboard.toml`
- [x] Macros M0/M1 migradas (copy/paste via `[[behavior.macro.macros]]`)
- [x] Commit + push + job verde no GitHub Actions
- [x] Workarounds CI documentados: `[split.central.matrix]` dummy, `[split.peripheral.matrix]` dummy para cada peripheral, keycodes `AudioVolUp/Down`/`AudioMute`, `[keyboard] name` = "central" para match de path, remoção temporária de `[behavior.morse]` (embassy_time missing no template upstream)
- [ ] **Próximo passo seu**: baixar o `firmware_bin` e flashar no ESP32-S3:
  - `gh run download 24594377654 -n firmware_bin` (ou baixar via web UI)
  - `espflash flash rmk.bin` ou `esptool.py --chip esp32s3 write_flash 0x0 rmk.bin`
  - Confirmar enumeração USB: host deve ver "Charybdis 3x6 Wireless" (via `product_name`) como HID Keyboard + HID Mouse

## Pendências de keymap que ficaram para F2

- [ ] Readicionar `[behavior.morse]` com `hold_timeout`/`gap_timeout` e perfil HRM (home-row mods agora estão com defaults do RMK)
- [ ] Re-anexar `, HRM` aos `MT()` calls
- [ ] Revalidar nomes de keycode ainda não testados: `MouseBtn1..5`, `MouseWheelUp/Down`, `MediaPlayPause`, `MediaNextTrack`, `MediaPrevTrack`, `BrightnessUp/Down`, `KpPlus`, `Macro0/1`, `PrintScreen` (build passou, mas não temos confirmação visual ainda)

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
