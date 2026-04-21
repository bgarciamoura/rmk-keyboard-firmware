//! Frames do Bongo Cat (64×64 @ 1 bpp, layout Adafruit_GFX).
//!
//! Sprites derivados de https://github.com/sidharth-458/Bongo_cat-32
//! (autor original dos sprites: @pixelbunny — ver NOTICE.md na raiz do repo).
//!
//! Lógica de animação (copiada do sketch original):
//! - Frames 0..8 = idle (cabeça balançando, loop suave)
//! - Frame 10 = pose base com as duas patas levantadas (transição pro tap)
//! - Frame 11 = pata esquerda batendo
//! - Frame 12 = pata direita batendo
//! - Frame 9  = ambas as patas batendo juntas

pub const FRAME_W: u16 = 64;
pub const FRAME_H: u16 = 64;
pub const FRAME_BYTES: usize = (FRAME_W as usize * FRAME_H as usize) / 8;

pub static FRAME_00: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_00.bin");
pub static FRAME_01: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_01.bin");
pub static FRAME_02: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_02.bin");
pub static FRAME_03: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_03.bin");
pub static FRAME_04: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_04.bin");
pub static FRAME_05: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_05.bin");
pub static FRAME_06: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_06.bin");
pub static FRAME_07: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_07.bin");
pub static FRAME_08: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_08.bin");
pub static FRAME_09: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_09.bin");
pub static FRAME_10: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_10.bin");
pub static FRAME_11: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_11.bin");
pub static FRAME_12: &[u8; FRAME_BYTES] = include_bytes!("bongo/frame_12.bin");

/// Índices lógicos dos frames na animação.
pub const IDLE_FRAMES: &[&[u8; FRAME_BYTES]] = &[
    FRAME_00, FRAME_01, FRAME_02, FRAME_03, FRAME_04, FRAME_05, FRAME_06, FRAME_07, FRAME_08,
];

/// Frame de transição (patas levantadas — comum a todos os taps).
pub const TAP_BASE: &[u8; FRAME_BYTES] = FRAME_10;
pub const TAP_LEFT: &[u8; FRAME_BYTES] = FRAME_11;
pub const TAP_RIGHT: &[u8; FRAME_BYTES] = FRAME_12;
pub const TAP_BOTH: &[u8; FRAME_BYTES] = FRAME_09;
