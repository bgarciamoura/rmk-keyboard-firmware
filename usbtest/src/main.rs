//! Diagnóstico USB HID mínimo — ESP32-S3 OTG FS + embassy-usb.
//!
//! Objetivo: confirmar que o pipeline F2 (compilação → flash → execução no
//! dongle) produz um USB HID keyboard visível no host, isolando RMK/BLE/split
//! do caminho. Sucesso = dispositivo aparece no host com VID 0x4C4B e
//! digita a tecla 'A' em loop (a cada 3s).
//!
//! Ponto crítico do ESP32-S3: USB OTG (GPIO19/20) e USB-Serial/JTAG nativo
//! compartilham o mesmo par de pinos físicos. Quando Driver::new() liga o OTG,
//! o USB-JTAG morre — e qualquer log via esp-println (que roteia pelo JTAG)
//! some. Por isso todo log interessante ocorre ANTES do Driver::new(). Os
//! logs depois desse ponto só chegariam via UART0 externa (não conectada).

#![no_std]
#![no_main]

use core::ptr::addr_of_mut;

use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_time::{Duration, Timer};
use embassy_usb::class::hid::{Config as HidConfig, HidReaderWriter, State};
use embassy_usb::{Builder, Config};

use esp_backtrace as _;
use esp_hal::otg_fs::Usb;
use esp_hal::otg_fs::asynch::{Config as DrvCfg, Driver};
use esp_hal::timer::timg::TimerGroup;

use log::info;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

// Endpoint memory para o driver USB — 1 KiB é suficiente para HID simples.
static mut EP_MEMORY: [u8; 1024] = [0; 1024];

#[esp_hal_embassy::main]
async fn main(_spawner: Spawner) {
    // 1) Init esp-hal core
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(size: 32 * 1024);
    esp_println::logger::init_logger_from_env();

    info!("=== usbtest boot ===");
    info!("step 1/6: esp-hal + heap + logger prontos");

    // 2) Timer para embassy
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);
    info!("step 2/6: embassy timer pronto");

    // 3) Pequeno delay para o host reconhecer a detecção pendente no USB-JTAG
    //    antes de derrubarmos ele trocando pelo OTG.
    Timer::after(Duration::from_millis(500)).await;
    info!("step 3/6: a ponto de inicializar USB OTG — logs via USB-JTAG param agora");

    // 4) USB OTG PHY — substitui o USB-Serial/JTAG no PHY interno do chip.
    //    A partir daqui, esp-println via USB-JTAG não entrega mais nada ao host.
    let usb = Usb::new(peripherals.USB0, peripherals.GPIO20, peripherals.GPIO19);
    let driver = Driver::new(usb, unsafe { &mut *addr_of_mut!(EP_MEMORY) }, DrvCfg::default());

    // 5) Descriptors + builder
    let mut config = Config::new(0x4C4B, 0x0001);
    config.manufacturer = Some("RMK-debug");
    config.product = Some("usbtest HID");
    config.serial_number = Some("0001");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

    // Buffers para o builder (precisam viver até o fim do main)
    let mut config_descriptor = [0u8; 256];
    let mut bos_descriptor = [0u8; 256];
    let mut msos_descriptor = [0u8; 256];
    let mut control_buf = [0u8; 64];
    let mut state = State::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut msos_descriptor,
        &mut control_buf,
    );

    let hid_config = HidConfig {
        report_descriptor: KeyboardReport::desc(),
        request_handler: None,
        poll_ms: 60,
        max_packet_size: 64,
    };
    let hid = HidReaderWriter::<_, 1, 8>::new(&mut builder, &mut state, hid_config);
    let mut usbd = builder.build();
    let (_reader, mut writer) = hid.split();

    // 6) Dois futures em paralelo: (a) run_usb do device stack,
    //    (b) injeção periódica de keypress "A".
    let run_usb = usbd.run();
    let key_loop = async {
        loop {
            Timer::after(Duration::from_secs(3)).await;
            // Report com keycode 0x04 = 'a'; modificador 0.
            let press = KeyboardReport {
                keycodes: [0x04, 0, 0, 0, 0, 0],
                modifier: 0,
                leds: 0,
                reserved: 0,
            };
            let _ = writer.write_serialize(&press).await;
            Timer::after(Duration::from_millis(20)).await;
            let release = KeyboardReport::default();
            let _ = writer.write_serialize(&release).await;
        }
    };

    join(run_usb, key_loop).await;
}
