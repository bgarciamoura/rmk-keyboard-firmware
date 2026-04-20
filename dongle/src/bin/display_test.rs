//! Display test — JD9853 1.47" LCD via SPI (Waveshare ESP32-S3-Touch-LCD-1.47).
//!
//! Alvo mínimo: inicializar o controlador, ligar o backlight, e preencher a
//! tela inteira de vermelho. Se aparecer um retângulo vermelho, prova que
//! SPI + GPIO controls + sequência de init do JD9853 funcionam — base pra
//! `embedded-graphics` e texto depois.
//!
//! Pinout — confirmado pelo bsp_display.h do mimiclaw (board conhecida-
//! funcional com este LCD). As macros EXAMPLE_PIN_LCD_* do componente
//! esp_bsp são a fonte da verdade:
//!   GPIO38 = LCD_SCLK  (SPI clock)
//!   GPIO39 = LCD_MOSI  (SPI MOSI)
//!   GPIO21 = LCD_CS    (chip select, ativo baixo)
//!   GPIO45 = LCD_DC    (data=high / command=low)
//!   GPIO40 = LCD_RST   (reset, ativo baixo)
//!   GPIO46 = LCD_BL    (backlight, LEDC PWM 5 kHz 10-bit no mimiclaw —
//!                      aqui simplificado pra GPIO HIGH direto)
//! SPI a 80 MHz é o que o mimiclaw usa (EXAMPLE_LCD_PIXEL_CLOCK_HZ).
//!
//! Janela visível: x=34..205 (172 cols), y=0..319. Offset x=34 é limitação
//! do controller — a tela física é 240 mas só 172 pixels são roteados.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

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

// Dimensões da janela visível do LCD (172×320).
const LCD_W: u16 = 172;
const LCD_H: u16 = 320;
const LCD_X_OFFSET: u16 = 34;

// RGB565 vermelho puro: R=31 (5 bits), G=0, B=0 → 0b11111 00000 00000 = 0xF800.
// Transmitido em big-endian (MSB primeiro) pelo JD9853.
const RED_HI: u8 = 0xF8;
const RED_LO: u8 = 0x00;

// Sequência de init extraída de mimiclaw/components/esp_lcd_jd9853/esp_lcd_jd9853.c
// linha 241. Formato: (cmd, &[data], delay_ms_após).
type InitCmd = (u8, &'static [u8], u16);

const INIT_SEQ: &[InitCmd] = &[
    (0x11, &[], 120),                                                          // SLPOUT
    // MADCTL não está no init_seq do mimiclaw, mas o driver C envia ele
    // separadamente antes do init. Sem isso, orientação/RGB order fica
    // indefinido após reset.
    (0x36, &[0x00], 0),                                                        // MADCTL: RGB order, no mirroring
    (0xDF, &[0x98, 0x53], 0),                                                  // enable ext cmd set
    (0xDF, &[0x98, 0x53], 0),                                                  // (repetido no driver original)
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
    ], 0),                                                                     // SET_R_GAMMA
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
    (0x35, &[0x00], 0),                                                        // TEON (tearing effect on)
    (0x3A, &[0x05], 0),                                                        // COLMOD = RGB565
    (0x2A, &[0x00, 0x22, 0x00, 0xCD], 0),                                      // CASET: x=34..205
    (0x2B, &[0x00, 0x00, 0x01, 0x3F], 0),                                      // RASET: y=0..319
    (0xDE, &[0x02], 0),
    (0xE5, &[0x00, 0x02, 0x00], 0),
    (0xDE, &[0x00], 0),
    (0x29, &[], 20),                                                           // DISPON (+ 20ms stabilize)
    // INVON frequentemente necessário em LCDs TFT modernos — a polaridade
    // do substrato LCD deste painel exige inversão para cores corretas.
    // Se a tela aparecer com cores "trocadas" (vermelho → ciano, etc.),
    // trocar por 0x20 (INVOFF).
    (0x21, &[], 0),                                                            // INVON
];

/// Envia cmd+data numa ÚNICA transação com CS continuamente low.
/// Esse é o comportamento do `esp_lcd_panel_io_spi` do ESP-IDF: CS só vai
/// high quando cmd e data terminaram. Toggle de CS entre cmd e data quebra
/// o protocolo do JD9853 — o chip descarta o comando "incompleto".
fn write_cmd<S: SpiBus>(spi: &mut S, cs: &mut Output, dc: &mut Output, cmd: u8, data: &[u8]) {
    cs.set_low();

    // Command phase (DC=low)
    dc.set_low();
    let _ = spi.write(&[cmd]);
    let _ = spi.flush();

    // Data phase (DC=high) — só se houver dados
    if !data.is_empty() {
        dc.set_high();
        let _ = spi.write(data);
        let _ = spi.flush();
    }

    cs.set_high();
}

