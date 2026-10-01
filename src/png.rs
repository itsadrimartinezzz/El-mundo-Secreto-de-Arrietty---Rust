use std::fs::File;
use std::io::{self, Write};

/// Encoder de PNG minimo, escrito a mano con solo la libreria estandar:
/// arma el stream zlib usando bloques "stored" (sin compresion real, que
/// es perfectamente valido segun la especificacion DEFLATE) y calcula
/// Adler32/CRC32 manualmente. No se usa ninguna crate de compresion ni de
/// imagenes.
pub fn write_png(path: &str, width: u32, height: u32, pixels: &[u8]) -> io::Result<()> {
    assert_eq!(pixels.len(), (width as usize) * (height as usize) * 3);

    let mut raw = Vec::with_capacity(pixels.len() + height as usize);
    let stride = (width * 3) as usize;
    for y in 0..height as usize {
        raw.push(0u8); // filtro "None" para cada scanline
        raw.extend_from_slice(&pixels[y * stride..y * stride + stride]);
    }

    let zlib = deflate_stored(&raw);

    let mut file = File::create(path)?;
    file.write_all(&[137, 80, 78, 71, 13, 10, 26, 10])?; // firma PNG

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits, color RGB, sin interlace
    write_chunk(&mut file, b"IHDR", &ihdr)?;

    write_chunk(&mut file, b"IDAT", &zlib)?;
    write_chunk(&mut file, b"IEND", &[])?;

    Ok(())
}

fn write_chunk(file: &mut File, kind: &[u8; 4], data: &[u8]) -> io::Result<()> {
    file.write_all(&(data.len() as u32).to_be_bytes())?;
    file.write_all(kind)?;
    file.write_all(data)?;
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    file.write_all(&crc32(&crc_input).to_be_bytes())?;
    Ok(())
}

/// Envuelve `data` en un stream zlib valido usando unicamente bloques
/// DEFLATE "stored" (BTYPE=00), es decir sin comprimir.
fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 11);
    out.push(0x78);
    out.push(0x01); // header zlib valido (CMF/FLG), sin diccionario

    const MAX_BLOCK: usize = 65535;
    let mut offset = 0;
    if data.is_empty() {
        out.push(1); // un bloque final vacio
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    }
    while offset < data.len() {
        let end = (offset + MAX_BLOCK).min(data.len());
        let is_final = end == data.len();
        out.push(if is_final { 1 } else { 0 });
        let len = (end - offset) as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(&data[offset..end]);
        offset = end;
    }

    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB88320 & mask);
        }
    }
    !crc
}
