use std::f64::consts::PI;

use crate::perlin::fbm;
use crate::vec3::{Color, Vec3};

// Convierte un color hex sRGB a lineal (gamma 2, el post-procesado aplica sqrt al final)
pub fn hex(c: u32) -> Color {
    let ch = |s: u32| ((c >> s) & 0xFF) as f64 / 255.0;
    Color::new(ch(16).powi(2), ch(8).powi(2), ch(0).powi(2))
}

// Hash 2D entero -> [0,1)
fn hash2(x: i64, y: i64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h >> 11) as f64 / (1u64 << 53) as f64
}

// Ruido celular (Voronoi): distancia al centro mas cercano, al segundo, y un id de la celda
fn voronoi(u: f64, v: f64) -> (f64, f64, f64) {
    let (iu, iv) = (u.floor() as i64, v.floor() as i64);
    let (mut f1, mut f2, mut id) = (9.0f64, 9.0f64, 0.0);
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (cx, cy) = (iu + dx, iv + dy);
            let px = cx as f64 + 0.15 + 0.7 * hash2(cx, cy);
            let py = cy as f64 + 0.15 + 0.7 * hash2(cy * 7 + 3, cx * 13 + 1);
            let d = ((px - u).powi(2) + (py - v).powi(2)).sqrt();
            if d < f1 {
                f2 = f1;
                f1 = d;
                id = hash2(cx * 31 + 5, cy * 17 + 9);
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    (f1, f2, id)
}

fn smooth01(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// Coordenadas (u,v) de la cara de un prisma segun su normal local, con v hacia arriba en las paredes
#[inline]
fn face_uv(p: Vec3, n: Vec3) -> (f64, f64) {
    if n.x.abs() > 0.5 {
        (p.z, p.y)
    } else if n.y.abs() > 0.5 {
        (p.x, p.z)
    } else {
        (p.x, p.y)
    }
}

// Anillos que dejan las gotas de lluvia en el agua: (intensidad, direccion radial) en el punto (x,z)
fn rain_ring(x: f64, z: f64, time: f64) -> (f64, f64, f64) {
    const CELL: f64 = 0.55;
    let (mut best, mut gx, mut gz) = (0.0f64, 0.0, 0.0);
    for layer in 0..2 {
        let off = layer as f64 * 0.37;
        let (cx, cz) = (((x / CELL) + off).floor() as i64, ((z / CELL) + off).floor() as i64);
        let h = hash2(cx + layer * 101, cz);
        let px = (cx as f64 + 0.2 + 0.6 * hash2(cz, cx + 7) - off) * CELL;
        let pz = (cz as f64 + 0.2 + 0.6 * hash2(cx + 3, cz + 11) - off) * CELL;
        let phase = (time * 1.4 + h * 7.0).fract();
        let (dx, dz) = (x - px, z - pz);
        let d = (dx * dx + dz * dz).sqrt();
        let ring = (1.0 - ((d - phase * 0.28).abs() / 0.025)).max(0.0) * (1.0 - phase);
        if ring > best {
            best = ring;
            let inv = 1.0 / d.max(1e-4);
            gx = dx * inv;
            gz = dz * inv;
        }
    }
    (best, gx, gz)
}

// Texturas procedurales del diorama (todas calculadas con matematica, sin archivos de imagen)
#[derive(Clone)]
pub enum Texture {
    Bark,
    Solid(Color),
    Planks { a: Color, b: Color, width: f64 },           // tablones de madera con veta
    Stamps,                                               // papel tapiz de estampillas (como en la casa de Arrietty)
    Siding { base: Color, trim: Color },                  // tablas horizontales de la fachada
    Roof { base: Color, moss: Color },                    // tejas en hileras con musgo
    Stone { base: Color, mortar: Color, scale: f64 },     // piedras irregulares (Voronoi)
    Ground { top: f64 },                                  // pasto arriba y corte de tierra a los lados
    Thread(Color),                                        // hilo enrollado del carrete
    Thimble,                                              // metal con hoyuelos del dedal
    Plaid { a: Color, b: Color },                         // tela a cuadros del panuelo
    Leaf { a: Color, b: Color },                          // hoja con nervaduras (en elipsoide aplanado)
    Fur { base: Color, stripe: Color, belly: Color },     // pelaje atigrado del gato
    Water { time: f64, rain: f64 },                       // agua con ondas y anillos de lluvia
    Matchbox,                                             // caja de fosforos con etiqueta
    Petals { a: Color, b: Color },                        // bolitas de hortensia
    Stucco(Color),                                        // pared de estuco con manchas de humedad
    Foliage { dark: Color, mid: Color, light: Color },    // follaje pintado en 3 tonos (como fondo de anime)
    Blade { base: Color, tip: Color },                    // brizna de pasto: oscura abajo, clara en la punta
    Rock { base: Color, moss: Color },                    // roca gris con musgo encima
    StainedGlass,                                         // vitral de rombos de colores
    Koi(u32),                                             // pez koi: naranja con manchas blancas (semilla)
    Birch,                                                // corteza blanca de abedul con rayas negras
}

impl Texture {
    // `p` punto local, `pw` punto en el mundo, `n` normal local
    pub fn value(&self, p: Vec3, pw: Vec3, n: Vec3) -> Color {
        match self {
            Texture::Bark => {
                let warp = fbm(pw * 1.8, 2);
                let grain = fbm(Vec3::new(pw.x * 22.0 + warp, pw.y * 1.4, pw.z * 22.0), 3) * 0.5 + 0.5;
                let furrow = smooth01((grain - 0.50) * 5.0);
                hex(0x3F352B).lerp(hex(0x82735A), grain).lerp(hex(0x2A261F), furrow * 0.6)
            }
            Texture::Solid(c) => *c,
            Texture::Planks { a, b, width } => {
                let (u, v) = face_uv(p, n);
                let row = (v / width).floor();
                let fv = (v / width).fract();
                // Uniones escalonadas a lo largo del tablon
                let seg_len = 2.3 + hash2(row as i64, 3) * 1.5;
                let fu = ((u + hash2(row as i64, 7) * 3.0) / seg_len).fract();
                let tone = hash2(row as i64, 11);
                let grain = fbm(Vec3::new(u * 0.6, v * 14.0, row), 3);
                let mut c = a.lerp(*b, (tone * 0.6 + grain * 0.5 + 0.2).clamp(0.0, 1.0));
                if fv < 0.04 || fv > 0.97 || fu < 0.008 {
                    c = c * 0.35; // ranura entre tablones
                }
                c
            }
            Texture::Stamps => {
                let (u, v) = face_uv(p, n);
                let (cw, chh) = (0.62, 0.78);
                let (cu, cv) = ((u / cw).floor() as i64, (v / chh).floor() as i64);
                let (fu, fv) = ((u / cw).fract(), (v / chh).fract());
                let paper = hex(0xE9DDC2);
                let edge = 0.1;
                if fu < edge * 0.5 || fu > 1.0 - edge * 0.5 || fv < edge * 0.5 || fv > 1.0 - edge * 0.5 {
                    return paper * (0.9 + 0.1 * fbm(pw * 6.0, 2)); // papel entre estampillas
                }
                // Borde blanco con perforaciones
                let border = fu < edge || fu > 1.0 - edge || fv < edge || fv > 1.0 - edge;
                if border {
                    let perf = ((fu * 18.0).fract() - 0.5).abs() < 0.2 && (fv < edge * 0.8 || fv > 1.0 - edge * 0.8)
                        || ((fv * 22.0).fract() - 0.5).abs() < 0.2 && (fu < edge * 0.8 || fu > 1.0 - edge * 0.8);
                    return if perf { paper * 0.8 } else { hex(0xF6F1E4) };
                }
                const INKS: [u32; 6] = [0xC0503A, 0x3C6E9C, 0x5E8C4A, 0xD19A3A, 0x8A4E7C, 0x2F7A78];
                let ink = hex(INKS[(hash2(cu, cv) * 6.0) as usize % 6]);
                let (x, y) = ((fu - 0.5) / (1.0 - 2.0 * edge), (fv - 0.5) / (1.0 - 2.0 * edge));
                // Motivo: sol, montana o franja segun la estampilla
                let motif = match (hash2(cv, cu) * 3.0) as i32 {
                    0 => (x * x + (y - 0.1) * (y - 0.1)).sqrt() < 0.22,
                    1 => y < 0.1 - x.abs() * 0.9,
                    _ => (y + 0.05).abs() < 0.1,
                };
                if motif { ink * 0.45 } else { ink.lerp(hex(0xF6F1E4), 0.25) }
            }
            Texture::Siding { base, trim } => {
                let v = pw.y / 0.32;
                let f = v.fract();
                let noise = 0.9 + 0.1 * fbm(Vec3::new(pw.x * 3.0, v.floor(), pw.z * 3.0), 2);
                if f < 0.07 { *trim * 0.6 } else { *base * (noise * (0.88 + 0.12 * f)) }
            }
            Texture::Roof { base, moss } => {
                let (u, v) = face_uv(p, n);
                let row = (v / 0.42).floor();
                let fv = (v / 0.42).fract();
                let fu = ((u + row * 0.25) / 0.5).fract();
                let round = 0.72 + 0.28 * (PI * fu).sin();
                let m = (fbm(Vec3::new(u * 0.8, v * 0.8, 3.0), 3) * 0.5 + 0.5 - 0.55).max(0.0) * 3.0;
                let c = base.lerp(*moss, m.min(1.0)) * round * (0.7 + 0.3 * fv);
                if fv < 0.08 { c * 0.4 } else { c }
            }
            Texture::Stone { base, mortar, scale } => {
                let (u, v) = face_uv(p, n);
                let (f1, f2, id) = voronoi(u * scale, v * scale);
                if f2 - f1 < 0.09 {
                    *mortar
                } else {
                    let speck = 0.85 + 0.15 * fbm(pw * 9.0, 2);
                    base.lerp(*base * 1.35, id) * speck * (1.0 - f1 * 0.25)
                }
            }
            Texture::Ground { top } => {
                let grass = |pw: Vec3| {
                    let n = crate::perlin::global();
                    let g = fbm(Vec3::new(pw.x * 0.35, 0.0, pw.z * 0.35), 2) * 0.5 + 0.5 + 0.12 * n.noise(Vec3::new(pw.x * 3.0, 0.0, pw.z * 3.0));
                    let (t1, t2) = (smooth01((g - 0.30) / 0.35), smooth01((g - 0.62) / 0.28));
                    hex(0x344D2B).lerp(hex(0x5B753B), t1).lerp(hex(0x84944E), t2)
                };
                if n.y > 0.5 {
                    grass(pw)
                } else {
                    // Corte de tierra: franja de pasto arriba con borde irregular, capas y piedritas
                    let fringe = 0.18 + 0.1 * fbm(Vec3::new(pw.x * 4.0, 0.0, pw.z * 4.0), 2);
                    if pw.y > top - fringe {
                        return grass(pw) * 0.8;
                    }
                    let layer = fbm(Vec3::new(pw.x * 0.5, pw.y * 3.0, pw.z * 0.5), 3);
                    let mut c = hex(0x5A3E2A).lerp(hex(0x7A5838), layer * 0.5 + 0.5);
                    let (u, v) = face_uv(pw, n);
                    let (f1, _, id) = voronoi(u * 2.5, v * 2.5);
                    if f1 < 0.18 && id > 0.55 {
                        c = hex(0x9A948A) * (0.8 + 0.4 * id);
                    }
                    c
                }
            }
            Texture::Thread(c) => {
                let t = (p.y * 90.0 + p.x.atan2(p.z) * 2.0).sin() * 0.5 + 0.5;
                *c * (0.75 + 0.25 * t)
            }
            Texture::Thimble => {
                let base = hex(0xC8CCD2);
                let ang = p.z.atan2(p.x);
                let (du, dv) = ((ang * 9.0).fract() - 0.5, (p.y * 7.0).fract() - 0.5);
                if n.y.abs() < 0.5 && du * du + dv * dv < 0.08 { base * 0.55 } else { base }
            }
            Texture::Plaid { a, b } => {
                let (u, v) = face_uv(p, n);
                let s1 = ((u * 3.0).fract() < 0.3) as i32 as f64;
                let s2 = ((v * 3.0).fract() < 0.3) as i32 as f64;
                let thin = (((u * 3.0).fract() - 0.65).abs() < 0.03 || ((v * 3.0).fract() - 0.65).abs() < 0.03) as i32 as f64;
                a.lerp(*b, (s1 + s2) * 0.5).lerp(hex(0xF4EEDC), thin * 0.7)
            }
            Texture::Leaf { a, b } => {
                // Largo en x local, ancho en z: nervadura central y laterales
                let mid = (p.z.abs() < 0.05) as i32 as f64;
                let vein = (((p.x * 0.8 + p.z.abs()) * 9.0).sin().abs() < 0.12) as i32 as f64;
                let tone = fbm(p * 3.0, 2) * 0.5 + 0.5;
                let edge = (p.x * p.x + p.z * p.z).sqrt();
                a.lerp(*b, tone).lerp(*b * 1.3, mid * 0.8 + vein * 0.35) * (1.05 - edge * 0.3)
            }
            Texture::Fur { base, stripe, belly } => {
                // Rayas atigradas onduladas + panza clara al frente y abajo
                let wave = fbm(p * 2.2, 3);
                let s = ((p.y * 5.5 + p.x * 1.5 + wave * 1.8) * PI).sin();
                let mut c = if s > 0.45 { *stripe } else { *base };
                if p.z > 0.45 && p.y < 0.35 {
                    c = c.lerp(*belly, ((p.z - 0.45) / 0.3).clamp(0.0, 1.0));
                }
                c * (0.9 + 0.1 * fbm(p * 18.0, 2))
            }
            Texture::Water { time, rain } => {
                let n1 = fbm(Vec3::new(pw.x * 1.5 + time * 0.2, time * 0.15, pw.z * 1.5), 3) * 0.5 + 0.5;
                let mut c = hex(0x1E6A78).lerp(hex(0x5EB8B4), n1);
                if *rain > 0.0 {
                    let (ring, _, _) = rain_ring(pw.x, pw.z, *time);
                    c = c.lerp(hex(0xD8ECF0), ring * rain * 0.8);
                }
                c
            }
            Texture::Matchbox => {
                let (u, v) = face_uv(p, n);
                if n.y > 0.5 {
                    // Tapa: papel con una etiqueta azul y un circulo amarillo
                    let (x, z) = (u - 1.1, v - 0.7);
                    if x.abs() < 0.8 && z.abs() < 0.45 {
                        return if x * x + z * z < 0.08 { hex(0xF2C230) } else { hex(0x2E5A9A) };
                    }
                    hex(0xE8DDC4)
                } else if n.x.abs() > 0.5 {
                    // Rascador marron de los costados
                    hex(0x5C3A26) * (0.8 + 0.2 * fbm(p * 30.0, 2))
                } else {
                    let stripe = ((v * 6.0).floor() as i64 % 2 == 0) as i32 as f64;
                    hex(0xB8322A).lerp(hex(0xD8483A), stripe)
                }
            }
            Texture::Stucco(base) => {
                let n = fbm(pw * 1.3, 2) * 0.5 + 0.5;
                let stain = smooth01(1.2 - pw.y) * 0.25 * n;
                (*base * (0.82 + 0.25 * n)).lerp(hex(0x6A7A4A), stain)
            }
            Texture::Foliage { dark, mid, light } => {
                // Manchas de ruido cuantizadas a 3 tonos, mas claras hacia arriba: look de acuarela/anime
                let n = fbm(pw * 1.7, 2) * 0.5 + 0.5 + n.y * 0.25 + 0.14 * crate::perlin::global().noise(pw * 7.0);
                let (t1, t2) = (smooth01((n - 0.30) / 0.32), smooth01((n - 0.62) / 0.26));
                dark.lerp(*mid, t1).lerp(*light, t2)
            }
            Texture::Blade { base, tip } => base.lerp(*tip, (p.y / 0.9).clamp(0.0, 1.0)),
            Texture::Rock { base, moss } => {
                let pores = smooth01((crate::perlin::global().noise(pw * 65.0) - 0.15) * 5.0);
                let grain = 0.8 + 0.2 * fbm(pw * 2.5, 2) + 0.08 * crate::perlin::global().noise(pw * 12.0) - pores * 0.22;
                let cover = smooth01((n.y - 0.5 + 0.65 * fbm(pw * 4.0, 3)) * 3.0);
                (*base * grain).lerp(*moss * (0.85 + 0.15 * grain), cover)
            }
            Texture::StainedGlass => {
                let (u, v) = face_uv(p, n);
                let (a, b) = ((u + v) * 3.0, (u - v) * 3.0);
                let (fa, fb) = (a.fract(), b.fract());
                if fa < 0.08 || fb < 0.08 {
                    return hex(0x2A2020); // plomo entre vidrios
                }
                const GLASS: [u32; 4] = [0x3A7AB0, 0x5AA05A, 0x8A5AB0, 0xD8B040];
                hex(GLASS[(hash2(a.floor() as i64, b.floor() as i64) * 4.0) as usize % 4])
            }
            Texture::Koi(seed) => {
                let spots = fbm(p * 2.2 + Vec3::new(*seed as f64 * 7.3, 0.0, 0.0), 2);
                let base = if seed % 3 == 0 { hex(0xF4F0E8) } else { hex(0xF07A2A) };
                let other = if seed % 3 == 0 { hex(0xE84A2A) } else { hex(0xF4F0E8) };
                if spots > 0.15 { other } else { base }
            }
            Texture::Birch => {
                let ang = p.z.atan2(p.x);
                let band = (p.y * 5.0).floor() as i64;
                let dash = hash2(band, (ang * 2.0).floor() as i64);
                let fy = (p.y * 5.0).fract();
                if dash > 0.55 && (fy - 0.5).abs() < 0.12 + 0.1 * dash {
                    hex(0x2A2622)
                } else {
                    hex(0xEDE8DC) * (0.85 + 0.15 * fbm(p * 8.0, 2))
                }
            }
            Texture::Petals { a, b } => {
                let t = (p.y * 0.5 + 0.5).clamp(0.0, 1.0);
                let petal = (((p.x.atan2(p.z) * 4.0).sin() * (p.y * 6.0).cos()).abs() < 0.15) as i32 as f64;
                a.lerp(*b, t) * (1.0 - petal * 0.25)
            }
        }
    }

    // Relieve animado: olas y anillos de lluvia inclinan la normal del agua
    pub fn bump(&self, pw: Vec3, n: Vec3) -> Vec3 {
        match self {
            Texture::Water { time, rain } if n.y > 0.5 => {
                let dx = 0.05 * (pw.x * 3.0 + time * 1.7).cos() + 0.03 * (pw.x * 1.3 + pw.z * 2.1 + time * 1.1).cos();
                let dz = 0.05 * (pw.z * 2.6 - time * 1.4).cos() + 0.03 * (pw.x * 1.7 - pw.z * 1.9 + time * 0.9).cos();
                let mut tilt = Vec3::new(dx, 0.0, dz);
                if *rain > 0.0 {
                    let (ring, gx, gz) = rain_ring(pw.x, pw.z, *time);
                    tilt = tilt + Vec3::new(gx, 0.0, gz) * (ring * rain * 0.6);
                }
                (n - tilt).unit()
            }
            _ => n,
        }
    }
}
