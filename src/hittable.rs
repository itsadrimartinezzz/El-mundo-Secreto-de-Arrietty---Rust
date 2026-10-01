use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::bvh::BvhNode;
use crate::material::Material;
use crate::ray::Ray;
use crate::vec3::{Mat3, Point3, Vec3};

// Resultado de un impacto; el material va por referencia para no clonarlo en cada rayo
pub struct HitRecord<'a> {
    pub t: f64,
    pub p: Point3,
    pub normal: Vec3,       // normal orientada contra el rayo (para sombreado)
    pub outward: Vec3,      // normal geometrica hacia afuera, en el mundo
    pub local_normal: Vec3, // normal hacia afuera en el espacio del objeto (elige la cara de la textura)
    pub front_face: bool,
    pub obj_point: Vec3, // punto en coordenadas locales del objeto (para las texturas)
    pub material: &'a Material,
}

// Orienta la normal contra el rayo y dice si pegamos por fuera
fn face_normal(ray: &Ray, outward: Vec3) -> (Vec3, bool) {
    if ray.dir.dot(outward) < 0.0 {
        (outward, true)
    } else {
        (-outward, false)
    }
}

// Rotacion de un objeto alrededor de un punto (la inversa es la transpuesta: no se guarda)
#[derive(Clone, Copy)]
pub struct Xform {
    rot: Mat3,
    pivot: Point3,
}

impl Xform {
    pub fn new(rot: Mat3, pivot: Point3) -> Self {
        Xform { rot, pivot }
    }

    // Lleva el rayo al espacio local del objeto (la distancia t se conserva)
    fn to_local(&self, ray: &Ray) -> Ray {
        let inv = self.rot.transpose();
        Ray::new(inv.mul_vec(ray.origin - self.pivot) + self.pivot, inv.mul_vec(ray.dir))
    }
}

// Caja alineada a los ejes usada por el BVH
#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: Point3,
    pub max: Point3,
}

impl Aabb {
    pub fn centroid(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn surrounding(a: &Aabb, b: &Aabb) -> Aabb {
        Aabb {
            min: Vec3::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y), a.min.z.min(b.min.z)),
            max: Vec3::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y), a.max.z.max(b.max.z)),
        }
    }

    // Caja que contiene a otra despues de rotarla
    fn transformed(&self, x: &Xform) -> Aabb {
        let mut out = Aabb { min: Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY), max: Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY) };
        for i in 0..8 {
            let c = Vec3::new(
                if i & 1 == 0 { self.min.x } else { self.max.x },
                if i & 2 == 0 { self.min.y } else { self.max.y },
                if i & 4 == 0 { self.min.z } else { self.max.z },
            );
            let w = x.rot.mul_vec(c - x.pivot) + x.pivot;
            out = Aabb::surrounding(&out, &Aabb { min: w, max: w });
        }
        out
    }

    // Igual que `hit`, pero devuelve la distancia de entrada a la caja (para visitar primero lo mas cercano)
    #[inline]
    pub fn entry(&self, ray: &Ray, mut t_min: f64, mut t_max: f64) -> Option<f64> {
        for axis in 0..3 {
            let inv_d = ray.inv_dir[axis];
            let mut t0 = (self.min[axis] - ray.origin[axis]) * inv_d;
            let mut t1 = (self.max[axis] - ray.origin[axis]) * inv_d;
            if inv_d < 0.0 {
                std::mem::swap(&mut t0, &mut t1);
            }
            t_min = t0.max(t_min);
            t_max = t1.min(t_max);
            if t_max <= t_min {
                return None;
            }
        }
        Some(t_min)
    }

    // Prueba por "slabs": intersecta los rangos de t de los tres ejes
    #[inline]
    pub fn hit(&self, ray: &Ray, mut t_min: f64, mut t_max: f64) -> bool {
        for axis in 0..3 {
            let inv_d = ray.inv_dir[axis];
            let mut t0 = (self.min[axis] - ray.origin[axis]) * inv_d;
            let mut t1 = (self.max[axis] - ray.origin[axis]) * inv_d;
            if inv_d < 0.0 {
                std::mem::swap(&mut t0, &mut t1);
            }
            t_min = t0.max(t_min);
            t_max = t1.min(t_max);
            if t_max <= t_min {
                return false;
            }
        }
        true
    }
}

