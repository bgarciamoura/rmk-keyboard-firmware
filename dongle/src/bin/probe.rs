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

use bt_hci::controller::ExternalController;

use embedded_storage::nor_flash::NorFlash;

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::rng::{Trng, TrngSource};
use esp_hal::timer::timg::TimerGroup;

use esp_radio::ble::controller::BleConnector;

use esp_storage::FlashStorage;

use log::info;

// Offset da região de storage na partição `factory` — o app ocupa até
// ~0x44000 dentro dela, então 0x3f0000 é espaço vazio seguro para smoke test.
// É perto de onde o RMK aponta seu storage por padrão.
const STORAGE_OFFSET: u32 = 0x3f_0000;
const SECTOR_SIZE: u32 = 0x1000;

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

    // Passo 9 — SUSPECT #1 (confirmado OK na V1 do probe). BleConnector::new
    // traz o radio BT up via esp-rtos.
    info!("passo 9: BleConnector::new(BT, default)");
    let connector = BleConnector::new(peripherals.BT, Default::default())
        .expect("BleConnector::new falhou");
    info!("passo 9: OK — radio BT up");

    // Passo 10 — ExternalController<_, 20> envolve o connector num wrapper
    // bt-hci. N=20 é o tamanho da queue de comandos (igual ao exemplo RMK).
    info!("passo 10: ExternalController::new(connector) [bt-hci wrapper, N=20]");
    let _controller: ExternalController<_, 20> = ExternalController::new(connector);
    info!("passo 10: OK");

    // Passo 11 — FlashStorage::new(FLASH). Wrapper do esp-storage sobre a SPI
    // flash interna. Não faz I/O ainda, só reserva o handle + peripheral.
    info!("passo 11: FlashStorage::new(FLASH)");
    let mut flash = FlashStorage::new(peripherals.FLASH);
    info!(
        "passo 11: OK — capacity={} bytes",
        embedded_storage::nor_flash::ReadNorFlash::capacity(&flash)
    );

    // Passo 12 — Smoke test de leitura. Lê 32 bytes numa região que deveria
    // estar sem uso (offset 0x3f0000). Se bloquear, o problema de storage é
    // no driver esp-storage (hw/patch).
    info!("passo 12: read 32 bytes @ 0x{:08x}", STORAGE_OFFSET);
    let mut read_buf = [0u8; 32];
    match embedded_storage::nor_flash::ReadNorFlash::read(
        &mut flash,
        STORAGE_OFFSET,
        &mut read_buf,
    ) {
        Ok(_) => info!(
            "passo 12: OK — primeiros 8 bytes: {:02x?}",
            &read_buf[..8]
        ),
        Err(_) => info!("passo 12: ERRO no read"),
    }

    // Passo 13 — Erase de 1 setor. Se travar, o caminho de erase do
    // esp-storage tem problema (suspeito forte pro init RMK que faz erase_all
    // em 16 setores).
    info!(
        "passo 13: erase sector [0x{:08x}..0x{:08x})",
        STORAGE_OFFSET,
        STORAGE_OFFSET + SECTOR_SIZE
    );
    match NorFlash::erase(&mut flash, STORAGE_OFFSET, STORAGE_OFFSET + SECTOR_SIZE) {
        Ok(_) => info!("passo 13: OK"),
        Err(_) => info!("passo 13: ERRO no erase"),
    }

    // Passo 14 — Write de 32 bytes no setor recém-apagado.
    info!("passo 14: write 32 bytes @ 0x{:08x}", STORAGE_OFFSET);
    let write_buf = [0xAAu8; 32];
    match NorFlash::write(&mut flash, STORAGE_OFFSET, &write_buf) {
        Ok(_) => info!("passo 14: OK"),
        Err(_) => info!("passo 14: ERRO no write"),
    }

    // Passo 15 — Read-back para validar o write.
    info!("passo 15: read-back @ 0x{:08x}", STORAGE_OFFSET);
    let mut verify_buf = [0u8; 32];
    match embedded_storage::nor_flash::ReadNorFlash::read(
        &mut flash,
        STORAGE_OFFSET,
        &mut verify_buf,
    ) {
        Ok(_) if verify_buf == write_buf => info!("passo 15: OK — write verificado"),
        Ok(_) => info!(
            "passo 15: MISMATCH — esperado {:02x?}, leu {:02x?}",
            &write_buf[..8],
            &verify_buf[..8]
        ),
        Err(_) => info!("passo 15: ERRO no read-back"),
    }

    info!("=== passos 1-15 completados (USB HID intencionalmente ignorado) ===");
    info!("=== heartbeat loop ativo ===");

    let mut n: u32 = 0;
    loop {
        Timer::after(Duration::from_secs(1)).await;
        info!("alive: n={}", n);
        n = n.wrapping_add(1);
    }
}
