use std::ops::{Add, AddAssign, Div, Index, Mul, MulAssign, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Vec3 { x, y, z }
    }

    pub const ZERO: Vec3 = Vec3::new(0.0, 0.0, 0.0);
    pub const ONE: Vec3 = Vec3::new(1.0, 1.0, 1.0);

    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn length(self) -> f64 {
        self.length_squared().sqrt()
    }

    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn unit(self) -> Vec3 {
        let len = self.length();
        if len == 0.0 {
            Vec3::ZERO
        } else {
            self / len
        }
    }

    pub fn near_zero(self) -> bool {
        const EPS: f64 = 1.0e-8;
        self.x.abs() < EPS && self.y.abs() < EPS && self.z.abs() < EPS
    }

    /// Refleja el vector `self` (rayo incidente) respecto a la normal `n`.
    pub fn reflect(self, n: Vec3) -> Vec3 {
        self - n * (2.0 * self.dot(n))
    }

    /// Refracta el vector `self` (unitario, incidente) segun la ley de Snell.
    /// `n` es la normal (apuntando contra el rayo), `etai_over_etat` es la
    /// razon de indices de refraccion (medio de origen / medio de destino).
    pub fn refract(self, n: Vec3, etai_over_etat: f64) -> Vec3 {
        let cos_theta = (-self).dot(n).min(1.0);
        let r_out_perp = (self + n * cos_theta) * etai_over_etat;
        let r_out_parallel = n * -((1.0 - r_out_perp.length_squared()).abs().sqrt());
        r_out_perp + r_out_parallel
    }

    pub fn component_mul(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x * o.x, self.y * o.y, self.z * o.z)
    }

    pub fn component_div(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x / o.x, self.y / o.y, self.z / o.z)
    }

    pub fn lerp(self, o: Vec3, t: f64) -> Vec3 {
        self * (1.0 - t) + o * t
    }

    pub fn max_component(self) -> f64 {
        self.x.max(self.y).max(self.z)
    }

    pub fn clamp01(self) -> Vec3 {
        Vec3::new(
            self.x.clamp(0.0, 1.0),
            self.y.clamp(0.0, 1.0),
            self.z.clamp(0.0, 1.0),
        )
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        self.x += o.x;
        self.y += o.y;
        self.z += o.z;
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, s: f64) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Mul<Vec3> for f64 {
    type Output = Vec3;
    fn mul(self, v: Vec3) -> Vec3 {
        v * self
    }
}

impl MulAssign<f64> for Vec3 {
    fn mul_assign(&mut self, s: f64) {
        self.x *= s;
        self.y *= s;
        self.z *= s;
    }
}

impl Div<f64> for Vec3 {
    type Output = Vec3;
    fn div(self, s: f64) -> Vec3 {
        self * (1.0 / s)
    }
}

impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        Vec3::new(-self.x, -self.y, -self.z)
    }
}

impl Index<usize> for Vec3 {
    type Output = f64;
    fn index(&self, i: usize) -> &f64 {
        match i {
            0 => &self.x,
            1 => &self.y,
            2 => &self.z,
            _ => panic!("indice fuera de rango para Vec3"),
        }
    }
}

pub type Color = Vec3;
pub type Point3 = Vec3;

// Matriz 3x3 de rotacion (filas), para girar monedas, estrellas y bolas de fuego
#[derive(Debug, Clone, Copy)]
pub struct Mat3 {
    pub r: [Vec3; 3],
}

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3 { r: [Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)] };

    pub fn rot_x(a: f64) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3 { r: [Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, c, -s), Vec3::new(0.0, s, c)] }
    }

    pub fn rot_y(a: f64) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3 { r: [Vec3::new(c, 0.0, s), Vec3::new(0.0, 1.0, 0.0), Vec3::new(-s, 0.0, c)] }
    }

    pub fn rot_z(a: f64) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3 { r: [Vec3::new(c, -s, 0.0), Vec3::new(s, c, 0.0), Vec3::new(0.0, 0.0, 1.0)] }
    }

    #[inline]
    pub fn mul_vec(&self, v: Vec3) -> Vec3 {
        Vec3::new(self.r[0].dot(v), self.r[1].dot(v), self.r[2].dot(v))
    }

    // Transpuesta = inversa para rotaciones
    pub fn transpose(&self) -> Mat3 {
        let [a, b, c] = self.r;
        Mat3 { r: [Vec3::new(a.x, b.x, c.x), Vec3::new(a.y, b.y, c.y), Vec3::new(a.z, b.z, c.z)] }
    }

    pub fn mul(&self, o: &Mat3) -> Mat3 {
        let t = o.transpose();
        Mat3 { r: [0, 1, 2].map(|i| Vec3::new(self.r[i].dot(t.r[0]), self.r[i].dot(t.r[1]), self.r[i].dot(t.r[2]))) }
    }
}