fn axis_vec(axis: usize, sign: f64) -> Vec3 {
    match axis {
        0 => Vec3::new(sign, 0.0, 0.0),
        1 => Vec3::new(0.0, sign, 0.0),
        _ => Vec3::new(0.0, 0.0, sign),
    }
}

// Cubo/prisma texturizado (la primitiva principal), con rotacion opcional alrededor de su centro.
// Los objetos guardan solo el indice de su material: asi son chicos y el recorrido usa mejor la cache
pub struct Cuboid {
    pub min: Point3,
    pub max: Point3,
    pub xform: Option<Box<Xform>>, // en caja: casi ningun prisma gira y asi el objeto ocupa menos cache
    pub mat: u32,
}

impl Cuboid {
    pub fn new(min: Point3, max: Point3, mat: u32) -> Self {
        Cuboid { min, max, xform: None, mat }
    }

    // Prisma centrado en `center` con medias medidas `half`, girado con `rot`
    pub fn rotated(center: Point3, half: Vec3, rot: Mat3, mat: u32) -> Self {
        Cuboid { min: center - half, max: center + half, xform: Some(Box::new(Xform::new(rot, center))), mat }
    }

    fn hit<'a>(&self, world_ray: &Ray, t_min: f64, t_max: f64, mats: &'a [Material]) -> Option<HitRecord<'a>> {
        let local;
        let ray = match &self.xform {
            Some(x) => {
                local = x.to_local(world_ray);
                &local
            }
            None => world_ray,
        };
        let (mut t_near, mut t_far) = (f64::NEG_INFINITY, f64::INFINITY);
        let (mut axis_near, mut axis_far) = (0, 0);
        for axis in 0..3 {
            let inv_d = ray.inv_dir[axis];
            let mut t0 = (self.min[axis] - ray.origin[axis]) * inv_d;
            let mut t1 = (self.max[axis] - ray.origin[axis]) * inv_d;
            if inv_d < 0.0 {
                std::mem::swap(&mut t0, &mut t1);
            }
            if t0 > t_near {
                t_near = t0;
                axis_near = axis;
            }
            if t1 < t_far {
                t_far = t1;
                axis_far = axis;
            }
        }
        if t_near > t_far {
            return None;
        }
        // Entrada si esta en rango; si no, salida (rayo que nace dentro del cubo, p.ej. refraccion)
        let (t, axis) = if t_near >= t_min && t_near <= t_max {
            (t_near, axis_near)
        } else if t_far >= t_min && t_far <= t_max {
            (t_far, axis_far)
        } else {
            return None;
        };

        let lp = ray.at(t);
        let center = (self.min + self.max) * 0.5;
        let local_normal = axis_vec(axis, if lp[axis] > center[axis] { 1.0 } else { -1.0 });
        let outward = match &self.xform {
            Some(x) => x.rot.mul_vec(local_normal),
            None => local_normal,
        };
        let (normal, front_face) = face_normal(world_ray, outward);
        Some(HitRecord { t, p: world_ray.at(t), normal, outward, local_normal, front_face, obj_point: lp - self.min, material: &mats[self.mat as usize] })
    }

    fn bounding_box(&self) -> Aabb {
        let pad = Vec3::new(1.0e-4, 1.0e-4, 1.0e-4);
        let b = Aabb { min: self.min - pad, max: self.max + pad };
        match &self.xform {
            Some(x) => b.transformed(x),
            None => b,
        }
    }
}

// Cilindro con tapas: eje Y local desde `base` hasta `base + height`, con rotacion opcional alrededor de la base
pub struct Cylinder {
    pub base: Point3,
    pub radius: f64,
    pub height: f64,
    pub xform: Option<Box<Xform>>,
    pub mat: u32,
}

impl Cylinder {
    pub fn new(base: Point3, radius: f64, height: f64, mat: u32) -> Self {
        Cylinder { base, radius, height, xform: None, mat }
    }

    pub fn rotated(base: Point3, radius: f64, height: f64, rot: Mat3, mat: u32) -> Self {
        Cylinder { base, radius, height, xform: Some(Box::new(Xform::new(rot, base))), mat }
    }

