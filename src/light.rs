use crate::vec3::{Color, Point3, Vec3};

// Luces: puntuales (ventanas, faroles) y direccional (sol o luna)
#[derive(Clone)]
pub enum Light {
    Point(PointLight),
    Directional { direction: Vec3, color: Color, intensity: f64 },
}

#[derive(Clone)]
pub struct PointLight {
    pub position: Point3,
    pub color: Color,
    pub intensity: f64,
    pub range: f64, // mas alla de esta distancia la luz ya no aporta y no se traza su sombra
}

impl Light {
    pub fn point(position: Point3, color: Color, intensity: f64) -> Self {
        Light::Point(PointLight { position, color, intensity, range: (intensity / 0.02).sqrt() })
    }

    // Luz puntual con alcance fijo (una vela ilumina su cuarto, no todo el jardin)
    pub fn point_range(position: Point3, color: Color, intensity: f64, range: f64) -> Self {
        Light::Point(PointLight { position, color, intensity, range })
    }
}
