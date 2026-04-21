// F3 MVP — dongle com RMK + display JD9853 "Hello RMK" estático.
//
// Estratégia: `#[overwritten(chip_init)]` substitui o chip_init default da
// macro rmk_keyboard. Replicamos o default literal (esp-hal init + heap 72 KiB
// + TIMG0 + sw_int + esp_rtos::start + TrngSource + Trng + BleConnector +
// ExternalController + build_ble_stack) e ADICIONAMOS, entre o esp_rtos::start
// e o BleConnector, o bring-up do display SPI2 + init sequence JD9853 +
// desenho "Hello RMK" numa transação única.
//
// O override é substituição textual pura (lido em rmk-macro/src/codegen/chip/
// chip_init.rs). Variáveis contratuais que precisam sair do bloco e seguir
// vivas no resto do main gerado: `p` (peripherals), `stack`, `rng`. As
// guardas de lifetime `_trng_source` e `host_resources` precisam continuar
// vivas até o fim do programa — drop prematuro crasheia.
//
// Pinos consumidos pelo display (zero colisão com USB/BLE/Flash/matrix):
//   SCLK GPIO38, MOSI GPIO39, CS GPIO21, DC GPIO45, RST GPIO40, BL GPIO46.

#![no_std]
#![no_main]

use esp_backtrace as _;

// ============================================================================
// Imports em crate root.
//
// A macro rmk_keyboard copia os `use` de dentro de `mod keyboard` também
// para este escopo. Por isso os imports ficam SÓ aqui — duplicá-los dentro
// do mod gera E0252 "defined multiple times".
// ============================================================================

use embassy_time::{Duration, Timer};

use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_10X20};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Baseline, Text};

use embedded_hal::spi::SpiBus;

use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;

use log::info;

// Janela visível do painel.
const LCD_W: u16 = 172;
const LCD_H: u16 = 320;
const LCD_X_OFFSET: u16 = 34;

// Init sequence JD9853 — Waveshare ESP32-S3-Touch-LCD-1.47.
type InitCmd = (u8, &'static [u8], u16);

const INIT_SEQ: &[InitCmd] = &[
    (0x11, &[], 120),
    (0x36, &[0x00], 0),
    (0xDF, &[0x98, 0x53], 0),
    (0xDF, &[0x98, 0x53], 0),
    (0xB2, &[0x23], 0),
    (0xB7, &[0x00, 0x47, 0x00, 0x6F], 0),
    (0xBB, &[0x1C, 0x1A, 0x55, 0x73, 0x63, 0xF0], 0),
    (0xC0, &[0x44, 0xA4], 0),
    (0xC1, &[0x16], 0),
    (0xC3, &[0x7D, 0x07, 0x14, 0x06, 0xCF, 0x71, 0x72, 0x77], 0),
    (0xC4, &[0x00, 0x00, 0xA0, 0x79, 0x0B, 0x0A, 0x16, 0x79, 0x0B, 0x0A, 0x16, 0x82], 0),
    (0xC8, &[
        0x3F, 0x32, 0x29, 0x29, 0x27, 0x2B, 0x27, 0x28, 0x28, 0x26, 0x25, 0x17, 0x12, 0x0D, 0x04, 0x00,
        0x3F, 0x32, 0x29, 0x29, 0x27, 0x2B, 0x27, 0x28, 0x28, 0x26, 0x25, 0x17, 0x12, 0x0D, 0x04, 0x00,
    ], 0),
    (0xD0, &[0x04, 0x06, 0x6B, 0x0F, 0x00], 0),
    (0xD7, &[0x00, 0x30], 0),
    (0xE6, &[0x14], 0),
    (0xDE, &[0x01], 0),
    (0xB7, &[0x03, 0x13, 0xEF, 0x35, 0x35], 0),
    (0xC1, &[0x14, 0x15, 0xC0], 0),
    (0xC2, &[0x06, 0x3A], 0),
    (0xC4, &[0x72, 0x12], 0),
    (0xBE, &[0x00], 0),
    (0xDE, &[0x02], 0),
    (0xE5, &[0x00, 0x02, 0x00], 0),
    (0xE5, &[0x01, 0x02, 0x00], 0),
    (0xDE, &[0x00], 0),
    (0x35, &[0x00], 0),
    (0x3A, &[0x05], 0),
    (0x2A, &[0x00, 0x22, 0x00, 0xCD], 0),
    (0x2B, &[0x00, 0x00, 0x01, 0x3F], 0),
    (0xDE, &[0x02], 0),
    (0xE5, &[0x00, 0x02, 0x00], 0),
    (0xDE, &[0x00], 0),
    (0x29, &[], 20),
    (0x21, &[], 0),
];

// Wrapper JD9853 — implementa DrawTarget do embedded-graphics.
pub struct Jd9853Display<'a, S: SpiBus> {
    pub spi: S,
    pub cs: Output<'a>,
    pub dc: Output<'a>,
}