/// Envia apenas dados adicionais (CS deve já estar low, em transação ativa).
/// Usado no fill: um write_cmd(0x2C, &[]) abre a transação RAMWR, depois
/// vários write_data_only() enviam os pixels, e fechamos CS manualmente.
fn write_data_only<S: SpiBus>(spi: &mut S, dc: &mut Output, data: &[u8]) {
    dc.set_high();
    let _ = spi.write(data);
    let _ = spi.flush();
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    info!("=== display_test boot ===");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 32 * 1024);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
    info!("esp-hal + esp-rtos: OK");

    // Pinos de controle (CS idle high, DC inicialmente low, BL OFF durante init).
    let out_cfg = OutputConfig::default();
    let mut cs = Output::new(peripherals.GPIO21, Level::High, out_cfg);
    let mut dc = Output::new(peripherals.GPIO45, Level::Low, out_cfg);
    let mut rst = Output::new(peripherals.GPIO40, Level::High, out_cfg);
    let mut bl = Output::new(peripherals.GPIO46, Level::Low, out_cfg);

    // SPI2 a 1 MHz pra desbugar — ultra lento pra eliminar qualquer timing
    // issue. Se funcionar a 1 MHz mas não a 80 MHz, sabemos que é clock.
    // Depois de confirmar, voltar pra 80 MHz ou usar DMA.
    let spi_config = SpiConfig::default()
        .with_frequency(Rate::from_mhz(1))
        .with_mode(Mode::_0);
    let mut spi = Spi::new(peripherals.SPI2, spi_config)
        .expect("SPI init falhou")
        .with_sck(peripherals.GPIO38)
        .with_mosi(peripherals.GPIO39);
    info!("SPI2 + GPIOs: OK");

    // Hardware reset — pulso de 20ms low, 150ms de estabilização.
    rst.set_low();
    Timer::after(Duration::from_millis(20)).await;
    rst.set_high();
    Timer::after(Duration::from_millis(150)).await;
    info!("RST: OK");

    // Init sequence (cmd+data na mesma transação — CS continuamente low).
    info!("init sequence ({} comandos)...", INIT_SEQ.len());
    for &(cmd, data, delay_ms) in INIT_SEQ {
        write_cmd(&mut spi, &mut cs, &mut dc, cmd, data);
        if delay_ms > 0 {
            Timer::after(Duration::from_millis(delay_ms as u64)).await;
        }
    }
    info!("init sequence: OK");

    // Backlight ON só depois do init (evita flash branco).
    bl.set_high();
    info!("BL: ON");

    // Diagnóstico antes do fill: comandos MIPI-padrão que não dependem de
    // RAMWR. Se qualquer um deles afetar a tela, sabemos que o init chegou
    // e o problema é só no fill (CASET/RASET/RAMWR via SPI bulk).
    info!("diagnóstico: ALLPON (tela deve ficar BRANCA por 3s)");
    write_cmd(&mut spi, &mut cs, &mut dc, 0x23, &[]);
    Timer::after(Duration::from_secs(3)).await;

    info!("diagnóstico: ALLPOFF (tela volta ao RAM — deve ficar PRETA 3s)");
    write_cmd(&mut spi, &mut cs, &mut dc, 0x22, &[]);
    Timer::after(Duration::from_secs(3)).await;

    // Set window pra área visível inteira — cada comando com seus dados
    // numa única transação (CS low durante cmd+data).
    write_cmd(
        &mut spi,
        &mut cs,
        &mut dc,
        0x2A,
        &[
            (LCD_X_OFFSET >> 8) as u8,
            LCD_X_OFFSET as u8,
            ((LCD_X_OFFSET + LCD_W - 1) >> 8) as u8,
            (LCD_X_OFFSET + LCD_W - 1) as u8,
        ],
    );
    write_cmd(
        &mut spi,
        &mut cs,
        &mut dc,
        0x2B,
        &[0x00, 0x00, ((LCD_H - 1) >> 8) as u8, (LCD_H - 1) as u8],
    );

    // Fill com vermelho dentro de uma ÚNICA transação RAMWR: abre CS,
    // envia cmd 0x2C (DC=low), depois vários chunks de pixels (DC=high),
    // fecha CS no final. Exatamente como o ESP-IDF faz internamente.
    let mut chunk = [0u8; 64];
    for i in 0..32 {
        chunk[i * 2] = RED_HI;
        chunk[i * 2 + 1] = RED_LO;
    }

    cs.set_low();
    // Command phase
    dc.set_low();
    let _ = spi.write(&[0x2C]);
    let _ = spi.flush();
    // Data phase
    let total_pixels = LCD_W as u32 * LCD_H as u32; // 55040
    let pixels_per_chunk = 32u32;
    let full_chunks = total_pixels / pixels_per_chunk; // 1720
    for _ in 0..full_chunks {
        write_data_only(&mut spi, &mut dc, &chunk);
    }
    cs.set_high();
    info!("=== fill VERMELHO concluído — tela deve estar vermelha ===");

    // Loop idle — mantém esp-rtos feliz e permite ver heartbeat via USB-JTAG.
    loop {
        Timer::after(Duration::from_secs(10)).await;
        info!("still alive");
    }
}
