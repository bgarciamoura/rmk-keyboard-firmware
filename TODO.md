# TODO — Charybdis 3x6 Wireless (RMK)

Lista ordenada por **fase**. F1, F2 e F2.5 concluídos; F2-final (`#[rmk_keyboard]` limpo) **validado em hardware em 2026-04-20** — dongle enumera stack HID completo, Vial reconhece os 6 layers, reboot estável. Próximas fases focam nos peripherals (F1.1, bloqueador crítico pro teclado digitar) e nas features extras do dongle (F3+).

---

## F1 — Build do dongle verde no CI ✅

Objetivo alcançado em run `24594377654` (commit `fe383a8`): `firmware_bin` artifact produzido, 3m59s.

- [x] `.github/workflows/build.yml` buildando só o dongle (left/right comentados)
- [x] `split.central` com `rows=1, cols=1` defensivo
- [x] 6 layers traduzidos do `.vil` no `dongle/keyboard.toml`
- [x] Macros M0/M1 migradas (copy/paste via `[[behavior.macro.macros]]`)
- [x] Commit + push + job verde no GitHub Actions
- [x] Workarounds CI documentados: `[split.central.matrix]` dummy, `[split.peripheral.matrix]` dummy para cada peripheral, keycodes `AudioVolUp/Down`/`AudioMute`, `[keyboard] name` = "central" para match de path, remoção temporária de `[behavior.morse]` (embassy_time missing no template upstream)
- [x] ~~Baixar e flashar~~ → Feito em 2026-04-19. **Resultado: o bin do workflow upstream NÃO enumera como USB HID no host**. Bootloader + partition + app têm headers válidos; o `rmk::ble::run_ble` trava num `.await` de flash read antes de chegar a spawnar o USB. Ver investigação em F2 abaixo.

## Pendências de keymap que ficaram para F2

- [ ] Readicionar `[behavior.morse]` com `hold_timeout`/`gap_timeout` e perfil HRM (home-row mods agora estão com defaults do RMK)
- [ ] Re-anexar `, HRM` aos `MT()` calls
- [ ] Revalidar nomes de keycode ainda não testados: `MouseBtn1..5`, `MouseWheelUp/Down`, `MediaPlayPause`, `MediaNextTrack`, `MediaPrevTrack`, `BrightnessUp/Down`, `KpPlus`, `Macro0/1`, `PrintScreen` (build passou, mas não temos confirmação visual ainda)

---

## F2 — Dongle vira projeto Rust ✅

CI verde no commit `1634620` (run `24596349354`), workflow paralelo `build-f2.yml` produzindo o artifact `dongle-f2-bin`. **A macro `#[rmk_keyboard]` compila e gera binário, mas ao flashar o firmware trava internamente no `rmk::ble::run_ble` — ver "F2 debug" abaixo para o workaround adotado.**

- [x] `dongle/Cargo.toml` — rmk em git main (crates.io 0.7 não tem feature `vial`), deps esp-rs com `[patch.crates-io]` pin em commit `20ed2bc`
- [x] `dongle/src/central.rs` — `#[rmk_keyboard]` via `rmk::macros::rmk_keyboard`, panic handler linkado com `use esp_backtrace as _;`
- [x] `dongle/.cargo/config.toml` — target xtensa, espflash runner, `KEYBOARD_TOML_PATH`/`VIAL_JSON_PATH` no env (exigido pela macro)
- [x] `dongle/rust-toolchain.toml` — channel `esp`
- [x] `dongle/build.rs` — comprime `vial.json` em XZ, gera `VIAL_KEYBOARD_DEF/ID` em `$OUT_DIR/config_generated.rs`, seta `-Tlinkall.x` como linker arg
- [x] `.github/workflows/build-f2.yml` — workflow paralelo, não toca em `build.yml`; F1 e F2 convivem

---

## F2 debug — isolamento do bug `rmk::ble::run_ble` ✅

Trilha de 3 probes incrementais (+ um binário minimal `usbtest/`) para localizar onde o firmware F1/F2 trava. **Tudo em `dongle/src/bin/` e `usbtest/`.**

