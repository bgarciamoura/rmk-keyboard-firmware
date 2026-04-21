// F3 — dongle com RMK + display JD9853 + render loop paralelo.
//
// Arquitetura:
// - `#[overwritten(chip_init)]`: roda o chip_init default (esp-hal + RTOS +
//   BLE controller + build_ble_stack). NÃO inicializa o display.
// - `#[register_processor(poll)]`: body inicializa SPI2 + GPIOs + JD9853,
//   desenha o layout estático (fundo + barra + título), retorna um
//   `DisplayUi` que vira task paralela do `run_rmk` via `PollingProcessor`.
// - Struct `DisplayUi` (crate root, macro rmk-macro descarta structs dentro
//   do mod keyboard) implementa `PollingProcessor` via `#[processor]` —
//   método `poll()` atualiza o contador a cada 500 ms.
//
// Por que `register_processor` e não init no chip_init: o struct precisa
// sobreviver até o `join` final. Se init acontecesse em chip_init, teríamos
// que mover o display via static/StaticCell. Fazer tudo no register_processor
// mantém ownership simples — o body cria o struct, o macro guarda em `let
// mut display_ui = { body };` e chama `.polling_loop().await` no join.

#![no_std]
#![no_main]

use esp_backtrace as _;

// ============================================================================
// Imports em crate root. A rmk-macro copia os `use` do `mod keyboard` pra cá;
// por isso não duplicamos (E0252).
// ============================================================================

use core::fmt::Write as _;

use embassy_time::{Duration, Instant, Timer};

use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_10X20};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Baseline, Text};

use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;

use heapless::String;

use log::info;

use rmk::event::{KeyboardEvent, LayerChangeEvent, PeripheralConnectedEvent};
use rmk::macros::{processor, rmk_keyboard};
use rmk_dongle::drivers::jd9853::{INIT_SEQ, Jd9853Display, LCD_W};

// ============================================================================
// Tipo concreto do SPI do dongle. esp-hal 1.0 retorna
// `Spi<'d, DM>` onde DM=Blocking por default. Pinos owned → lifetime `'static`.
// ============================================================================
type DongleSpi = esp_hal::spi::master::Spi<'static, esp_hal::Blocking>;

// ============================================================================
// DisplayUi — struct que vive no `join` final como PollingProcessor.
//
// O atributo `#[processor(...)]` (rmk::macros::processor) gera impls de
// Processor + PollingProcessor + Runnable. Assinamos KeyboardEvent só porque
// o supertrait exige pelo menos um evento; o handler é no-op.
//
// Campos `rst` e `bl` ficam no struct pra evitar o Drop (que resetaria GPIO
// como input flutuante, apagando a tela e soltando o backlight).
// ============================================================================

// ----------------------------------------------------------------------------
// Layout do dashboard (coordenadas em pixels).
//
// y=0..28   barra verde com "Hello RMK"
// y=32..52  "Uptime: HH:MM:SS"      FONT_10X20, atualiza a cada poll
// y=56..72  "Layer: 0"              FONT_10X20, placeholder (3.2)
// y=76..88  "BLE:   scanning"       FONT_6X10,  placeholder (3.3)
// y=90..102 "L:     offline"        FONT_6X10,  placeholder (3.4)
// y=104..116 "R:    offline"        FONT_6X10,  placeholder (3.4)
// ----------------------------------------------------------------------------

const UPTIME_Y: i32 = 32;
const UPTIME_H: u32 = 20;
const LAYER_Y: i32 = 56;
const LAYER_H: u32 = 20;
const LR_L_Y: i32 = 94;
const LR_R_Y: i32 = 108;
const LR_H: u32 = 10;

const BG: Rgb565 = Rgb565::new(3, 6, 12);

#[processor(
    subscribe = [KeyboardEvent, LayerChangeEvent, PeripheralConnectedEvent],
    poll_interval = 500
)]
pub struct DisplayUi {
    display: Jd9853Display<'static, DongleSpi>,
    _rst: Output<'static>,
    _bl: Output<'static>,
    last_layer: u8,
    // LayerChangeEvent só é emitido em mudanças reais — `true` inicial
    // garante que o primeiro poll pinte o valor de boot (0).
    layer_dirty: bool,
    // Estado de conexão dos peripherals split (id 0 = left, id 1 = right).
    // bt_dirty=true inicial força primeiro poll a sobrescrever placeholders.
    left_online: bool,
    right_online: bool,
    bt_dirty: bool,
}

impl DisplayUi {
    // Supertrait Processor exige handler pra cada evento assinado. KeyboardEvent
    // aqui é no-op — usado lá atrás só pra ter pelo menos um evento, porque
    // PollingProcessor exige ao menos um subscriber.
    async fn on_keyboard_event(&mut self, _event: KeyboardEvent) {}

