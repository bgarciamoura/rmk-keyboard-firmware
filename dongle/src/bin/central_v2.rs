//! Dongle F2 — Caminho 2 (workaround pro travamento de `rmk::ble::run_ble`).
//!
//! O central.rs original usa `#[rmk_keyboard]`, que expande o fluxo completo
//! do RMK (`run_rmk` → `run_ble` → join com usb_task). Como o probe V3 provou,
//! `run_ble` trava num `.await` de flash read ANTES de chegar ao `join`, então
//! o USB HID nunca é enumerado.
//!
//! Este binário substitui essa expansão por código manual que reordena os
//! passos: inicializa o rádio BLE e o storage, mas IMEDIATAMENTE sobe o USB
//! HID — sem passar pelo fluxo `run_ble`. Resultado: o dongle enumera como
//! teclado USB (VID 4C4B:4643 "Charybdis 3x6 Wireless") mesmo sem as metades
//! nRF52840 pareadas, dando feedback visual (tecla 'R' a cada 30s).
//!
//! Quando as metades existirem e o pareamento BLE for adicionado, este main
//! vira a base do `central.rs` definitivo (F3 também parte daqui).

#![no_std]
#![no_main]

use core::ptr::addr_of_mut;

use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_time::{Duration, Timer};
use embassy_usb::class::hid::{Config as HidConfig, HidReaderWriter, State};
use embassy_usb::{Builder, Config};

use bt_hci::controller::ExternalController;

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::otg_fs::Usb;
use esp_hal::otg_fs::asynch::{Config as DrvCfg, Driver};
use esp_hal::rng::{Trng, TrngSource};
use esp_hal::timer::timg::TimerGroup;

use esp_radio::ble::controller::BleConnector;

use esp_storage::FlashStorage;

use log::info;
use usbd_hid::descriptor::{KeyboardReport, SerializedDescriptor};

esp_bootloader_esp_idf::esp_app_desc!();

static mut EP_MEMORY: [u8; 1024] = [0; 1024];

// Prova de vida: keycode 0x15 = 'r'. Com modifier LShift (bit 1) fica 'R'.
// Espaço de 30s entre cada — discreto o suficiente pra não atrapalhar uso
// normal do computador, visível o suficiente pra confirmar que o dongle
// está vivo.
const REPORT_PRESS_R: [u8; 8] = [0x02, 0, 0x15, 0, 0, 0, 0, 0];
const REPORT_RELEASE: [u8; 8] = [0; 8];
const HEARTBEAT_INTERVAL_SECS: u64 = 30;

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    esp_println::logger::init_logger_from_env();
    info!("=== central_v2 boot ===");

    // Init esp-hal + heap + embassy timer + esp-rtos scheduler (idêntico ao
    // exemplo esp32s3_ble do RMK).
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    esp_alloc::heap_allocator!(size: 72 * 1024);
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
    info!("esp-hal + esp-rtos + embassy: OK");

    // RNG (necessário para BLE bonding; a gente ainda não faz pareamento,
    // mas init é barato e prepara pro futuro).
    let _trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1);
    let _rng = Trng::try_new().expect("Trng init falhou");
    info!("RNG: OK");

    // BLE controller — fica ativo mas não advertiser. Usado quando as
    // metades pareadas chegarem. Não entra em nenhum .await bloqueante.
    let connector = BleConnector::new(peripherals.BT, Default::default())
        .expect("BleConnector init falhou");
    let _controller: ExternalController<_, 20> = ExternalController::new(connector);
    info!("BLE controller: OK (idle, sem advertise)");

    // Storage handle — reservado para persistir bonding keys quando o
    // pareamento BLE for implementado.
    let _flash = FlashStorage::new(peripherals.FLASH);
    info!("FlashStorage: OK");

    // USB OTG — última coisa antes do loop. A partir daqui o USB-JTAG nativo
    // morre e os `info!` não chegam mais ao host via COM. É esperado.
    info!("Usb::new em 2s — USB-JTAG vai cair agora");
    Timer::after(Duration::from_secs(2)).await;

    let usb = Usb::new(peripherals.USB0, peripherals.GPIO20, peripherals.GPIO19);
    let driver = Driver::new(
        usb,
        unsafe { &mut *addr_of_mut!(EP_MEMORY) },
        DrvCfg::default(),
    );

    // USB device config — valores reais do dongle/keyboard.toml.
    let mut config = Config::new(0x4C4B, 0x4643);
    config.manufacturer = Some("RMK");
    config.product = Some("Charybdis 3x6 Wireless");
    config.serial_number = Some("dongle-v2");
    config.max_power = 100;
    config.max_packet_size_0 = 64;

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

    // Duas tasks em paralelo:
    // - usbd.run() poll contínuo do device stack USB.
    // - heartbeat 'R' a cada 30s.
    // Quando as metades BLE existirem, adicionar aqui uma terceira task que
    // recebe eventos BLE e injeta reports no writer.
    let run_usb = usbd.run();
    let heartbeat = async {
        // Primeira piscada só após enumeração estabilizar.
        Timer::after(Duration::from_secs(3)).await;
        loop {
            let _ = writer.write(&REPORT_PRESS_R).await;
            Timer::after(Duration::from_millis(30)).await;
            let _ = writer.write(&REPORT_RELEASE).await;
            Timer::after(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;
        }
    };

    join(run_usb, heartbeat).await;
}