- [x] `usbtest/` — crate isolado usando `embassy-usb` 0.5.1 + `usbd-hid` 0.9 + `esp-rtos` 0.2 (sem RMK). Flashado, enumerou como VID 4C4B:0001 e digitou 'a' a cada 3s → **USB HID stack OK**
- [x] `dongle/src/bin/probe.rs` V1 — passos 1-9 do init RMK (esp-hal → esp-rtos → BleConnector::new). Heartbeat OK → **passos 1-9 OK**
- [x] `dongle/src/bin/probe.rs` V2 — adicionou passos 10-15 (ExternalController + FlashStorage read/erase/write/read-back). Heartbeat OK → **esp-storage + bt-hci wrapper OK**
- [x] `dongle/src/bin/probe.rs` V3 — adicionou passos 16-18 (Usb::new() + HID enumeration DEPOIS de BLE + storage). Enumerou como VID 4C4B:0002 e digitou 'c' a cada 3s → **USB + BLE + storage coexistem sem problema**

**Conclusão do debug:** o travamento está 100% localizado dentro de `rmk::ble::run_ble` — especificamente nos passos pré-`join`: leitura de `StorageKey::ConnectionType` ou `profile_manager.load_bonded_devices(storage).await`. Isso trava o executor antes do USB task ser polled. O esp-hal/BLE controller/storage crus estão corretos.

---

## F2.5 — Workaround produção (`central_v2`) ✅ (obsoleto após F2-final)

Binário `dongle/src/bin/central_v2.rs` foi criado como workaround enquanto o travamento do `rmk::ble::run_ble` estava sendo investigado. **Deixou de ser necessário em 2026-04-20** quando o fix real foi descoberto (`[storage]` ausente no `keyboard.toml`). Mantido no repositório como referência de "USB HID minimal + BLE controller idle", útil pra diagnósticos futuros.

## F2-final — `#[rmk_keyboard]` validado em hardware ✅

Commit `1ee32be` (run `24689080197`, build 3m53s). Fix: adicionar `[storage] start_addr=0x3f0000 num_sectors=16` ao `dongle/keyboard.toml` — default do RMK (`num_sectors=2`) era insuficiente pra empacotar 288 chaves de keymap + 2 peer_addresses do split, disparando WDT.

- [x] Descobrir causa-raiz do travamento (via comparação com exemplo oficial esp32s3_ble)
- [x] Adicionar `[storage]` ao `dongle/keyboard.toml`
- [x] Build F2 verde em 3m53s
- [x] Flash via `espflash write-bin --chip esp32s3 0x0 dongle.bin` OK
- [x] Dongle enumera como "Charybdis 3x6 Wireless" (VID 4C4B:4643) com stack HID completo: Keyboard + Mouse + Consumer Control + Power/System + Vendor-defined
- [x] **Vial reconhece o dispositivo e lê os 6 layers**
- [x] Reboot (desconectar/reconectar USB ~3x) mantém estabilidade

### Pendências F2-final residuais (baixa prioridade)

- [ ] Deletar `build.yml` (F1 config-only obsoleto) e `build-central-v2.yml` quando F1.1 estiver completo; renomear `build-f2.yml` → `build.yml`
- [ ] Remover hacks do config-only em `keyboard.toml` (`[split.central.matrix]` dummy com pinos GPIO1/GPIO2, `[split.peripheral.matrix]` dummy com GPIO3-12 e GPIO13-33, `name = "central"` exigido pelo template upstream)
- [ ] Readicionar `[behavior.morse]` agora que estamos no Cargo próprio (F2) e `embassy_time` está disponível no escopo — era o motivo de ter sido removido no F1
- [ ] Re-investigar sintaxe atual de `[[behavior.macro.macros]]` em rmk main e re-adicionar M0 (Ctrl+Shift+C) / M1 (Ctrl+Shift+V)

---

## F3 — Display driver (JD9853 via SPI) ✅ (pendências menores)

Objetivo original: tela acende e imprime "Hello RMK". Expandido em 2026-04-20 pra dashboard completo + Bongo Cat. Apenas bateria L/R (3.4) pendente, esperando F1.1. Reaproveitou sequências de init de `C:\Users\bgarciamoura\projects\mimiclaw\components\esp_lcd_jd9853\esp_lcd_jd9853.c` (460 LoC C) como referência de port.

