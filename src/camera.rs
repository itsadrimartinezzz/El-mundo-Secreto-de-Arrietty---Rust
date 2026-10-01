use crate::ray::Ray;
use crate::rng::Rng;
use crate::vec3::{Point3, Vec3};

#[derive(Clone, Copy, PartialEq)]
pub struct Camera {
    pub origin: Point3,
    lower_left: Point3,
    horizontal: Vec3,
    vertical: Vec3,
    u: Vec3,
    v: Vec3,
    w: Vec3,
    focus: f64,
    lens_radius: f64,
}

impl Camera {
    // Camara look-at con desenfoque de lente opcional; vfov en grados
    pub fn new(look_from: Point3, look_at: Point3, vup: Vec3, vfov: f64, aspect: f64, aperture: f64, focus: f64) -> Self {
        let h = (vfov.to_radians() / 2.0).tan();
        let (vh, vw) = (2.0 * h, 2.0 * h * aspect);
        let w = (look_from - look_at).unit();
        let u = vup.cross(w).unit();
        let v = w.cross(u);
        let horizontal = u * (vw * focus);
        let vertical = v * (vh * focus);
        let lower_left = look_from - horizontal / 2.0 - vertical / 2.0 - w * focus;
        Camera { origin: look_from, lower_left, horizontal, vertical, u, v, w, focus, lens_radius: aperture / 2.0 }
    }

    // Camara que orbita alrededor de `target` (yaw/pitch en grados)
    pub fn orbit(target: Point3, distance: f64, yaw_deg: f64, pitch_deg: f64, vfov: f64, aspect: f64, aperture: f64) -> Self {
        let (yaw, pitch) = (yaw_deg.to_radians(), pitch_deg.to_radians());
        let dir = Vec3::new(yaw.cos() * pitch.cos(), pitch.sin(), yaw.sin() * pitch.cos());
        Camera::new(target + dir * distance, target, Vec3::new(0.0, 1.0, 0.0), vfov, aspect, aperture, distance)
    }

    pub fn get_ray(&self, s: f64, t: f64, rng: &mut Rng) -> Ray {
        if self.lens_radius <= 0.0 {
            return Ray::new(self.origin, self.lower_left + self.horizontal * s + self.vertical * t - self.origin);
        }
        let rd = rng.in_unit_disk() * self.lens_radius;
        let offset = self.u * rd.x + self.v * rd.y;
        Ray::new(self.origin + offset, self.lower_left + self.horizontal * s + self.vertical * t - self.origin - offset)
    }

    pub fn view_depth(&self, p: Point3) -> f64 {
        -(p - self.origin).dot(self.w)
    }

    // Proyecta un punto del mundo a coordenadas (s,t) de pantalla; None si queda detras de la camara
    pub fn project(&self, p: Point3) -> Option<(f64, f64)> {
        let d = p - self.origin;
        let depth = -d.dot(self.w);
        if depth <= 1.0e-6 {
            return None;
        }
        let q = self.origin + d * (self.focus / depth) - self.lower_left;
        Some((q.dot(self.horizontal) / self.horizontal.length_squared(), q.dot(self.vertical) / self.vertical.length_squared()))
    }
}