    fn hit<'a>(&self, world_ray: &Ray, t_min: f64, t_max: f64, mats: &'a [Material]) -> Option<HitRecord<'a>> {
        let local;
        let ray = match &self.xform {
            Some(x) => {
                local = x.to_local(world_ray);
                &local
            }
            None => world_ray,
        };
        let o = ray.origin - self.base;
        let d = ray.dir;
        let mut best: Option<(f64, Vec3)> = None;
        let mut consider = |t: f64, n: Vec3| {
            if t >= t_min && t <= t_max && best.map_or(true, |(bt, _)| t < bt) {
                best = Some((t, n));
            }
        };
        // Pared lateral: x^2 + z^2 = r^2
        let a = d.x * d.x + d.z * d.z;
        if a > 1.0e-12 {
            let half_b = o.x * d.x + o.z * d.z;
            let c = o.x * o.x + o.z * o.z - self.radius * self.radius;
            let disc = half_b * half_b - a * c;
            if disc >= 0.0 {
                let sq = disc.sqrt();
                for t in [(-half_b - sq) / a, (-half_b + sq) / a] {
                    let y = o.y + d.y * t;
                    if y >= 0.0 && y <= self.height {
                        consider(t, Vec3::new((o.x + d.x * t) / self.radius, 0.0, (o.z + d.z * t) / self.radius));
                    }
                }
            }
        }
        // Tapas de abajo y arriba
        if d.y.abs() > 1.0e-12 {
            for (y, ny) in [(0.0, -1.0), (self.height, 1.0)] {
                let t = (y - o.y) / d.y;
                let (px, pz) = (o.x + d.x * t, o.z + d.z * t);
                if px * px + pz * pz <= self.radius * self.radius {
                    consider(t, Vec3::new(0.0, ny, 0.0));
                }
            }
        }
        let (t, local_normal) = best?;
        let outward = match &self.xform {
            Some(x) => x.rot.mul_vec(local_normal),
            None => local_normal,
        };
        let (normal, front_face) = face_normal(world_ray, outward);
        Some(HitRecord { t, p: world_ray.at(t), normal, outward, local_normal, front_face, obj_point: o + d * t, material: &mats[self.mat as usize] })
    }

    fn bounding_box(&self) -> Aabb {
        let r = self.radius + 1.0e-4;
        let b = Aabb { min: self.base - Vec3::new(r, 1.0e-4, r), max: self.base + Vec3::new(r, self.height + 1.0e-4, r) };
        match &self.xform {
            Some(x) => b.transformed(x),
            None => b,
        }
    }
}

// Elipsoide con rotacion opcional: gato, hojas, hortensias, gotas
pub struct Sphere {
    pub center: Point3,
    pub radii: Vec3,
    pub rot: Option<Box<Mat3>>, // local->mundo (la inversa es su transpuesta)
    pub mat: u32,
}

impl Sphere {
    pub fn uniform(center: Point3, radius: f64, mat: u32) -> Self {
        Sphere { center, radii: Vec3::new(radius, radius, radius), rot: None, mat }
    }

    pub fn ellipsoid(center: Point3, radii: Vec3, mat: u32) -> Self {
        Sphere { center, radii, rot: None, mat }
    }

    pub fn rotated(center: Point3, radii: Vec3, rot: Mat3, mat: u32) -> Self {
        Sphere { center, radii, rot: Some(Box::new(rot)), mat }
    }