- [x] Identificar pinos SPI do display na placa (GPIO38/39 + CS=21, DC=45, RST=40, BL=46) — confirmado via `projects/esp32s3/CLAUDE.md` + `mimiclaw/bsp_display.h`
- [x] Port da sequência de init do JD9853 (34 comandos) de C → Rust em `dongle/src/bin/display_test.rs`
- [x] Fill vermelho a 80 MHz funcionando em <20 ms após boot (validado 2026-04-20)
- [x] Bug principal resolvido: CS deve ficar low durante cmd+data numa única transação (ver cerebrum)
- [x] Integrar `embedded-graphics` para desenhar "Hello RMK" — validado 2026-04-20 em `dongle/src/bin/display_hello.rs`
- [x] **MVP F3 validado em hardware** (commit `0104d31`, run `24699971413`): display "Hello RMK" + RMK completo + Vial, tudo coexistindo. `#[overwritten(chip_init)]` comprovado como caminho limpo.
- [x] Refatorar `Jd9853Display` de `central.rs` para módulo reutilizável (`dongle/src/drivers/jd9853.rs`) — concluído 2026-04-20 commit `97c4b6f`
- [x] Task Embassy paralela via `#[register_processor(poll)]` — validado 2026-04-20 commit `f9ba97c` run `24700880799`. `DisplayUi` struct com `PollingProcessor` desenha "tick: N" a cada 500 ms sem quebrar USB/BLE/Vial
- [x] **3.1** Dashboard skeleton com Uptime real (embassy_time::Instant) + placeholders — commit `4cafe99`
- [x] **3.2** Layer real via `rmk::event::LayerChangeEvent(pub u8)` — commit `4599085`; dirty bit + redraw seletivo
- [x] **3.3** Status L/R via `rmk::event::PeripheralConnectedEvent { id, connected }` — commit `3207048`; id 0=left, 1=right; disconnect coberto pelo loop de reconexão
- [x] **Bongo Cat animando (128×128 @ 2×)** — commits `960b9bd`/`93a5bee`/`59b820c`, run `24702405690`. Estrutura: 13 sprites 64×64 @ 1 bpp extraídos via `tools/extract_bongo_sprites.py`, `rmk_dongle::assets::bongo` expõe `IDLE_FRAMES` + `TAP_*`, `jd9853::blit_bitmap_1bpp` com scale inteiro, `DisplayUi` state machine (idle 9 frames × 500 ms, tap em KeyboardEvent 1 s, freeze após 30 s). Licença: ver `NOTICE.md` (upstream sem licença declarada, credita @pixelbunny).
- [ ] **3.4** Bateria L/R — depende de F1.1 (peripherals enviando reading ADC da VDDH). Bloqueado até haver hardware pra testar.
- [ ] Teste de cores puras (RGB squares) para confirmar ordem MADCTL vs câmera

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

## F1.1 — Firmwares dos peripherals (BLOQUEADOR CRÍTICO AGORA)

Com F2-final validado, esta fase é o único bloqueador real pra sair do estado "dongle enumera lindamente, mas nenhuma tecla digita" — não há fonte de eventos até os peripherals parearem.

- [ ] **Mapear pinos da matriz esquerda** em `left/keyboard.toml` (4 rows × 6 cols)
- [ ] **Mapear pinos da matriz direita** em `right/keyboard.toml`
- [ ] **Mapear pinos SPI da trackball** PMW3360DM em `right/keyboard.toml` (`sck`, `mosi`, `miso`, `cs`, `cpi`)
- [ ] Decidir diodo (`row2col = true` ou não) conforme PCB
- [ ] Re-habilitar jobs `left`/`right` em `.github/workflows/build.yml`
- [ ] Flashar via `probe-rs` nos nRF52840 (UF2 do artifact)
- [ ] Validar pareamento BLE: peripherals conectam no dongle (BLE addrs `...00:02` e `...00:03`)
- [ ] Adicionar task BLE ao `central_v2.rs` recebendo eventos HID dos peripherals

---

## Pendências menores (qualquer fase)

- [ ] Migrar TD0 (BTN1/BTN2 tap/doubletap, 100ms) e TD1 (Quote, 220ms) para `[behavior.morse]` — ou descartar se forem vestigiais
- [ ] Decidir destino do combo BTN1+MS_U → VolumeUp do `.vil` (parece vestigial)
- [ ] Refinar `dongle/vial.json` com stagger real de coluna do Charybdis
- [ ] Pinar `rmk_version` em commit específico quando estável (hoje está `main`)
- [ ] Adicionar badge de build ou `README.md`
