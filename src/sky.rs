use std::sync::OnceLock;

use crate::perlin::fbm;
use crate::renderer::par_rows;
use crate::texture::hex;
use crate::vec3::{Color, Vec3};

pub fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// Direcciones fijas del sol (atardecer, bajo y de lado) y de la luna (tras las nubes)
pub fn sun_dir() -> Vec3 {
    Vec3::new(-0.75, 0.33, 0.55).unit()
}

pub fn moon_dir() -> Vec3 {
    Vec3::new(0.35, 0.75, -0.55).unit()
}

// ---------- Cubemap: 6 caras horneadas una vez y muestreadas con filtro bilineal ----------

pub struct SkyBox {
    res: usize,
    texels: Vec<f32>, // 6 caras * res * res * rgb
}

// Direccion que corresponde a (u,v) en [-1,1] de una cara
fn face_dir(face: usize, u: f64, v: f64) -> Vec3 {
    match face {
        0 => Vec3::new(1.0, u, v),
        1 => Vec3::new(-1.0, u, v),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, v),
        4 => Vec3::new(u, v, 1.0),
        _ => Vec3::new(u, v, -1.0),
    }
    .unit()
}

// Cara y (u,v) en [-1,1] hacia donde apunta una direccion
fn dir_face(d: Vec3) -> (usize, f64, f64) {
    let (ax, ay, az) = (d.x.abs(), d.y.abs(), d.z.abs());
    if ax >= ay && ax >= az {
        (if d.x > 0.0 { 0 } else { 1 }, d.y / ax, d.z / ax)
    } else if ay >= az {
        (if d.y > 0.0 { 2 } else { 3 }, d.x / ay, d.z / ay)
    } else {
        (if d.z > 0.0 { 4 } else { 5 }, d.x / az, d.y / az)
    }
}

impl SkyBox {
    // Hornea las 6 caras evaluando `f` en el centro de cada texel (en paralelo)
    fn bake(res: usize, f: impl Fn(Vec3) -> Color + Sync) -> Self {
        let mut texels = vec![0f32; 6 * res * res * 3];
        par_rows(&mut texels, res * 3, |row, out| {
            let (face, y) = (row / res, row % res);
            for x in 0..res {
                let u = (x as f64 + 0.5) / res as f64 * 2.0 - 1.0;
                let v = (y as f64 + 0.5) / res as f64 * 2.0 - 1.0;
                let c = f(face_dir(face, u, v));
                out[x * 3..x * 3 + 3].copy_from_slice(&[c.x as f32, c.y as f32, c.z as f32]);
            }
        });
        SkyBox { res, texels }
    }

    pub fn sample(&self, d: Vec3) -> Color {
        let (face, u, v) = dir_face(d);
        let r = self.res;
        let s = ((u + 1.0) * 0.5 * r as f64 - 0.5).clamp(0.0, (r - 1) as f64);
        let t = ((v + 1.0) * 0.5 * r as f64 - 0.5).clamp(0.0, (r - 1) as f64);
        let (x0, y0) = (s as usize, t as usize);
        let (x1, y1) = ((x0 + 1).min(r - 1), (y0 + 1).min(r - 1));
        let (fx, fy) = (s - x0 as f64, t - y0 as f64);
        let base = face * r * r;
        let px = |x: usize, y: usize| {
            let i = (base + y * r + x) * 3;
            Color::new(self.texels[i] as f64, self.texels[i + 1] as f64, self.texels[i + 2] as f64)
        };
        px(x0, y0).lerp(px(x1, y0), fx).lerp(px(x0, y1).lerp(px(x1, y1), fx), fy)
    }
}

// Siluetas de arboles lejanos (bosque japones) en el horizonte, segun el azimut
fn treeline(az: f64) -> f64 {
    0.03 + 0.05 * (fbm(Vec3::new(az * 2.5, 0.0, 1.7), 3) * 0.5 + 0.5) + 0.018 * (az * 23.0).sin().abs() + 0.01 * (az * 57.0).sin().abs()
}

// Nubes proyectadas sobre un "techo" plano: densidad en [0,1]
fn clouds(d: Vec3, scale: f64, cover: f64) -> f64 {
    let k = scale / (d.y + 0.12);
    let n = fbm(Vec3::new(d.x * k, 0.37, d.z * k), 5) * 0.5 + 0.5;
    smoothstep(cover, cover + 0.3, n)
}