impl<'a, S: SpiBus> Jd9853Display<'a, S> {
    pub fn new(spi: S, cs: Output<'a>, dc: Output<'a>) -> Self {
        Self { spi, cs, dc }
    }

    // Envia cmd+data numa única transação CS-low. Se puxarmos CS high entre
    // cmd e data, o JD9853 descarta o cmd silenciosamente.
    pub fn write_cmd(&mut self, cmd: u8, data: &[u8]) {
        self.cs.set_low();
        self.dc.set_low();
        let _ = self.spi.write(&[cmd]);
        let _ = self.spi.flush();
        if !data.is_empty() {
            self.dc.set_high();
            let _ = self.spi.write(data);
            let _ = self.spi.flush();
        }
        self.cs.set_high();
    }

    pub fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) {
        let cx0 = x0 + LCD_X_OFFSET;
        let cx1 = x1 + LCD_X_OFFSET;
        self.write_cmd(
            0x2A,
            &[(cx0 >> 8) as u8, cx0 as u8, (cx1 >> 8) as u8, cx1 as u8],
        );
        self.write_cmd(
            0x2B,
            &[(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8],
        );
    }

    pub fn start_ramwr(&mut self) {
        self.cs.set_low();
        self.dc.set_low();
        let _ = self.spi.write(&[0x2C]);
        let _ = self.spi.flush();
        self.dc.set_high();
    }

    pub fn end_ramwr(&mut self) {
        self.cs.set_high();
    }

    pub fn fill_rect(&mut self, x0: u16, y0: u16, x1: u16, y1: u16, color: Rgb565) {
        if x0 > x1 || y0 > y1 {
            return;
        }
        self.set_window(x0, y0, x1, y1);

        let raw = color.into_storage();
        let hi = (raw >> 8) as u8;
        let lo = raw as u8;
        let mut chunk = [0u8; 64];
        for i in 0..32 {
            chunk[i * 2] = hi;
            chunk[i * 2 + 1] = lo;
        }

        let total_pixels = (x1 - x0 + 1) as u32 * (y1 - y0 + 1) as u32;
        let full_chunks = total_pixels / 32;
        let remainder_pixels = total_pixels % 32;

        self.start_ramwr();
        for _ in 0..full_chunks {
            let _ = self.spi.write(&chunk);
            let _ = self.spi.flush();
        }
        if remainder_pixels > 0 {
            let bytes = (remainder_pixels * 2) as usize;
            let _ = self.spi.write(&chunk[..bytes]);
            let _ = self.spi.flush();
        }
        self.end_ramwr();
    }
}

impl<'a, S: SpiBus> OriginDimensions for Jd9853Display<'a, S> {
    fn size(&self) -> Size {
        Size::new(LCD_W as u32, LCD_H as u32)
    }
}

impl<'a, S: SpiBus> DrawTarget for Jd9853Display<'a, S> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels.into_iter() {
            let x = coord.x;
            let y = coord.y;
            if x < 0 || y < 0 || x >= LCD_W as i32 || y >= LCD_H as i32 {
                continue;
            }
            let x = x as u16;
            let y = y as u16;
            self.set_window(x, y, x, y);
            let raw = color.into_storage();
            let bytes = [(raw >> 8) as u8, raw as u8];
            self.start_ramwr();
            let _ = self.spi.write(&bytes);
            let _ = self.spi.flush();
            self.end_ramwr();
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        if area.size.width == 0 || area.size.height == 0 {
            return Ok(());
        }
        let x0 = area.top_left.x.max(0) as u16;
        let y0 = area.top_left.y.max(0) as u16;
        let x1 =
            ((area.top_left.x + area.size.width as i32 - 1).min(LCD_W as i32 - 1)) as u16;
        let y1 = ((area.top_left.y + area.size.height as i32 - 1).min(LCD_H as i32 - 1))
            as u16;
        self.fill_rect(x0, y0, x1, y1, color);
        Ok(())
    }
}

// ============================================================================
// Módulo consumido pelo rmk-macro. Apenas `use` e a fn do override são
// preservados — qualquer outra coisa é descartada silenciosamente.
// ============================================================================

use rmk::macros::rmk_keyboard;

#[rmk_keyboard]
mod keyboard {
    // Imports ficam no crate root — a macro copia os `use` pro main e colide
    // se houver duplicação. O corpo do override abaixo enxerga os nomes via
    // imports de crate root (main fica no mesmo escopo).