    fn hit<'a>(&self, ray: &Ray, t_min: f64, t_max: f64, mats: &'a [Material]) -> Option<HitRecord<'a>> {
        // El rayo se lleva al espacio local donde el elipsoide es una esfera unitaria
        let (mut o, mut d) = (ray.origin - self.center, ray.dir);
        if let Some(rot) = &self.rot {
            let inv = rot.transpose();
            o = inv.mul_vec(o);
            d = inv.mul_vec(d);
        }
        let oc = o.component_div(self.radii);
        let dir = d.component_div(self.radii);
        let a = dir.length_squared();
        let half_b = oc.dot(dir);
        let c = oc.length_squared() - 1.0;
        let disc = half_b * half_b - a * c;
        if disc < 0.0 {
            return None;
        }
        let sqrtd = disc.sqrt();
        let mut root = (-half_b - sqrtd) / a;
        if root < t_min || root > t_max {
            root = (-half_b + sqrtd) / a;
            if root < t_min || root > t_max {
                return None;
            }
        }
        let p_obj = oc + dir * root;
        let local_normal = p_obj.component_div(self.radii).unit();
        let outward = match &self.rot {
            Some(m) => m.mul_vec(local_normal),
            None => local_normal,
        };
        let (normal, front_face) = face_normal(ray, outward);
        Some(HitRecord { t: root, p: ray.at(root), normal, outward, local_normal, front_face, obj_point: p_obj, material: &mats[self.mat as usize] })
    }

    fn bounding_box(&self) -> Aabb {
        let r = if self.rot.is_some() {
            let m = self.radii.max_component();
            Vec3::new(m, m, m)
        } else {
            self.radii
        };
        let pad = Vec3::new(1.0e-4, 1.0e-4, 1.0e-4);
        Aabb { min: self.center - r - pad, max: self.center + r + pad }
    }
}

// Triangulo de doble cara para hojas plegadas, petalos y hastiales.
// Las coordenadas de textura son unas pocas plantillas (hoja, petalo...) compartidas por todos:
// el triangulo guarda solo un puntero y ocupa menos cache al recorrer el BVH
pub struct Triangle {
    pub vertices: [Vec3; 3],
    pub uv: &'static [Vec3; 3],
    pub mat: u32,
}

impl Triangle {
    pub fn new(vertices: [Vec3; 3], uv: [Vec3; 3], mat: u32) -> Self {
        static SHARED: OnceLock<Mutex<HashMap<[u64; 9], &'static [Vec3; 3]>>> = OnceLock::new();
        let key: [u64; 9] = std::array::from_fn(|i| uv[i / 3][i % 3].to_bits());
        let mut shared = SHARED.get_or_init(Default::default).lock().unwrap();
        let uv = *shared.entry(key).or_insert_with(|| Box::leak(Box::new(uv)));
        Triangle { vertices, uv, mat }
    }

    fn hit<'a>(&self, ray: &Ray, t_min: f64, t_max: f64, mats: &'a [Material]) -> Option<HitRecord<'a>> {
        let [a, b, c] = self.vertices;
        let e1 = b - a;
        let e2 = c - a;
        let p = ray.dir.cross(e2);
        let det = e1.dot(p);
        if det.abs() < 1.0e-12 { return None; }
        let inv = 1.0 / det;
        let s = ray.origin - a;
        let u = s.dot(p) * inv;
        if !(0.0..=1.0).contains(&u) { return None; }
        let q = s.cross(e1);
        let vv = ray.dir.dot(q) * inv;
        if vv < 0.0 || u + vv > 1.0 { return None; }
        let t = e2.dot(q) * inv;
        if t < t_min || t > t_max { return None; }
        let outward = e1.cross(e2).unit();
        let (normal, front_face) = face_normal(ray, outward);
        Some(HitRecord { t, p: ray.at(t), normal, outward, local_normal: Vec3::new(0.0, 1.0, 0.0), front_face,
            obj_point: self.uv[0] * (1.0 - u - vv) + self.uv[1] * u + self.uv[2] * vv, material: &mats[self.mat as usize] })
    }

    fn bounding_box(&self) -> Aabb {
        let [a, b, c] = self.vertices;
        let pad = Vec3::new(0.0001, 0.0001, 0.0001);
        Aabb { min: Vec3::new(a.x.min(b.x).min(c.x), a.y.min(b.y).min(c.y), a.z.min(b.z).min(c.z)) - pad,
            max: Vec3::new(a.x.max(b.x).max(c.x), a.y.max(b.y).max(c.y), a.z.max(b.z).max(c.z)) + pad }
    }
}

pub enum Object {
    Triangle(Triangle),
    Cuboid(Cuboid),
    Cylinder(Cylinder),
    Sphere(Sphere),
}

