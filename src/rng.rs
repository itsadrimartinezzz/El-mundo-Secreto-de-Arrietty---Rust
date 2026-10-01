use crate::vec3::Vec3;

/// Generador pseudoaleatorio xorshift64*, escrito a mano para no depender
/// de la crate `rand`. Cada hilo de render tiene su propia instancia
/// sembrada con una semilla distinta.
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        // Evita el estado cero, que es un punto fijo de xorshift.
        Rng {
            state: seed ^ 0x9E3779B97F4A7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// f64 uniforme en [0, 1).
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    pub fn range(&mut self, min: f64, max: f64) -> f64 {
        min + (max - min) * self.f64()
    }

    #[allow(dead_code)]
    pub fn vec3(&mut self) -> Vec3 {
        Vec3::new(self.f64(), self.f64(), self.f64())
    }

    pub fn vec3_range(&mut self, min: f64, max: f64) -> Vec3 {
        Vec3::new(
            self.range(min, max),
            self.range(min, max),
            self.range(min, max),
        )
    }

    pub fn in_unit_sphere(&mut self) -> Vec3 {
        loop {
            let p = self.vec3_range(-1.0, 1.0);
            if p.length_squared() < 1.0 {
                return p;
            }
        }
    }

    #[allow(dead_code)]
    pub fn unit_vector(&mut self) -> Vec3 {
        self.in_unit_sphere().unit()
    }

    pub fn in_unit_disk(&mut self) -> Vec3 {
        loop {
            let p = Vec3::new(self.range(-1.0, 1.0), self.range(-1.0, 1.0), 0.0);
            if p.length_squared() < 1.0 {
                return p;
            }
        }
    }
}
