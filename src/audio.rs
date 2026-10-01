// Efectos de sonido sintetizados (sin archivos): lluvia, truenos y pajaritos.
// Se generan como WAV PCM de 16 bits mono para que Windows los reproduzca junto a la musica.

use crate::rng::Rng;

pub const RATE: u32 = 22_050;

// Archivo WAV completo (cabecera RIFF + muestras)
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // tamano del bloque fmt
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes()); // bytes por segundo
    out.extend_from_slice(&2u16.to_le_bytes()); // bytes por muestra
    out.extend_from_slice(&16u16.to_le_bytes()); // bits por muestra
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    out
}

// Escala para que el pico quede en `peak`
fn normalize(s: &mut [f32], peak: f32) {
    let max = s.iter().fold(0f32, |m, v| m.max(v.abs()));
    if max > 0.0 {
        s.iter_mut().for_each(|v| *v *= peak / max);
    }
}

fn noise(rng: &mut Rng) -> f32 {
    rng.f64() as f32 * 2.0 - 1.0
}

// Lluvia en bucle: siseo filtrado + gotas sueltas. Los extremos se funden para que el bucle no se note
pub fn rain(seconds: f32, seed: u64) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    let mut rng = Rng::new(seed);
    let (mut lp, mut hp_prev, mut drop_env, mut drop_lp) = (0f32, 0f32, 0f32, 0f32);
    let mut s: Vec<f32> = (0..n).map(|_| {
        let w = noise(&mut rng);
        lp += (w - lp) * 0.35; // siseo suave
        let hiss = lp - hp_prev * 0.97; // quita el retumbe grave
        hp_prev = lp;
        if rng.f64() < 0.0012 {
            drop_env = 0.4 + rng.f64() as f32 * 0.6; // gota que pega en una hoja
        }
        drop_env *= 0.992;
        drop_lp += (noise(&mut rng) - drop_lp) * 0.6;
        hiss * 0.55 + drop_lp * drop_env * 0.5
    }).collect();
    let fade = (RATE / 2) as usize;
    for i in 0..fade.min(n / 2) {
        let k = i as f32 / fade as f32;
        let j = n - fade + i;
        s[i] = s[i] * k + s[j] * (1.0 - k); // el final se mezcla con el principio
    }
    s.truncate(n - fade);
    normalize(&mut s, 0.6);
    s
}

// Trueno: chasquido inicial y retumbe grave que crece y se apaga en oleadas
pub fn thunder(seed: u64) -> Vec<f32> {
    let n = (5.5 * RATE as f32) as usize;
    let mut rng = Rng::new(seed);
    let swells: Vec<(f32, f32)> = (0..4).map(|i| (0.25 + i as f32 * 0.7 + rng.f64() as f32 * 0.5, 0.5 + rng.f64() as f32 * 0.5)).collect();
    let (mut brown, mut lp1, mut lp2, mut crack_lp) = (0f32, 0f32, 0f32, 0f32);
    let mut s: Vec<f32> = (0..n).map(|i| {
        let t = i as f32 / RATE as f32;
        let w = noise(&mut rng);
        brown = (brown + w * 0.08) * 0.995;
        lp1 += (brown - lp1) * 0.05;
        lp2 += (lp1 - lp2) * 0.05;
        let env: f32 = swells.iter().map(|&(c, a)| a * (-((t - c) / 0.45).powi(2)).exp()).sum::<f32>() * (1.0 - t / 5.5).max(0.0);
        crack_lp += (w - crack_lp) * 0.5;
        let crack = crack_lp * (-t / 0.08).exp() * (t < 0.6) as u8 as f32;
        lp2 * 6.0 * env + crack * 0.7
    }).collect();
    normalize(&mut s, 0.9);
    s
}

// Silaba de canto: barrido de frecuencia con vibrato rapido, ataque suave y cola corta
fn syllable(out: &mut [f32], start: usize, dur: f32, f0: f32, f1: f32, vibrato: (f32, f32), amp: f32) {
    let len = (dur * RATE as f32) as usize;
    let mut phase = 0f32;
    for k in 0..len {
        let Some(o) = out.get_mut(start + k) else { break };
        let x = k as f32 / len as f32;
        let t = k as f32 / RATE as f32;
        // Curva tipo ave: cambia rapido al inicio y se asienta al final
        let glide = 1.0 - (1.0 - x).powi(3);
        let f = f0 + (f1 - f0) * glide + vibrato.1 * (t * vibrato.0 * std::f32::consts::TAU).sin();
        phase += f / RATE as f32 * std::f32::consts::TAU;
        let env = (x * 14.0).min(1.0) * (1.0 - x).powf(1.5);
        // Casi senoidal (los pajaros cantan con tono puro) con un leve segundo armonico
        *o += (phase.sin() + 0.12 * (2.0 * phase).sin()) * env * amp;
    }
}

// Canto de pajarito: frase de silabas variadas (silbidos que bajan, trinos y gorjeos)
// con un poco de eco del jardin y sin los agudos mas duros, como si viniera de un arbol cercano
pub fn birds(seed: u64) -> Vec<f32> {
    let mut rng = Rng::new(seed);
    let mut r = move || rng.f64() as f32;
    let mut s = vec![0f32; (2.8 * RATE as f32) as usize];
    let pitch = 2800.0 + r() * 1200.0; // cada pajaro con su propio tono
    let mut t = 0.05f32;
    let phrases = 2 + (r() * 3.0) as usize;
    for _ in 0..phrases {
        match (r() * 3.0) as usize {
            // Silbido que baja: "tiu"
            0 => {
                let d = 0.10 + r() * 0.08;
                syllable(&mut s, (t * RATE as f32) as usize, d, pitch * 1.45, pitch * 0.85, (0.0, 0.0), 1.0);
                t += d + 0.06 + r() * 0.12;
            }
            // Trino: varias notas cortas y rapidas iguales "tritritri"
            1 => {
                let n = 4 + (r() * 6.0) as usize;
                let f = pitch * (1.0 + r() * 0.3);
                for _ in 0..n {
                    syllable(&mut s, (t * RATE as f32) as usize, 0.035, f * 1.15, f * 0.95, (0.0, 0.0), 0.8);
                    t += 0.055;
                }
                t += 0.08 + r() * 0.1;
            }
            // Gorjeo con vibrato que sube: "huiiit"
            _ => {
                let d = 0.16 + r() * 0.12;
                syllable(&mut s, (t * RATE as f32) as usize, d, pitch * 0.9, pitch * 1.3, (28.0 + r() * 15.0, pitch * 0.06), 0.9);
                t += d + 0.08 + r() * 0.12;
            }
        }
    }
    // Suaviza los agudos (distancia) y agrega ecos cortos del jardin
    let mut lp = 0f32;
    for v in s.iter_mut() {
        lp += (*v - lp) * 0.55;
        *v = lp;
    }
    let dry = s.clone();
    for (delay, gain) in [(0.043f32, 0.22f32), (0.071, 0.15), (0.113, 0.09)] {
        let d = (delay * RATE as f32) as usize;
        for i in d..s.len() {
            s[i] += dry[i - d] * gain;
        }
    }
    normalize(&mut s, 0.45);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_and_effects_are_valid() {
        let w = wav(&[0.0, 1.0, -1.0]);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(w.len(), 44 + 6);
        for s in [rain(2.0, 1), thunder(2), birds(3)] {
            assert!(!s.is_empty());
            assert!(s.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
            assert!(s.iter().any(|v| v.abs() > 0.1), "el efecto no deberia quedar en silencio");
        }
    }
}
