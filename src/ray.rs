use crate::vec3::{Point3, Vec3};

#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Point3,
    pub dir: Vec3,
    pub inv_dir: Vec3, // 1/dir precalculado para las pruebas contra cajas
}

impl Ray {
    pub fn new(origin: Point3, dir: Vec3) -> Self {
        Ray {
            origin,
            dir,
            inv_dir: Vec3::new(1.0 / dir.x, 1.0 / dir.y, 1.0 / dir.z),
        }
    }

    pub fn at(&self, t: f64) -> Point3 {
        self.origin + self.dir * t
    }
}