// Cielo de atardecer estilo Ghibli: degradado naranja-rosa-azul, sol con halo, cumulos iluminados y bosque
fn sunset_sky(d: Vec3) -> Color {
    let el = d.y.clamp(-1.0, 1.0).asin();
    let az = d.z.atan2(d.x);
    let sun = sun_dir();
    let mut c = if el > 0.0 {
        let t = (el / 1.2).powf(0.55);
        if t < 0.3 { hex(0xF8E2B4).lerp(hex(0x9CCBEA), t / 0.3) } else { hex(0x9CCBEA).lerp(hex(0x2E6CC0), (t - 0.3) / 0.7) }
    } else {
        hex(0xF8E2B4).lerp(hex(0xB8D4C8), (-el / 0.5).min(1.0))
    };
    let g = d.dot(sun).max(0.0);
    c += hex(0xFFC080) * (g.powi(64) * 1.5) + hex(0xFF9050) * (g.powi(6) * 0.35);
    if g > 0.9994 {
        return hex(0xFFF2D0) * 6.0;
    }
    if el > 0.0 {
        let dens = clouds(d, 1.6, 0.48) * smoothstep(0.0, 0.08, el);
        let facing = 0.5 + 0.5 * Vec3::new(d.x, 0.0, d.z).unit().dot(Vec3::new(sun.x, 0.0, sun.z).unit());
        let cloud = hex(0x9AAECC).lerp(hex(0xFFF4E0), (0.3 + 0.7 * facing).powf(1.2)) * (1.0 + 0.5 * g.powi(8));
        c = c.lerp(cloud, dens * 0.95);
    }
    if el < treeline(az) && el > -0.35 {
        let haze = (1.0 - (el + 0.35) / 0.4).clamp(0.0, 1.0);
        return hex(0x2E5A40).lerp(hex(0xB8D4C8), 0.25 + haze * 0.5);
    }
    c
}

// Cielo de noche lluviosa: nubes densas y oscuras, la luna apenas brilla detras, bosque negro
fn rainy_night_sky(d: Vec3) -> Color {
    let el = d.y.clamp(-1.0, 1.0).asin();
    let az = d.z.atan2(d.x);
    let mut c = if el > 0.0 { hex(0x34405E).lerp(hex(0x121828), (el / 1.2).powf(0.6)) } else { hex(0x34405E).lerp(hex(0x1A2030), (-el / 0.5).min(1.0)) };
    let glow = d.dot(moon_dir()).max(0.0);
    c += hex(0x6070A0) * (glow.powi(40) * 0.35);
    if el > 0.0 {
        let n = clouds(d, 1.1, 0.2);
        c = c.lerp(hex(0x3E4862).lerp(hex(0x1C2436), n), 0.8 * smoothstep(0.0, 0.1, el));
    }
    if el < treeline(az) && el > -0.35 {
        return hex(0x0C1018).lerp(hex(0x1A2030), (1.0 - (el + 0.35) / 0.4).clamp(0.0, 1.0) * 0.5);
    }
    c
}

// Los dos cielos se hornean la primera vez que se piden
fn skies() -> &'static [SkyBox; 2] {
    static SKIES: OnceLock<[SkyBox; 2]> = OnceLock::new();
    SKIES.get_or_init(|| [SkyBox::bake(256, sunset_sky), SkyBox::bake(256, rainy_night_sky)])
}

// ---------- Estado del cielo en un instante ----------

// `station`: 0 = atardecer, 1 = noche lluviosa; `flash` = relampago en curso
#[derive(Clone, Copy)]
pub struct SkyParams {
    pub station: f64,
    pub time: f64,
    pub flash: f64,
}

impl SkyParams {
    pub fn new(station: f64, time: f64, flash: f64) -> Self {
        skies(); // se asegura de hornear antes de renderizar
        SkyParams { station: station.clamp(0.0, 1.0), time, flash }
    }

    pub fn rain(&self) -> f64 {
        smoothstep(0.4, 1.0, self.station)
    }

    // Luces direccionales: sol del atardecer y luz fria de la noche (mas el relampago)
    pub fn lights(&self) -> [(Vec3, Color, f64); 2] {
        let s = self.station;
        [
            (sun_dir(), Color::new(1.0, 0.86, 0.66), 1.35 * (1.0 - s)),
            (moon_dir(), Color::new(0.55, 0.62, 0.9).lerp(Color::new(0.85, 0.9, 1.0), self.flash), 0.1 * s + 1.8 * self.flash),
        ]
    }

    // Luz ambiente (cielo arriba, rebote abajo)
    pub fn ambient(&self) -> (Color, Color) {
        let s = self.station;
        let flash = Color::new(0.6, 0.65, 0.8) * self.flash;
        // Sombras frescas azul-verdosas como en los fondos de Ghibli; rebote verde del pasto
        (Color::new(0.5, 0.72, 0.85).lerp(Color::new(0.12, 0.15, 0.24), s) + flash, Color::new(0.4, 0.5, 0.25).lerp(Color::new(0.03, 0.035, 0.05), s) + flash * 0.5)
    }

    // Color del horizonte, usado por la niebla
    pub fn horizon(&self) -> Color {
        hex(0xC8DCD8).lerp(hex(0x1C2436), self.station) + Color::new(0.3, 0.32, 0.4) * self.flash
    }

    pub fn fog_density(&self) -> f64 {
        0.006 + 0.03 * self.station
    }
}

// Color del skybox en una direccion: mezcla de los dos cubemaps, iluminado por el relampago
pub fn sky_color(dir: Vec3, sky: &SkyParams) -> Color {
    let [sunset, night] = skies();
    let d = dir.unit();
    if d.length_squared() < 0.5 {
        return Color::ZERO; // direccion degenerada
    }
    let c = if sky.station <= 0.0 {
        sunset.sample(d)
    } else if sky.station >= 1.0 {
        night.sample(d)
    } else {
        sunset.sample(d).lerp(night.sample(d), sky.station)
    };
    c + Color::new(0.5, 0.55, 0.75) * (sky.flash * 1.5)
}