    // Chamado a cada mudança de layer efetiva (MO/TG/TT/OSL/LT/TO/DF).
    // Evento já traz a topmost-active layer como u8 — não precisamos
    // reconstruir a stack.
    async fn on_layer_change_event(&mut self, event: LayerChangeEvent) {
        let new_layer = event.0;
        if new_layer != self.last_layer {
            self.last_layer = new_layer;
            self.layer_dirty = true;
        }
    }

    // PeripheralConnectedEvent é emitido pelo run_ble_peripheral_manager do
    // rmk::split::ble::central: connected=false antes de cada tentativa de
    // conexão (inclusive reconnect após drop) e connected=true após connect
    // bem-sucedido. `id` vem do índice em [[split.peripheral]]: 0=left, 1=right.
    async fn on_peripheral_connected_event(&mut self, event: PeripheralConnectedEvent) {
        match event.id {
            0 => {
                if self.left_online != event.connected {
                    self.left_online = event.connected;
                    self.bt_dirty = true;
                }
            }
            1 => {
                if self.right_online != event.connected {
                    self.right_online = event.connected;
                    self.bt_dirty = true;
                }
            }
            _ => {}
        }
    }

    async fn poll(&mut self) {
        // ---- Uptime (redesenha sempre) ----
        let total_secs = Instant::now().as_secs();
        let h = total_secs / 3600;
        let m = (total_secs / 60) % 60;
        let s = total_secs % 60;

        let _ = Rectangle::new(Point::new(0, UPTIME_Y), Size::new(LCD_W as u32, UPTIME_H))
            .into_styled(PrimitiveStyleBuilder::new().fill_color(BG).build())
            .draw(&mut self.display);

        let big = MonoTextStyleBuilder::new()
            .font(&FONT_10X20)
            .text_color(Rgb565::new(28, 56, 28))
            .build();

        let mut buf: String<32> = String::new();
        let _ = write!(buf, "Up {:02}:{:02}:{:02}", h, m, s);
        let _ = Text::with_baseline(buf.as_str(), Point::new(6, UPTIME_Y + 2), big, Baseline::Top)
            .draw(&mut self.display);

        // ---- Layer (só redesenha se mudou) ----
        if self.layer_dirty {
            self.layer_dirty = false;

            let _ = Rectangle::new(Point::new(0, LAYER_Y), Size::new(LCD_W as u32, LAYER_H))
                .into_styled(PrimitiveStyleBuilder::new().fill_color(BG).build())
                .draw(&mut self.display);

            let mut lbuf: String<16> = String::new();
            let _ = write!(lbuf, "Layer: {}", self.last_layer);
            let _ = Text::with_baseline(lbuf.as_str(), Point::new(6, LAYER_Y), big, Baseline::Top)
                .draw(&mut self.display);
        }

        // ---- L/R peripheral status (só redesenha se bt_dirty) ----
        if self.bt_dirty {
            self.bt_dirty = false;

            let style_on = MonoTextStyleBuilder::new()
                .font(&FONT_6X10)
                .text_color(Rgb565::new(20, 60, 30))
                .build();
            let style_off = MonoTextStyleBuilder::new()
                .font(&FONT_6X10)
                .text_color(Rgb565::new(16, 34, 18))
                .build();

            let _ = Rectangle::new(Point::new(0, LR_L_Y), Size::new(LCD_W as u32, LR_H))
                .into_styled(PrimitiveStyleBuilder::new().fill_color(BG).build())
                .draw(&mut self.display);
            let _ = Rectangle::new(Point::new(0, LR_R_Y), Size::new(LCD_W as u32, LR_H))
                .into_styled(PrimitiveStyleBuilder::new().fill_color(BG).build())
                .draw(&mut self.display);

            let (l_text, l_style) = if self.left_online {
                ("L:    online", style_on)
            } else {
                ("L:    offline", style_off)
            };
            let (r_text, r_style) = if self.right_online {
                ("R:    online", style_on)
            } else {
                ("R:    offline", style_off)
            };

            let _ = Text::with_baseline(l_text, Point::new(6, LR_L_Y), l_style, Baseline::Top)
                .draw(&mut self.display);
            let _ = Text::with_baseline(r_text, Point::new(6, LR_R_Y), r_style, Baseline::Top)
                .draw(&mut self.display);
        }
    }
}

// ============================================================================
// Módulo consumido pela rmk-macro.
// ============================================================================

