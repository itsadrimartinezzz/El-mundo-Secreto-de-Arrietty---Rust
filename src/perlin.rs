use crate::rng::Rng;
use crate::vec3::Vec3;

const POINT_COUNT: usize = 256;

/// Ruido de Perlin clasico (gradiente + interpolacion hermite), implementado
/// desde cero. Se usa para todas las texturas procedurales de la escena
/// (pasto, agua, madera, motas del hongo) ya que no se permiten librerias
/// externas para cargar imagenes.
pub struct Perlin {
    ranvec: Vec<Vec3>,
    perm_x: Vec<i32>,
    perm_y: Vec<i32>,
    perm_z: Vec<i32>,
}

impl Perlin {
    pub fn new(rng: &mut Rng) -> Self {
        let ranvec = (0..POINT_COUNT)
            .map(|_| rng.vec3_range(-1.0, 1.0).unit())
            .collect();
        Perlin {
            ranvec,
            perm_x: Self::generate_perm(rng),
            perm_y: Self::generate_perm(rng),
            perm_z: Self::generate_perm(rng),
        }
    }

    fn generate_perm(rng: &mut Rng) -> Vec<i32> {
        let mut p: Vec<i32> = (0..POINT_COUNT as i32).collect();
        for i in (1..p.len()).rev() {
            let target = (rng.f64() * (i + 1) as f64) as usize;
            p.swap(i, target);
        }
        p
    }

    // Ruido de gradiente: mismo resultado que la version trilineal clasica, pero interpolando directo
    pub fn noise(&self, p: Vec3) -> f64 {
        let (fx, fy, fz) = (p.x.floor(), p.y.floor(), p.z.floor());
        let (u, v, w) = (p.x - fx, p.y - fy, p.z - fz);
        let (i, j, k) = (fx as i32, fy as i32, fz as i32);
        let px = [self.perm_x[(i & 255) as usize], self.perm_x[((i + 1) & 255) as usize]];
        let py = [self.perm_y[(j & 255) as usize], self.perm_y[((j + 1) & 255) as usize]];
        let pz = [self.perm_z[(k & 255) as usize], self.perm_z[((k + 1) & 255) as usize]];
        let g = |a: usize, b: usize, c: usize| {
            let r = self.ranvec[(px[a] ^ py[b] ^ pz[c]) as usize];
            r.x * (u - a as f64) + r.y * (v - b as f64) + r.z * (w - c as f64)
        };
        let s = |t: f64| t * t * (3.0 - 2.0 * t);
        let (uu, vv, ww) = (s(u), s(v), s(w));
        let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
        let x00 = lerp(g(0, 0, 0), g(1, 0, 0), uu);
        let x10 = lerp(g(0, 1, 0), g(1, 1, 0), uu);
        let x01 = lerp(g(0, 0, 1), g(1, 0, 1), uu);
        let x11 = lerp(g(0, 1, 1), g(1, 1, 1), uu);
        lerp(lerp(x00, x10, vv), lerp(x01, x11, vv), ww)
    }

    /// Ruido turbulento (suma de octavas), util para vetas de madera / olas.
    pub fn turbulence(&self, p: Vec3, depth: i32) -> f64 {
        let mut accum = 0.0;
        let mut temp_p = p;
        let mut weight = 1.0;
        for _ in 0..depth {
            accum += weight * self.noise(temp_p);
            weight *= 0.5;
            temp_p = temp_p * 2.0;
        }
        accum.abs()
    }
}

// Ruido global compartido por todas las texturas procedurales (semilla fija)
pub fn global() -> &'static Perlin {
    static NOISE: std::sync::OnceLock<Perlin> = std::sync::OnceLock::new();
    NOISE.get_or_init(|| Perlin::new(&mut Rng::new(0xA221_E77E)))
}

// Ruido fractal (suma de octavas) en [-1, 1] aproximadamente
pub fn fbm(p: Vec3, octaves: u32) -> f64 {
    let n = global();
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for _ in 0..octaves {
        sum += n.noise(p * freq) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.03;
    }
    sum / norm
}

