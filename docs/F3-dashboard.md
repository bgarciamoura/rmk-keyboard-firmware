# F3 — Dashboard dinâmico no display + Bongo Cat

**Status**: concluído em hardware 2026-04-20. Pendências menores: bateria L/R (3.4) aguarda F1.1.

## Visão geral

O dongle ESP32-S3 exibe no display JD9853 172×320 um dashboard em tempo real com:

- Header verde "Hello RMK" (estático)
- **Uptime** no formato `HH:MM:SS` atualizado a cada 500 ms
- **Layer atual** (0..5) sincronizado com mudanças em tempo real
- **Status BLE** dos peripherals `L:` e `R:` (online/offline)
- **Bongo Cat** 128×128 (sprite 64×64 @ 2×) animando idle e reagindo a keypress

Tudo isso roda **em paralelo com `run_rmk`** — USB HID, Vial e BLE funcionando normalmente.

## Arquitetura

### Crate layout

```
dongle/
  src/
    lib.rs                     # expõe assets + drivers como lib compartilhada
    central.rs                 # bin principal — #[rmk_keyboard] + #[overwritten] + #[register_processor]
    drivers/
      mod.rs
      jd9853.rs                # driver do LCD, implementa DrawTarget + blit_bitmap_1bpp
    assets/
      mod.rs
      bongo.rs                 # frames do Bongo Cat via include_bytes!
      bongo/
        frame_00.bin..12.bin   # 13× 512 bytes, 1 bpp Adafruit_GFX layout
    bin/
      display_hello.rs         # bin standalone de regressão pro driver
```

### Fluxo de init

A macro `#[rmk_keyboard]` em `central.rs` gera o `main`. Dois hooks são usados:

1. **`#[overwritten(chip_init)]`** — substitui o init default. Replicamos o fluxo (esp-hal + heap + RTOS + BLE controller + `build_ble_stack`) SEM tocar no display. O display é deixado pra depois porque queremos o ownership dentro do processor.

2. **`#[register_processor(poll)]`** — o body é colado no main entre `flash_init` e `run_rmk`. Aqui `p` (peripherals) ainda tem SPI2 + GPIOs do display disponíveis (RMK só usou USB0, GPIO19/20, FLASH, GPIO1/2 até esse ponto). O body inicializa SPI + JD9853, desenha o layout estático, e **retorna um `DisplayUi`** que vira task paralela do `run_rmk` via `PollingProcessor::polling_loop`.

### Struct DisplayUi

Anotado com `#[processor(subscribe = [KeyboardEvent, LayerChangeEvent, PeripheralConnectedEvent], poll_interval = 500)]`. A macro gera impls de `Processor` + `PollingProcessor` + `Runnable` despachando:

- `on_keyboard_event` → dispara tap do Bongo
- `on_layer_change_event` → atualiza `last_layer` + marca `layer_dirty`
- `on_peripheral_connected_event` → atualiza `left_online`/`right_online` por `id` (0=left, 1=right)
- `poll()` → chamado a cada 500 ms pelo polling_loop default; redesenha só o que está dirty

O struct vive em crate root (dentro do `mod keyboard` seria descartado pela rmk-macro).

### State machine do Bongo Cat

Dois estados controlados por `bongo_tap_remaining`:

- `tap_remaining > 0` → modo tap: mantém o frame tap atual (TAP_LEFT/TAP_RIGHT) por 2 polls (1 s). Quando chega a 0, reseta pra idle frame 0 e zera `quiet_polls`.
- `tap_remaining == 0` e `quiet_polls < 60` → modo idle: avança `bongo_idle_idx` a cada poll, loop em 9 frames (4.5 s por ciclo).
- `tap_remaining == 0` e `quiet_polls >= 60` (30 s) → freeze: para de animar pra economizar SPI.

Handler de keypress alterna `bongo_flip` a cada evento pra simular tap esquerdo/direito alternado (sem fonte real de "qual metade" até F1.1; quando peripherals existirem, dá pra usar `event.col()` pra detectar o lado real).

### Render incremental

Padrão repetido pra cada campo do dashboard:

1. Handler do evento marca `<campo>_dirty = true` quando estado interno mudou.
2. `poll()` chega a cada 500 ms.
3. Pra cada bloco (uptime / layer / L-R / bongo), se dirty:
   - Limpa retângulo afetado com cor de fundo (`BG = Rgb565::new(3, 6, 12)`)
   - Redesenha texto/sprite novo
   - Seta dirty = false

Uptime é exceção — redesenha **sempre** (mudança a cada poll é esperada).

### Driver JD9853

Método-chave adicionado pro Bongo Cat:

```rust
pub fn blit_bitmap_1bpp(
    &mut self,
    x: u16, y: u16,
    src_w: u16, src_h: u16,
    scale: u8,
    bitmap: &[u8],
    fg: Rgb565, bg: Rgb565,
)
```

- Formato do bitmap: **Adafruit_GFX row-major MSB-first**, bate 1:1 com `embedded_graphics::ImageRaw<BinaryColor>`
- `set_window(x, y, x+out_w-1, y+out_h-1)` uma vez
- Line buffer stack de 512 bytes (suporta até 128 px de largura output)
- Pra cada linha source: expande cada bit em `scale` cópias horizontais, envia linha montada `scale` vezes via SPI (replica vertical)
- 64×64 @ 2× = 32 KB sobre SPI 80 MHz ≈ 3 ms

## Fontes de dados usadas

| Campo | Evento RMK | Quando dispara |
|---|---|---|
| Uptime | — (usa `embassy_time::Instant::now()`) | a cada poll |
| Layer | `rmk::event::LayerChangeEvent(pub u8)` | em qualquer MO/TG/TT/OSL/LT/TO/DF |
| L/R online | `rmk::event::PeripheralConnectedEvent { id, connected }` | antes de cada tentativa de connect + após sucesso |
| Bongo tap | `rmk::event::KeyboardEvent` (handler no-op p/ conteúdo) | a cada keypress do matrix |

## Layout pixel-a-pixel

```
y=0..28     Header (barra verde + "Hello RMK")
y=32..52    Uptime: HH:MM:SS               FONT_10X20
y=56..76    Layer: N                       FONT_10X20
y=80..90    BLE:  scanning                 FONT_6X10 (placeholder)
y=94..104   L:    online/offline           FONT_6X10 verde (on) / dim (off)
y=108..118  R:    online/offline           FONT_6X10 verde (on) / dim (off)
y=130..257  Bongo Cat 128×128 em x=22..149
```

## Dependências adicionadas

Em `dongle/Cargo.toml`:

- `heapless = "0.8"` — `heapless::String<N>` pra formatar `HH:MM:SS` e `"Layer: N"` sem alloc
- `embassy-time = "0.5"` (bumpado de 0.4 em 2026-04-20 pra unificar com versão transitiva do `rmk` git-main)

## Por que não `rmk::display` nativo

O TOML do RMK tem `[display]` nativo mas:

1. `DisplayDriver` enum fechado aceita só SSD1306, SH110x — **JD9853 não está na lista**
2. Todo codegen SPI em `rmk-macro/src/codegen/display.rs` faz `panic!("SPI display interface is not yet supported")` — **só I2C implementado hoje**

Nosso display é 172×320 RGB565 SPI 80 MHz. A opção canônica é `#[register_processor(poll)]` até upstream implementar SPI + registrar driver extensível.

## Licença dos sprites

`NOTICE.md` na raiz credita `@pixelbunny` (origem da arte) e `sidharth-458` (repo fonte dos bytes). Upstream não tem licença declarada — redistribuição aqui é uso pessoal/educacional. Pra fork público, considerar regerar sprites de fonte CC0.

## Pendências F3

- **3.4 bateria L/R**: depende de F1.1 (peripherals enviando ADC da VDDH pela BLE). Reservado campo no dashboard mas sem dados ainda.
- Teste de cores puras pra verificar ordem MADCTL (se cores aparecem espelhadas, trocar `0x36` data byte na `INIT_SEQ`).

## Arquivos-chave a consultar em sessões futuras

- `dongle/src/central.rs` — arquitetura DisplayUi + state machine
- `dongle/src/drivers/jd9853.rs` — driver + blit
- `dongle/src/assets/bongo.rs` — sprites
- `.wolf/cerebrum.md` — learnings (imports, embassy_time version, WDT/storage, etc.)