impl Object {
    #[inline]
    pub fn hit<'a>(&self, ray: &Ray, t_min: f64, t_max: f64, mats: &'a [Material]) -> Option<HitRecord<'a>> {
        match self {
            Object::Triangle(t) => t.hit(ray, t_min, t_max, mats),
            Object::Cuboid(b) => b.hit(ray, t_min, t_max, mats),
            Object::Cylinder(c) => c.hit(ray, t_min, t_max, mats),
            Object::Sphere(s) => s.hit(ray, t_min, t_max, mats),
        }
    }

    pub fn bounding_box(&self) -> Aabb {
        match self {
            Object::Triangle(t) => t.bounding_box(),
            Object::Cuboid(b) => b.bounding_box(),
            Object::Cylinder(c) => c.bounding_box(),
            Object::Sphere(s) => s.bounding_box(),
        }
    }
}

// Mundo con dos capas: objetos que proyectan sombra y decoracion que no (pasto, flores, lluvia, agua)
pub struct World {
    materials: Vec<Material>,
    objects: Vec<Object>,
    bvh: Option<BvhNode>,
    decor: Vec<Object>,
    decor_bvh: Option<BvhNode>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        World { materials: Vec::new(), objects: Vec::new(), bvh: None, decor: Vec::new(), decor_bvh: None }
    }

    // Guarda un material y devuelve su indice para los objetos
    pub fn add_material(&mut self, m: Material) -> u32 {
        self.materials.push(m);
        (self.materials.len() - 1) as u32
    }

    pub fn push(&mut self, obj: Object) {
        self.objects.push(obj);
    }

    // Objeto que no proyecta sombra: los rayos de sombra ni siquiera recorren esta capa
    pub fn push_decor(&mut self, obj: Object) {
        self.decor.push(obj);
    }

    pub fn len(&self) -> usize {
        self.objects.len() + self.decor.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // Construye los BVH de ambas capas (una vez, despues de armar la escena)
    pub fn build(&mut self) {
        self.bvh = Some(BvhNode::build(&mut self.objects));
        self.decor_bvh = Some(BvhNode::build(&mut self.decor));
    }

    pub fn hit(&self, ray: &Ray, t_min: f64, t_max: f64) -> Option<HitRecord<'_>> {
        let bvh = self.bvh.as_ref().expect("World::build() debe llamarse antes de renderizar");
        let solid = bvh.hit(ray, t_min, t_max, &self.objects, &self.materials);
        let closest = solid.as_ref().map_or(t_max, |r| r.t);
        let decor = self.decor_bvh.as_ref().and_then(|b| b.hit(ray, t_min, closest, &self.decor, &self.materials));
        decor.or(solid)
    }

    // Rayo de sombra: basta con encontrar un bloqueador opaco en la capa que proyecta sombras
    pub fn is_shadowed(&self, ray: &Ray, t_max: f64) -> bool {
        self.bvh.as_ref().is_some_and(|b| b.any_opaque_hit(ray, 0.001, t_max, &self.objects, &self.materials))
    }
}

#[cfg(test)]
mod triangle_tests {
    use super::*;
    use crate::texture::Texture;

    #[test]
    fn triangle_hits_both_sides_and_rejects_outside_and_parallel_rays() {
        let tri = Triangle::new([Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)], [Vec3::ZERO, Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0)], 0);
        let mats = [Material::new(Texture::Solid(Vec3::new(1.0, 1.0, 1.0)))];
        for side in [-1.0, 1.0] {
            let ray = Ray::new(Vec3::new(0.25, 0.25, side), Vec3::new(0.0, 0.0, -side));
            let hit = tri.hit(&ray, 0.001, 2.0, &mats).unwrap();
            assert!((hit.t - 1.0).abs() < 1e-9);
            assert!(hit.normal.dot(ray.dir) < 0.0);
            assert!((hit.obj_point.x - 0.25).abs() < 1e-9);
            assert!(tri.bounding_box().hit(&ray, 0.001, 2.0));
            assert!(tri.hit(&ray, 0.001, 0.5, &mats).is_none());
        }
        assert!(tri.hit(&Ray::new(Vec3::new(0.8, 0.8, 1.0), Vec3::new(0.0, 0.0, -1.0)), 0.001, 2.0, &mats).is_none());
        assert!(tri.hit(&Ray::new(Vec3::new(0.2, 0.2, 1.0), Vec3::new(1.0, 0.0, 0.0)), 0.001, 2.0, &mats).is_none());
    }
}

