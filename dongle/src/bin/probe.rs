//! Probe de diagnóstico — reproduz a sequência de init do RMK `esp32s3_ble`
//! exemplo, exceto que NÃO chama `Usb::new()`. O ponto é identificar em qual
//! passo o F1 trava antes de chegar ao USB HID.
//!
//! Estratégia:
//! - USB-JTAG nativo fica vivo o tempo todo (porque nunca ligamos o OTG) →
//!   logs via `esp-println` continuam chegando ao host via VID 303A:1001.
//! - `info!` denso entre cada passo. Se algum trava, o último log imprime
//!   aponta o culpado.
//! - Se todos os 9 passos (até `BleConnector::new` inclusive) completam,
//!   entramos num loop heartbeat que imprime `alive: N` a cada segundo.
//!   Isso valida passos 1-9 e foca o problema em passos 10-17 (storage,
//!   run_ble, ou o próprio Usb::new()).
//!
//! Fluxo reproduzido (ordem idêntica ao
//! `rmk/examples/use_rust/esp32s3_ble/src/main.rs`):
//!   1. esp_println::logger::init_logger_from_env
//!   2. esp_hal::init com CpuClock::max
//!   3. esp_alloc::heap_allocator!(72 KiB)
//!   4. TimerGroup::new(TIMG0)
//!   5. SoftwareInterruptControl::new
//!   6. esp_rtos::start(timer, sw_int)  <-- aqui Embassy sobre RTOS sobe
//!   7. TrngSource::new(RNG, ADC1)
//!   8. Trng::try_new().unwrap()
//!   9. BleConnector::new(BT, Default::default()).unwrap()  <-- SUSPECT #1

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rng::{Trng, TrngSource};
use esp_hal::timer::timg::TimerGroup;

use esp_radio::ble::controller::BleConnector;

use log::info;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    // Passo 1 — logger sobre USB-JTAG (VID 303A:1001 continua vivo porque
    // nunca chamamos Usb::new(). Todos os info! abaixo chegam ao host.)
    esp_println::logger::init_logger_from_env();
    info!("=== probe boot — diagnóstico RMK init sem USB OTG ===");

    // Passo 2 — esp_hal::init com CPU clock no máximo (idêntico ao exemplo RMK).
    info!("passo 2/9: esp_hal::init (CpuClock::max)");
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    info!("passo 2/9: OK");

    // Passo 3 — heap 72 KiB (mesmo tamanho do exemplo esp32s3_ble; RMK aloca
    // estruturas grandes na init do BLE host stack).
    info!("passo 3/9: heap_allocator (72 KiB)");
    esp_alloc::heap_allocator!(size: 72 * 1024);
    info!("passo 3/9: OK");

    // Passo 4 — TimerGroup0, usado pelo esp-rtos como fonte de tempo do embassy.
    info!("passo 4/9: TimerGroup::new(TIMG0)");
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    info!("passo 4/9: OK");

    // Passo 5 — Software interrupts. O esp-rtos usa SW_INTERRUPT0 para IPC
    // entre core e scheduler.
    info!("passo 5/9: SoftwareInterruptControl::new");
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    info!("passo 5/9: OK");

    // Passo 6 — esp_rtos::start. A partir daqui Embassy está rodando sobre o
    // scheduler do esp-rtos. Se o patch.crates-io (rev 20ed2bc3) não casar
    // com os features, pode travar aqui.
    info!("passo 6/9: esp_rtos::start (embassy on top of RTOS)");
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);
    info!("passo 6/9: OK");

    // Passo 7 — TrngSource: RMK usa para gerar chaves de pareamento BLE.
    info!("passo 7/9: TrngSource::new(RNG, ADC1)");
    let _trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1);
    info!("passo 7/9: OK");

    // Passo 8 — Trng handle.
    info!("passo 8/9: Trng::try_new()");
    let _rng = Trng::try_new().expect("Trng::try_new falhou");
    info!("passo 8/9: OK");

    // Passo 9 — SUSPECT #1. BleConnector::new traz o radio BT up via esp-rtos.
    // Se o esp-rtos não subiu completo no passo 6, isto trava SEM timeout,
    // SEM await, SEM panic visível — o executor simplesmente para.
    info!("passo 9/9: BleConnector::new(BT, default) <=== SUSPECT #1");
    let _connector = BleConnector::new(peripherals.BT, Default::default())
        .expect("BleConnector::new falhou");
    info!("passo 9/9: OK — radio BT up");

    info!("=== todos os 9 passos de init completados ===");
    info!("=== entrando em loop heartbeat; sem USB, sem storage ===");

    // Heartbeat — apenas imprime um contador a cada segundo. Se você vê isto
    // no monitor, passos 1-9 estão todos OK e o problema do F1 está em:
    // (a) Usb::new()/Driver::new() panica, (b) storage init trava,
    // (c) run_ble host stack trava.
    let mut n: u32 = 0;
    loop {
        Timer::after(Duration::from_secs(1)).await;
        info!("alive: n={}", n);
        n = n.wrapping_add(1);
    }
}