#[rmk_keyboard]
mod keyboard {
    // Override textual do chip_init default. Agora SEM display bring-up —
    // display vai pro register_processor abaixo.
    #[overwritten(chip_init)]
    async fn chip_init_default() {
        ::esp_println::logger::init_logger_from_env();
        info!("=== chip_init_default: start ===");

        let p = ::esp_hal::init(
            ::esp_hal::Config::default()
                .with_cpu_clock(::esp_hal::clock::CpuClock::max()),
        );
        ::esp_alloc::heap_allocator!(size: 72 * 1024);
        let timg0 = ::esp_hal::timer::timg::TimerGroup::new(p.TIMG0);
        let software_interrupt =
            ::esp_hal::interrupt::software::SoftwareInterruptControl::new(p.SW_INTERRUPT);
        ::esp_rtos::start(timg0.timer0, software_interrupt.software_interrupt0);

        let _trng_source = ::esp_hal::rng::TrngSource::new(p.RNG, p.ADC1);
        let mut rng = ::esp_hal::rng::Trng::try_new().unwrap();
        let connector =
            ::esp_radio::ble::controller::BleConnector::new(p.BT, Default::default()).unwrap();
        let controller: ::bt_hci::controller::ExternalController<_, 64> =
            ::bt_hci::controller::ExternalController::new(connector);
        let ble_addr = [0xC0u8, 0xDE, 0xC0, 0xDE, 0x00, 0x01];
        let mut host_resources = ::rmk::HostResources::new();
        let stack = ::rmk::ble::build_ble_stack(
            controller,
            ble_addr,
            &mut rng,
            &mut host_resources,
        )
        .await;
        if ::rmk::ble::passkey_entry_enabled() {
            stack.set_io_capabilities(::rmk::IoCapabilities::KeyboardOnly);
        }

        info!("=== chip_init_default: done ===");
    }

    // Body inlined pela rmk-macro como:
    //   let mut display_ui = { <body> };
    // e depois empurrado no join final via `.polling_loop().await`.
    // `p` está no escopo (criado no chip_init acima).
    #[register_processor(poll)]
    async fn display_ui() {
        info!("display_ui: bring-up JD9853");

        let out_cfg = OutputConfig::default();
        let cs = Output::new(p.GPIO21, Level::High, out_cfg);
        let dc = Output::new(p.GPIO45, Level::Low, out_cfg);
        let mut rst = Output::new(p.GPIO40, Level::High, out_cfg);
        let mut bl = Output::new(p.GPIO46, Level::Low, out_cfg);

        let spi_cfg = SpiConfig::default()
            .with_frequency(Rate::from_mhz(80))
            .with_mode(Mode::_0);
        let spi = Spi::new(p.SPI2, spi_cfg)
            .expect("SPI init falhou")
            .with_sck(p.GPIO38)
            .with_mosi(p.GPIO39);

        rst.set_low();
        Timer::after(Duration::from_millis(20)).await;
        rst.set_high();
        Timer::after(Duration::from_millis(150)).await;

        let mut display = Jd9853Display::new(spi, cs, dc);
        for &(cmd, data, delay_ms) in INIT_SEQ {
            display.write_cmd(cmd, data);
            if delay_ms > 0 {
                Timer::after(Duration::from_millis(delay_ms as u64)).await;
            }
        }
        bl.set_high();

        // Layout estático base — desenhado uma vez.
        let _ = display.clear(Rgb565::new(3, 6, 12));

        let _ = Rectangle::new(Point::new(0, 0), Size::new(LCD_W as u32, 28))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::new(10, 25, 10))
                    .build(),
            )
            .draw(&mut display);

        let title_style = MonoTextStyleBuilder::new()
            .font(&FONT_10X20)
            .text_color(Rgb565::new(20, 60, 30))
            .build();
        let _ = Text::with_baseline("Hello RMK", Point::new(6, 4), title_style, Baseline::Top)
            .draw(&mut display);

        // Placeholders BLE/L/R — 3.3/3.4 vão substituir. Layer é pintado
        // pelo primeiro poll() (layer_dirty=true inicial no DisplayUi).
        let dim = MonoTextStyleBuilder::new()
            .font(&FONT_6X10)
            .text_color(Rgb565::new(16, 34, 18))
            .build();
        let _ = Text::with_baseline("BLE:  scanning", Point::new(6, 80), dim, Baseline::Top)
            .draw(&mut display);
        let _ = Text::with_baseline("L:    offline", Point::new(6, 94), dim, Baseline::Top)
            .draw(&mut display);
        let _ = Text::with_baseline("R:    offline", Point::new(6, 108), dim, Baseline::Top)
            .draw(&mut display);

        // Valor do bloco — `rst`/`bl` entram no struct pra evitar Drop.
        DisplayUi {
            display,
            _rst: rst,
            _bl: bl,
            last_layer: 0,
            layer_dirty: true,
            left_online: false,
            right_online: false,
            bt_dirty: true,
        }
    }
}
