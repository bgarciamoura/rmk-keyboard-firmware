//! Display hello — bin standalone que exercita só o driver JD9853 (sem RMK).
//!
//! Usa o mesmo módulo `rmk_dongle::drivers::jd9853` que o `central.rs` (bin
//! de produção). Serve como regression test: se este bin para de pintar, o
//! driver quebrou para todo mundo.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

use embedded_graphics::mono_font::MonoTextStyleBuilder;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_10X20};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyleBuilder, Rectangle};
use embedded_graphics::text::{Baseline, Text};

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::spi::Mode;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;
use esp_hal::timer::timg::TimerGroup;

use log::info;

use rmk_dongle::drivers::jd9853::{INIT_SEQ, Jd9853Display, LCD_W};

esp_bootloader_esp_idf::esp_app_desc!();

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

    rst.set_low();
    Timer::after(Duration::from_millis(20)).await;
    rst.set_high();
    Timer::after(Duration::from_millis(150)).await;

    let mut display = Jd9853Display::new(spi, cs, dc);

    info!("init JD9853...");
    for &(cmd, data, delay_ms) in INIT_SEQ {
        display.write_cmd(cmd, data);
        if delay_ms > 0 {
            Timer::after(Duration::from_millis(delay_ms as u64)).await;
        }
    }
    bl.set_high();

    let _ = display.clear(Rgb565::new(3, 6, 12));
    info!("clear OK");

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
