//! Driver JD9853 para o LCD SPI do ESP32-S3-Touch-LCD-1.47 (172×320 RGB565).
//!
//! Implementa `DrawTarget<Color = Rgb565>` do embedded-graphics com:
//! - `draw_iter` pixel-a-pixel (lento, ok pra texto)
//! - `fill_solid` otimizado via `fill_rect` com chunks de 64 bytes
//!
//! Uso canônico:
//! ```ignore
//! use rmk_dongle::drivers::jd9853::{Jd9853Display, INIT_SEQ, LCD_W};
//!
//! let mut display = Jd9853Display::new(spi, cs, dc);
//! for &(cmd, data, delay_ms) in INIT_SEQ {
//!     display.write_cmd(cmd, data);
//!     if delay_ms > 0 {
//!         Timer::after(Duration::from_millis(delay_ms as u64)).await;
//!     }
//! }
//! bl.set_high();
//! display.clear(Rgb565::BLACK)?;
//! ```
//!
//! Gotcha essencial (ver cerebrum): CS deve ficar LOW durante cmd+data numa
//! única transação. Puxar CS high entre os dois faz o JD9853 descartar o cmd
//! silenciosamente — tela preta sem erro.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use embedded_hal::spi::SpiBus;
use esp_hal::gpio::Output;

/// Largura visível do painel.
pub const LCD_W: u16 = 172;
/// Altura visível do painel.
pub const LCD_H: u16 = 320;
/// Offset horizontal — o painel tem 34 colunas "escondidas" antes da área visível.
pub const LCD_X_OFFSET: u16 = 34;

/// Comando de init: (cmd, data, delay_ms depois).
pub type InitCmd = (u8, &'static [u8], u16);

/// Sequência completa de init do JD9853 (34 comandos). Port da referência C
/// em `mimiclaw/components/esp_lcd_jd9853/esp_lcd_jd9853.c`.
pub const INIT_SEQ: &[InitCmd] = &[
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

/// Wrapper do display — possui o barramento SPI e os pinos CS/DC.
pub struct Jd9853Display<'a, S: SpiBus> {
    pub spi: S,
    pub cs: Output<'a>,
    pub dc: Output<'a>,
}

impl<'a, S: SpiBus> Jd9853Display<'a, S> {
    pub fn new(spi: S, cs: Output<'a>, dc: Output<'a>) -> Self {
        Self { spi, cs, dc }
    }

    /// Envia cmd+data numa única transação CS-low.
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

    /// Define janela de escrita (CASET + RASET). Coordenadas já incluem `LCD_X_OFFSET`.
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

    /// Abre transação RAMWR (0x2C). CS fica LOW, DC volta HIGH pra dados.
    /// Chamador fecha com `end_ramwr` depois de enviar pixels.
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

    /// Fill rápido de um retângulo com cor sólida. Usa chunks de 32 pixels
    /// (64 bytes) pra caber no FIFO do SPI2 sem DMA.
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