    // Override textual do chip_init default (lido em
    // rmk-macro/src/codegen/chip/chip_init.rs ramo ChipSeries::Esp32,
    // peripheral_id=None). O corpo é colado verbatim no main gerado.
    //
    // Nomes contratuais que precisam sair daqui: `p`, `stack`, `rng`.
    // Guardas de lifetime que precisam ficar vivas: `_trng_source`,
    // `host_resources`, `timg0`, `software_interrupt`.
    #[overwritten(chip_init)]
    async fn chip_init_with_display() {
        ::esp_println::logger::init_logger_from_env();
        info!("=== chip_init_with_display: start ===");

        let p = ::esp_hal::init(
            ::esp_hal::Config::default()
                .with_cpu_clock(::esp_hal::clock::CpuClock::max()),
        );
        ::esp_alloc::heap_allocator!(size: 72 * 1024);
        let timg0 = ::esp_hal::timer::timg::TimerGroup::new(p.TIMG0);
        let software_interrupt =
            ::esp_hal::interrupt::software::SoftwareInterruptControl::new(p.SW_INTERRUPT);
        ::esp_rtos::start(timg0.timer0, software_interrupt.software_interrupt0);

        // ---- Display bring-up ----
        // async já funciona (esp_rtos::start executado). BLE ainda não está
        // competindo por CPU → SPI síncrono a 80 MHz roda sem disputa.
        info!("display: configurando pinos + SPI2 @ 80 MHz");
        let out_cfg = OutputConfig::default();
        let cs = Output::new(p.GPIO21, Level::High, out_cfg);
        let dc = Output::new(p.GPIO45, Level::Low, out_cfg);
        let mut rst = Output::new(p.GPIO40, Level::High, out_cfg);
        let mut bl = Output::new(p.GPIO46, Level::Low, out_cfg);

        let spi_config = SpiConfig::default()
            .with_frequency(Rate::from_mhz(80))
            .with_mode(Mode::_0);
        let spi = Spi::new(p.SPI2, spi_config)
            .expect("SPI init falhou")
            .with_sck(p.GPIO38)
            .with_mosi(p.GPIO39);

        // Hardware reset.
        rst.set_low();
        Timer::after(Duration::from_millis(20)).await;
        rst.set_high();
        Timer::after(Duration::from_millis(150)).await;

        let mut display = crate::Jd9853Display::new(spi, cs, dc);

        info!("display: init sequence JD9853 (34 cmds)");
        for &(cmd, data, delay_ms) in crate::INIT_SEQ {
            display.write_cmd(cmd, data);
            if delay_ms > 0 {
                Timer::after(Duration::from_millis(delay_ms as u64)).await;
            }
        }
        bl.set_high();

        // Fundo azul escuro.
        let _ = display.clear(Rgb565::new(3, 6, 12));

        // Barra superior verde-musgo.
        let _ = Rectangle::new(Point::new(0, 0), Size::new(crate::LCD_W as u32, 28))
            .into_styled(
                PrimitiveStyleBuilder::new()
                    .fill_color(Rgb565::new(10, 25, 10))
                    .build(),
            )
            .draw(&mut display);

        // Título "Hello RMK".
        let title_style = MonoTextStyleBuilder::new()
            .font(&FONT_10X20)
            .text_color(Rgb565::new(20, 60, 30))
            .build();
        let _ =
            Text::with_baseline("Hello RMK", Point::new(6, 4), title_style, Baseline::Top)
                .draw(&mut display);

        // Subtextos.
        let sub_style = MonoTextStyleBuilder::new()
            .font(&FONT_6X10)
            .text_color(Rgb565::new(18, 36, 18))
            .build();
        let _ = Text::with_baseline(
            "Charybdis 3x6 RMK",
            Point::new(8, 44),
            sub_style,
            Baseline::Top,
        )
        .draw(&mut display);
        let _ = Text::with_baseline(
            "Display + USB OK",
            Point::new(8, 60),
            sub_style,
            Baseline::Top,
        )
        .draw(&mut display);

        info!("display: Hello RMK pintado — cedendo controle ao RMK");

        // Evita o Drop de Output (esp-hal 1.0 devolve GPIO como input
        // flutuante, apagando o display e soltando o backlight). Os pinos
        // GPIO21/38/39/40/45/46 não são reusados pelo RMK (usb_init usa
        // GPIO19/20; flash_init não usa GPIO; matrix_config usa GPIO1/2).
        core::mem::forget(display);
        core::mem::forget(rst);
        core::mem::forget(bl);

        // ---- Continuação do chip_init default ----
        let _trng_source = ::esp_hal::rng::TrngSource::new(p.RNG, p.ADC1);
        let mut rng = ::esp_hal::rng::Trng::try_new().unwrap();
        let connector = ::esp_radio::ble::controller::BleConnector::new(p.BT, Default::default())
            .unwrap();
        let controller: ::bt_hci::controller::ExternalController<_, 64> =
            ::bt_hci::controller::ExternalController::new(connector);
        let ble_addr = [0xC0u8, 0xDE, 0xC0, 0xDE, 0x00, 0x01];
        let mut host_resources = ::rmk::HostResources::new();
        let stack =
            ::rmk::ble::build_ble_stack(controller, ble_addr, &mut rng, &mut host_resources)
                .await;
        if ::rmk::ble::passkey_entry_enabled() {
            stack.set_io_capabilities(::rmk::IoCapabilities::KeyboardOnly);
        }

        info!("=== chip_init_with_display: done ===");
    }
}
