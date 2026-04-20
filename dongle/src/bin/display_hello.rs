//! Display hello — mesmo driver JD9853 do display_test, mas implementa
//! `DrawTarget<Color = Rgb565>` do embedded-graphics e desenha "Hello RMK".
//!
//! Estratégia:
//! - Wrapper `Jd9853Display` encapsula SPI + GPIOs do LCD (CS/DC).
//! - Implementa `DrawTarget<Color = Rgb565>` — método `draw_iter` itera
//!   pixels e os endereça via CASET+RASET+RAMWR.
//! - Também implementa `OriginDimensions` pra o tamanho visível 172×320.
//! - Um `fill_contiguous` otimizado para bulk fills (usado pelo background
//!   solid e pelo clear) evita o overhead de endereçamento por pixel.
//!
//! Depois de validado, este código vira a base do módulo reutilizável
//! `dongle/src/drivers/jd9853.rs` para F5 (UI completa).

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_6X10};
use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};

use embedded_hal::spi::SpiBus;

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;

use log::info;

esp_bootloader_esp_idf::esp_app_desc!();

// Janela visível do painel (mesma do display_test).
const LCD_W: u16 = 172;
const LCD_H: u16 = 320;
const LCD_X_OFFSET: u16 = 34;

// Init sequence idêntica ao display_test — JD9853 de Waveshare.
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

/// Display wrapper que envolve SPI + pinos de controle do LCD.
/// CS e DC são GPIO managed manualmente; RST e BL ficam fora (só precisam
/// durante o reset inicial).
struct Jd9853Display<'a, S: SpiBus> {
    spi: S,
    cs: Output<'a>,
    dc: Output<'a>,
}

impl<'a, S: SpiBus> Jd9853Display<'a, S> {
    fn new(spi: S, cs: Output<'a>, dc: Output<'a>) -> Self {
        Self { spi, cs, dc }
    }

    /// Envia cmd+data numa única transação CS-low. Mesmo padrão do display_test.
    fn write_cmd(&mut self, cmd: u8, data: &[u8]) {
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

    /// Define a janela de escrita (CASET + RASET) — coordenadas já incluem
    /// o offset do painel (x += LCD_X_OFFSET).
    fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) {
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

    /// Abre transação RAMWR (0x2C) — CS fica LOW e DC em HIGH após retornar.
    /// Chamador deve fechar com `self.cs.set_high()` depois de mandar pixels.
    fn start_ramwr(&mut self) {
        self.cs.set_low();
        self.dc.set_low();
        let _ = self.spi.write(&[0x2C]);
        let _ = self.spi.flush();
        self.dc.set_high();
    }

    fn end_ramwr(&mut self) {
        self.cs.set_high();
    }

    /// Fill rápido de um retângulo com cor sólida. Usa chunks de 64 bytes
    /// (32 pixels) pra caber no FIFO do SPI2 sem DMA.
    fn fill_rect(&mut self, x0: u16, y0: u16, x1: u16, y1: u16, color: Rgb565) {
        if x0 > x1 || y0 > y1 {
            return;
        }
        self.set_window(x0, y0, x1, y1);

        let raw = color.into_storage(); // u16 RGB565
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

    /// Versão draw_iter: endereça cada pixel isoladamente. Lenta pra áreas
    /// grandes mas ok pra texto (poucos pixels por frame).
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

    /// fill_solid otimizado — o fill_rect usa chunks grandes em vez de
    /// pixel-a-pixel, tornando clears e backgrounds muito mais rápidos.
    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        if area.size.width == 0 || area.size.height == 0 {
            return Ok(());
        }
        let x0 = area.top_left.x.max(0) as u16;
        let y0 = area.top_left.y.max(0) as u16;
        let x1 = ((area.top_left.x + area.size.width as i32 - 1)
            .min(LCD_W as i32 - 1)) as u16;
        let y1 = ((area.top_left.y + area.size.height as i32 - 1)
            .min(LCD_H as i32 - 1)) as u16;
        self.fill_rect(x0, y0, x1, y1, color);
        Ok(())
    }
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    info!("=== display_hello boot ===");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 32 * 1024);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    let out_cfg = OutputConfig::default();
    let cs = Output::new(peripherals.GPIO21, Level::High, out_cfg);
    let dc = Output::new(peripherals.GPIO45, Level::Low, out_cfg);
    let mut rst = Output::new(peripherals.GPIO40, Level::High, out_cfg);
    let mut bl = Output::new(peripherals.GPIO46, Level::Low, out_cfg);

    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_mhz(80))
        .with_mode(Mode::_0);
    let spi = Spi::new(peripherals.SPI2, spi_config)
        .expect("SPI init falhou")
        .with_sck(peripherals.GPIO38)
        .with_mosi(peripherals.GPIO39);

    // Hardware reset.
    rst.set_low();
    Timer::after(Duration::from_millis(20)).await;
    rst.set_high();
    Timer::after(Duration::from_millis(150)).await;

    let mut display = Jd9853Display::new(spi, cs, dc);

    // Init sequence.
    info!("init JD9853...");
    for &(cmd, data, delay_ms) in INIT_SEQ {
        display.write_cmd(cmd, data);
        if delay_ms > 0 {
            Timer::after(Duration::from_millis(delay_ms as u64)).await;
        }
    }
    bl.set_high();

    // Fundo: limpa a tela inteira com azul escuro (via embedded-graphics).
    let _ = display.clear(Rgb565::new(3, 6, 12)); // ~#1a1a2e — dark navy
    info!("clear OK");

    // Barra superior decorativa — retângulo de cor destaque.
    let _ = Rectangle::new(Point::new(0, 0), Size::new(LCD_W as u32, 28))
        .into_styled(
            PrimitiveStyleBuilder::new()
                .fill_color(Rgb565::new(10, 25, 10)) // verde-musgo
                .build(),
        )
        .draw(&mut display);

    // Título "Hello RMK" em fonte grande, centrado verticalmente na barra.
    let title_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb565::new(20, 60, 30)) // verde-menta brilhante
        .build();
    let _ = Text::with_baseline("Hello RMK", Point::new(6, 4), title_style, Baseline::Top)
        .draw(&mut display);

    // Subtexto em fonte pequena.
    let sub_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Rgb565::new(18, 36, 18))
        .build();
    let _ = Text::with_baseline(
        "Charybdis 3x6 wireless",
        Point::new(8, 44),
        sub_style,
        Baseline::Top,
    )
    .draw(&mut display);

    let _ = Text::with_baseline(
        "F3 display OK",
        Point::new(8, 60),
        sub_style,
        Baseline::Top,
    )
    .draw(&mut display);

    let _ = Text::with_baseline(
        "JD9853 @ 80 MHz",
        Point::new(8, 76),
        sub_style,
        Baseline::Top,
    )
    .draw(&mut display);

    info!("=== Hello RMK pintado na tela ===");

    loop {
        Timer::after(Duration::from_secs(10)).await;
        info!("still alive");
    }
}
