use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};

use crate::camera::Camera;
use crate::ray::Ray;
use crate::hittable::{Cuboid, Cylinder, HitRecord, Object, Sphere, Triangle, World};
use crate::light::Light;
use crate::material::Material;
use crate::sky::{smoothstep, sun_dir, SkyParams};
use crate::texture::{hex, Texture};
use crate::vec3::{Color, Mat3, Vec3};

// Vista inicial de la camara orbital (en metros), compartida por los tres binarios
pub const CAM_TARGET: Vec3 = Vec3::new(-6.0, 6.4, -11.0);
pub const CAM_YAW: f64 = 78.0;
pub const CAM_PITCH: f64 = 10.0;
pub const CAM_DISTANCE: f64 = 32.0;
pub const CAM_VFOV: f64 = 55.0;

// Puntos de vista predefinidos (tecla V en la ventana)
pub struct View {
    pub name: &'static str,
    pub target: Vec3,
    pub yaw: f64,
    pub pitch: f64,
    pub dist: f64,
    pub dist_range: (f64, f64),
    pub pitch_range: (f64, f64),
}

pub const VIEWS: [View; 7] = [
    View { name: "jardin", target: CAM_TARGET, yaw: CAM_YAW, pitch: CAM_PITCH, dist: CAM_DISTANCE, dist_range: (8.0, 34.0), pitch_range: (2.0, 55.0) },
    // A la altura de Arrietty (10 cm): el pasto se ve gigante, como en la pelicula
    View { name: "casita de Arrietty", target: Vec3::new(-5.985, 0.073, -7.005), yaw: 82.0, pitch: 9.0, dist: 0.30, dist_range: (0.2, 3.0), pitch_range: (1.0, 45.0) },
    View { name: "lago", target: Vec3::new(13.0, -0.3, -18.0), yaw: 22.0, pitch: 32.0, dist: 12.0, dist_range: (3.0, 16.0), pitch_range: (8.0, 75.0) },
    View { name: "gatito", target: Vec3::new(-6.8, 0.3, -2.1), yaw: 70.0, pitch: 20.0, dist: 2.0, dist_range: (0.8, 8.0), pitch_range: (8.0, 60.0) },
    View { name: "flores", target: Vec3::new(-3.5, 0.65, 1.8), yaw: 65.0, pitch: 24.0, dist: 3.3, dist_range: (1.0, 9.0), pitch_range: (5.0, 65.0) },
    View { name: "lectura junto al kiosko", target: Vec3::new(0.65, 0.5, -15.5), yaw: 75.0, pitch: 24.0, dist: 3.7, dist_range: (1.5, 14.0), pitch_range: (5.0, 65.0) },
    View { name: "Arrietty junto a Sho", target: Vec3::new(-0.40, 0.073, -14.75), yaw: 82.0, pitch: 9.0, dist: 0.30, dist_range: (0.2, 3.0), pitch_range: (1.0, 45.0) },
];

pub struct SceneData {
    pub fixed: Arc<World>, // geometria que no se mueve (se reutiliza entre cuadros)
    pub moving: World,     // lo animado de este cuadro (gato, agua, lluvia, hojas)
    pub lights: Vec<Light>,
    pub sky: SkyParams,
    pub fog_start: f64,
    pub wetness: f64,                 // 0 seco, 1 empapado por la lluvia
    pub dry_zones: Vec<(Vec3, Vec3)>, // cajas techadas donde la lluvia no moja
}

impl SceneData {
    pub fn hit(&self, ray: &Ray, t_min: f64, t_max: f64) -> Option<HitRecord<'_>> {
        let b = self.moving.hit(ray, t_min, t_max);
        let a = self.fixed.hit(ray, t_min, b.as_ref().map_or(t_max, |r| r.t));
        match (a, b) {
            (Some(a), Some(b)) => Some(if a.t < b.t { a } else { b }),
            (a, b) => b.or(a),
        }
    }

    pub fn is_shadowed(&self, ray: &Ray, t_max: f64) -> bool {
        self.fixed.is_shadowed(ray, t_max) || self.moving.is_shadowed(ray, t_max)
    }

    pub fn object_count(&self) -> usize {
        self.fixed.len() + self.moving.len()
    }

    pub fn is_dry(&self, p: Vec3) -> bool {
        self.dry_zones.iter().any(|(a, b)| p.x > a.x && p.x < b.x && p.y > a.y && p.y < b.y && p.z > a.z && p.z < b.z)
    }
}

// Posicion del sol en pantalla (0..1, y hacia abajo) para los rayos de luz del post-procesado
pub fn sun_screen(cam: &Camera) -> Option<(f32, f32)> {
    let (s, t) = cam.project(cam.origin + sun_dir() * 1000.0)?;
    if (-0.6..1.6).contains(&s) && (-0.6..1.6).contains(&t) {
        Some((s as f32, (1.0 - t) as f32))
    } else {
        None
    }
}

// Gotas de lluvia de este instante como segmentos 3D (se dibujan en pantalla despues del raytracing)
pub fn rain_streaks(t: f64, station: f64) -> Vec<(Vec3, Vec3)> {
    let rain = smoothstep(0.4, 1.0, station);
    let drops = (1400.0 * rain) as u64;
    let mut out = Vec::with_capacity(drops as usize);
    for i in 0..drops {
        let (x, z) = (-26.0 + h1(i * 5) * 50.0, -30.0 + h1(i * 5 + 1) * 48.0);
        if x > -24.0 && x < -2.0 && z > -23.0 && z < -7.8 {
            continue; // bajo el techo de la casa
        }
        let speed = 14.0 + 4.0 * h1(i * 5 + 2);
        let y = 16.0 - (t * speed + h1(i * 5 + 3) * 16.0).rem_euclid(16.0);
        out.push((v(x, y, z), v(x + 0.08, y + 0.8, z))); // un poco inclinada por el viento
    }
    out
}

// Hash determinista [0,1) para esparcir objetos (pasto, flores, arboles, lluvia...)
fn h1(i: u64) -> f64 {
    let mut x = i.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0x632B_E59B_D9B4_E019);
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

fn v(x: f64, y: f64, z: f64) -> Vec3 {
    Vec3::new(x, y, z)
}

fn solid(c: u32) -> Material {
    Material::new(Texture::Solid(hex(c)))
}

// ---------- Atajos para agregar primitivas ----------

struct Builder {
    world: World,
    decor: bool,                 // true: lo que se agregue no proyecta sombra
    xf: Option<(Vec3, f64)>,     // escala todo alrededor de un ancla (para agrandar casa y kiosco)
}

impl Builder {
    fn triangle(&mut self, vertices: [Vec3; 3], uv: [Vec3; 3], m: &Material) {
        let mat = self.world.add_material(m.clone());
        self.triangle_id(vertices, uv, mat);
    }

    // Triangulo con un material ya registrado (las hojas comparten uno solo entre sus triangulos)
    fn triangle_id(&mut self, vertices: [Vec3; 3], uv: [Vec3; 3], mat: u32) {
        self.push(Object::Triangle(Triangle::new(vertices.map(|p| self.map(p)), uv, mat)));
    }

    // Triangulos de una hoja o petalo: comparten un solo material (no se copia por triangulo)
    fn foliage_triangles(&mut self, tris: impl Iterator<Item = ([Vec3; 3], [Vec3; 3])>, m: &Material) {
        let mat = self.world.add_material(m.clone());
        for (vertices, uv) in tris {
            self.triangle_id(vertices, uv, mat);
        }
    }

    // Hoja lanceolada: punta, borde ancho y nervio elevado, nunca una esfera.
    fn leaf(&mut self, c: Vec3, len: f64, width: f64, rot: Mat3, m: &Material) {
        let edge = [v(-1.0, 0.0, 0.0), v(-0.35, 0.0, -0.85), v(0.35, 0.0, -0.7),
                    v(1.0, -0.14, 0.0), v(0.35, 0.0, 0.7), v(-0.35, 0.0, 0.85)];
        let center = v(0.0, 0.12, 0.0);
        let at = |p: Vec3| c + rot.mul_vec(v(p.x * len, p.y * len, p.z * width));
        self.foliage_triangles((0..6).map(|i| {
            let uv = [center, edge[i], edge[(i + 1) % 6]];
            (uv.map(at), uv)
        }), m);
    }

    fn petal(&mut self, c: Vec3, len: f64, width: f64, rot: Mat3, m: &Material) {
        // Contorno redondeado y borde curvado, distinto de las hojas puntiagudas.
        let edge = [v(-1.0, 0.0, 0.0), v(-0.6, 0.04, -0.6), v(0.0, 0.09, -0.94),
            v(0.55, 0.14, -0.85), v(0.9, 0.19, -0.5), v(1.0, 0.2, 0.0),
            v(0.9, 0.19, 0.5), v(0.55, 0.14, 0.85), v(0.0, 0.09, 0.94), v(-0.6, 0.04, 0.6)];
        let at = |p: Vec3| c + rot.mul_vec(v(p.x * len, p.y * len, p.z * width));
        let n = edge.len();
        self.foliage_triangles((0..n).map(|i| {
            let uv = [v(0.0, 0.01, 0.0), edge[i], edge[(i + 1) % n]];
            (uv.map(at), uv)
        }), m);
    }
    fn new() -> Self {
        Builder { world: World::new(), decor: false, xf: None }
    }
    // Aplica la escala activa a un punto y a una medida
    fn map(&self, p: Vec3) -> Vec3 {
        match self.xf {
            Some((a, s)) => a + (p - a) * s,
            None => p,
        }
    }
    fn sc(&self, x: f64) -> f64 {
        self.xf.map_or(x, |(_, s)| x * s)
    }
    fn push(&mut self, o: Object) {
        if self.decor {
            self.world.push_decor(o);
        } else {
            self.world.push(o);
        }
    }
    // Prisma alineado entre dos esquinas
    fn cube(&mut self, a: (f64, f64, f64), b: (f64, f64, f64), m: &Material) {
        let min = self.map(v(a.0.min(b.0), a.1.min(b.1), a.2.min(b.2)));
        let max = self.map(v(a.0.max(b.0), a.1.max(b.1), a.2.max(b.2)));
        let id = self.world.add_material(m.clone());
        self.push(Object::Cuboid(Cuboid::new(min, max, id)));
    }
    // Prisma girado alrededor de su centro
    fn rcube(&mut self, c: Vec3, half: Vec3, rot: Mat3, m: &Material) {
        let (c, half) = (self.map(c), half * self.sc(1.0));
        let id = self.world.add_material(m.clone());
        self.push(Object::Cuboid(Cuboid::rotated(c, half, rot, id)));
    }
    fn cyl(&mut self, base: Vec3, r: f64, h: f64, m: &Material) {
        let (base, r, h) = (self.map(base), self.sc(r), self.sc(h));
        let id = self.world.add_material(m.clone());
        self.push(Object::Cylinder(Cylinder::new(base, r, h, id)));
    }
    fn rcyl(&mut self, base: Vec3, r: f64, h: f64, rot: Mat3, m: &Material) {
        let (base, r, h) = (self.map(base), self.sc(r), self.sc(h));
        let id = self.world.add_material(m.clone());
        self.push(Object::Cylinder(Cylinder::rotated(base, r, h, rot, id)));
    }
    fn ell(&mut self, c: Vec3, radii: Vec3, m: &Material) {
        let (c, radii) = (self.map(c), radii * self.sc(1.0));
        let id = self.world.add_material(m.clone());
        self.push(Object::Sphere(Sphere::ellipsoid(c, radii, id)));
    }
    fn rell(&mut self, c: Vec3, radii: Vec3, rot: Mat3, m: &Material) {
        let (c, radii) = (self.map(c), radii * self.sc(1.0));
        let id = self.world.add_material(m.clone());
        self.push(Object::Sphere(Sphere::rotated(c, radii, rot, id)));
    }
    fn ball(&mut self, c: Vec3, r: f64, m: &Material) {
        let (c, r) = (self.map(c), self.sc(r));
        let id = self.world.add_material(m.clone());
        self.push(Object::Sphere(Sphere::uniform(c, r, id)));
    }
}

// ---------- Materiales (cada uno con su textura y sus propios parametros) ----------

struct Mats {
    siding: Material,
    trim: Material,
    white: Material,
    slate: Material,
    stone: Material,
    brick: Material,
    ground: Material,
    lakebed: Material,
    water: Material,
    window: Material,
    stained: Material,
    door: Material,
    wood: Material,
    rock: Material,
    leaves: Material,
    leaves_dark: Material,
    ivy: Material,
    leaves_far: Material,
    pine: Material,
    bark: Material,
    blades: [Material; 3],
    fur: Material,
}

fn materials(t: f64, station: f64, rain: f64) -> Mats {
    let glow = Color::new(1.0, 0.66, 0.32) * (0.03 + 1.6 * station);
    Mats {
        siding: Material::new(Texture::Siding { base: hex(0xE0BE60), trim: hex(0x9A7A3A) }).specular(0.1, 16.0),
        trim: solid(0x8A3A28).specular(0.3, 32.0).wet_gloss(0.3),
        white: solid(0xF2EEE2).specular(0.3, 32.0),
        slate: Material::new(Texture::Roof { base: hex(0x5E6878), moss: hex(0x566470) }).specular(0.35, 48.0).reflectivity(0.04).wet_gloss(0.8),
        stone: Material::new(Texture::Stone { base: hex(0x9A948A), mortar: hex(0x5A564E), scale: 1.6 }).specular(0.1, 16.0).wet_gloss(0.7),
        brick: Material::new(Texture::Stone { base: hex(0xA0503A), mortar: hex(0x6A3A2A), scale: 2.5 }).specular(0.1, 16.0),
        ground: Material::new(Texture::Ground { top: 0.0 }).specular(0.04, 8.0),
        lakebed: Material::new(Texture::Rock { base: hex(0x6A6A58), moss: hex(0x3E6A4A) }),
        water: Material::new(Texture::Water { time: t, rain }).diffuse(0.5).specular(0.9, 220.0).transparency(0.72, 1.33),
        // Vidrio de las ventanas: refleja el cielo de tarde y brilla calido de noche
        window: solid(0x6A8A9A).diffuse(0.4).specular(1.0, 300.0).reflectivity(0.35).emission(glow),
        stained: Material::new(Texture::StainedGlass).diffuse(0.6).specular(1.0, 300.0).reflectivity(0.2).emission(glow * 0.8),
        door: Material::new(Texture::Planks { a: hex(0x4A3222), b: hex(0x6A4630), width: 0.22 }).specular(0.3, 32.0),
        wood: solid(0x6A4A30).specular(0.2, 24.0).wet_gloss(0.3),
        rock: Material::new(Texture::Rock { base: hex(0x6E706C), moss: hex(0x4E7A36) }).specular(0.15, 24.0).wet_gloss(0.6),
        leaves: Material::new(Texture::Foliage { dark: hex(0x294B2A), mid: hex(0x547A3D), light: hex(0x8C9E57) }).specular(0.08, 12.0),
        leaves_dark: Material::new(Texture::Foliage { dark: hex(0x1E4A3A), mid: hex(0x2E6A40), light: hex(0x5A9A48) }).specular(0.05, 8.0),
        ivy: Material::new(Texture::Leaf { a: hex(0x2E6A2A), b: hex(0x5A9A3A) }).specular(0.45, 60.0).wet_gloss(0.4),
        leaves_far: Material::new(Texture::Foliage { dark: hex(0x163636), mid: hex(0x22504A), light: hex(0x3E7A5A) }).diffuse(0.8),
        pine: Material::new(Texture::Foliage { dark: hex(0x1E4A2A), mid: hex(0x3A7A34), light: hex(0x7AAA48) }).specular(0.05, 8.0),
        bark: Material::new(Texture::Bark).specular(0.05, 8.0),
        blades: [
            Material::new(Texture::Blade { base: hex(0x2E6A2A), tip: hex(0x9CCB4A) }).specular(0.25, 24.0),
            Material::new(Texture::Blade { base: hex(0x3A7A2E), tip: hex(0xC8DC6A) }).specular(0.25, 24.0),
            Material::new(Texture::Blade { base: hex(0x1E5A34), tip: hex(0x6AAA4A) }).specular(0.25, 24.0),
        ],
        fur: Material::new(Texture::Fur { base: hex(0x8C7B6B), stripe: hex(0x4A3E36), belly: hex(0xE8DFD2) }).specular(0.12, 12.0),
    }
}

// Luz calida (ventanas, faroles) con alcance limitado; al atardecer no se agrega
fn warm_light(lights: &mut Vec<Light>, p: Vec3, t: f64, intensity: f64, range: f64) {
    let _ = t;
    lights.push(Light::point_range(p, Color::new(1.0, 0.68, 0.38), intensity, range));
}

// Zonas donde no crece pasto (casa, lago, kiosco, piedras del camino)
fn blocked(x: f64, z: f64) -> bool {
    (x > -23.2 && x < -1.8 && z > -22.8 && z < -7.9)
        || (x > 3.2 && x < 24.8 && z > -26.8 && z < -9.4)
        || ((x - GAZEBO.x) * (x - GAZEBO.x) + (z - GAZEBO.z) * (z - GAZEBO.z)) < 10.5
        || ((x - 13.0) * (x - 13.0) + (z + 4.0) * (z + 4.0)) < 1.4
        || (x > -0.9 && x < 2.6 && z > -16.5 && z < -13.8) // claro para el lector
        || ((x + 6.8) * (x + 6.8) + (z + 2.1) * (z + 2.1)) < 1.2 // alrededor del gato
}

// Ancla y escala de la casa gigante y del kiosco
const HOUSE_ANCHOR: Vec3 = Vec3::new(-13.0, 0.0, -9.0);
const HOUSE_SCALE: f64 = 1.8;
const GAZEBO: Vec3 = Vec3::new(1.2, 0.0, -19.5);
const GAZEBO_SCALE: f64 = 1.6;

// ---------- La casa amarilla de la familia (estilo occidental antiguo, como en la pelicula) ----------

fn window(b: &mut Builder, m: &Mats, x0: f64, x1: f64, y0: f64, y1: f64, z: f64, glass: &Material) {
    b.cube((x0 - 0.14, y0 - 0.14, z - 0.02), (x1 + 0.14, y1 + 0.14, z + 0.08), &m.trim);
    b.cube((x0, y0, z + 0.08), (x1, y1, z + 0.09), glass);
    for k in 1..3 {
        let xm = x0 + (x1 - x0) * k as f64 / 3.0;
        b.cube((xm - 0.025, y0, z + 0.08), (xm + 0.025, y1, z + 0.12), &m.wood);
    }
    for k in 1..4 {
        let y = y0 + (y1 - y0) * k as f64 / 4.0;
        b.cube((x0, y - 0.025, z + 0.08), (x1, y + 0.025, z + 0.12), &m.wood);
    }
    b.cube((x0 - 0.2, y0 - 0.26, z), (x1 + 0.2, y0 - 0.14, z + 0.25), &m.trim); // alfeizar
}

fn house(b: &mut Builder, lights: &mut Vec<Light>, m: &Mats, t: f64, station: f64) {
    b.cube((-18.2, 0.0, -16.2), (-7.8, 0.6, -8.8), &m.stone);
    b.cube((-18.0, 0.6, -16.0), (-8.0, 6.0, -9.0), &m.siding);
    // Esquineros y cornisa rojizos
    for (x, z) in [(-18.0, -9.0), (-8.0, -9.0), (-18.0, -16.0), (-8.0, -16.0)] {
        b.cube((x - 0.12, 0.6, z - 0.12), (x + 0.12, 6.0, z + 0.12), &m.trim);
    }
    b.cube((-14.8, 5.3, -8.6), (-11.6, 5.55, -8.4), &m.wood);

    // Techo de pizarra a dos aguas, hastiales y chimenea de ladrillo
    let s = 45f64.to_radians();
    b.rcube(v(-13.0, 7.535, -10.365), v(5.6, 0.12, 2.95), Mat3::rot_x(s), &m.slate);
    b.rcube(v(-13.0, 7.535, -14.635), v(5.6, 0.12, 2.95), Mat3::rot_x(-s), &m.slate);
    for x in [-17.925, -8.075] {
        b.rcube(v(x, 6.0, -12.5), v(0.075, 2.475, 2.475), Mat3::rot_x(s), &m.siding);
    }
    b.rcyl(v(-18.6, 9.55, -12.5), 0.16, 11.2, Mat3::rot_z(-PI / 2.0), &m.trim);
    b.cube((-11.0, 7.0, -14.2), (-10.1, 11.2, -13.3), &m.brick);
    b.cube((-11.1, 11.2, -14.3), (-10.0, 11.4, -13.2), &m.stone);

    // Fachada: vitral alto, puerta con escalones, ventana blanca
    window(b, m, -16.9, -15.6, 2.0, 4.8, -9.0, &m.stained);
    window(b, m, -11.2, -9.4, 2.2, 4.4, -9.0, &m.window);
    b.cube((-13.9, 0.6, -9.0), (-12.3, 3.6, -8.93), &m.trim);
    b.cube((-13.75, 0.6, -8.93), (-12.45, 3.45, -8.88), &m.door);
    b.ball(v(-12.7, 2.0, -8.82), 0.06, &solid(0xC8A040).specular(1.0, 128.0).reflectivity(0.4));
    b.cube((-14.3, 0.0, -8.8), (-11.9, 0.35, -7.9), &m.stone);
    b.rcube(v(-13.1, 3.9, -8.5), v(1.2, 0.06, 0.6), Mat3::rot_x(0.35), &m.slate); // tejadillo
    b.cube((-12.1, 2.6, -8.9), (-11.85, 3.0, -8.65), &solid(0xFFE0A0).glow(0.2 + 2.6 * station)); // farol
    warm_light(lights, b.map(v(-13.0, 3.0, -7.8)), t, 5.0 * station, 9.0);
    warm_light(lights, b.map(v(-10.3, 3.2, -8.0)), t, 4.0 * station, 8.0);
    warm_light(lights, b.map(v(-16.2, 3.2, -8.0)), t, 3.5 * station, 8.0);

    // Ventana salediza blanca en el costado derecho, con su techito
    b.cube((-8.0, 1.4, -13.2), (-6.9, 4.3, -10.8), &m.white);
    b.cube((-6.9, 1.9, -12.9), (-6.87, 3.9, -11.1), &m.window);
    b.cube((-6.87, 2.85, -12.9), (-6.84, 2.95, -11.1), &m.white);
    b.cube((-6.87, 1.9, -12.02), (-6.84, 3.9, -11.98), &m.white);
    b.rcube(v(-7.35, 4.5, -12.0), v(0.8, 0.07, 1.45), Mat3::rot_z(-0.45), &m.slate);
    warm_light(lights, b.map(v(-6.0, 3.0, -12.0)), t, 3.5 * station, 7.0);

    // Dos cuerpos salientes con hastiales; rompen la gran superficie plana del techo.
    for (cx, half, peak, front) in [(-16.45, 1.55, 8.6, -8.05), (-10.1, 1.40, 9.1, -8.05)] {
        let eave = peak - half;
        b.cube((cx - half, 0.6, front - 2.4), (cx + half, eave, front), &m.siding);
        let verts = [v(cx - half, eave, front), v(cx + half, eave, front), v(cx, peak, front)];
        b.triangle(verts, verts, &m.siding);
        for side in [-1.0, 1.0] {
            b.rcube(v(cx + side * half * 0.53, peak - half * 0.53, front - 1.0), v(half * 0.79, 0.085, 1.65), Mat3::rot_z(-side * PI / 4.0), &m.slate);
            b.rcube(v(cx + side * half * 0.53, peak - half * 0.53 - 0.08, front + 0.65), v(half * 0.81, 0.085, 0.09), Mat3::rot_z(-side * PI / 4.0), &m.wood);
            b.cube((cx + side * (half - 0.07) - 0.055, 0.6, front + 0.01), (cx + side * (half - 0.07) + 0.055, eave, front + 0.13), &m.wood);
        }
        window(b, m, cx - 0.95, cx + 0.95, 1.05, 3.15, front + 0.02, &m.window);
        window(b, m, cx - 0.95, cx + 0.95, 3.9, 6.15, front + 0.02, &m.window);
        b.cube((cx - half, 3.48, front + 0.01), (cx + half, 3.61, front + 0.19), &m.wood);
        b.cyl(v(cx, peak + 0.08, front + 0.1), 0.045, 0.42, &m.wood);
        b.cube((cx - 0.17, peak - 1.15, front + 0.01), (cx + 0.17, peak - 0.67, front + 0.07), &m.wood);
        for slat in 0..5 {
            let y = peak - 1.10 + slat as f64 * 0.085;
            b.cube((cx - 0.13, y, front + 0.07), (cx + 0.13, y + 0.027, front + 0.09), &m.white);
        }
        b.rell(v(cx, peak + 0.48, front + 0.1), v(0.075, 0.15, 0.075), Mat3::IDENTITY, &m.trim);
        flower_box(b, m, v(cx, 3.63, front + 0.3), 1.9, (cx.abs() * 21.0) as u64);
        flower_box(b, m, v(cx, 0.84, front + 0.3), 1.9, (cx.abs() * 31.0) as u64);
        for side in [-1.0, 1.0] {
            climbing_vine(b, m, v(cx + side * 1.17, 0.7, front + 0.18), 5.6, (cx.abs() * 71.0) as u64 + (side + 1.0) as u64);
        }
    }
    // Buhardilla central, con carpinteria y tejadillo propio.
    b.cube((-13.75, 6.2, -10.9), (-12.45, 7.65, -9.6), &m.siding);
    let dv = [v(-13.75, 7.65, -9.59), v(-12.45, 7.65, -9.59), v(-13.1, 8.3, -9.59)];
    b.triangle(dv, dv, &m.siding);
    for side in [-1.0, 1.0] {
        b.rcube(v(-13.1 + side * 0.4, 7.96, -10.2), v(0.58, 0.075, 0.92), Mat3::rot_z(-side * PI / 4.0), &m.slate);
    }
    window(b, m, -13.5, -12.7, 6.48, 7.5, -9.55, &m.window);
    // Porche de entrada con columnas de piedra, vigas, peldaños y fronton.
    for k in 0..4 {
        let step = k as f64;
        b.cube((-14.65, step * 0.12, -8.6), (-11.55, (step + 1.0) * 0.12, -6.25 - step * 0.28), &m.stone);
    }
    for x in [-14.3, -11.9] {
        b.cube((x - 0.17, 0.48, -7.3), (x + 0.17, 3.35, -6.96), &m.stone);
        b.cube((x - 0.24, 3.3, -7.38), (x + 0.24, 3.5, -6.9), &m.white);
        b.cube((x - 0.25, 0.48, -7.4), (x + 0.25, 0.7, -6.88), &m.stone);
    }
    for side in [-1.0, 1.0] {
        b.rcube(v(-13.1 + side * 0.76, 3.88, -7.75), v(1.08, 0.085, 1.12), Mat3::rot_z(-side * 0.55), &m.slate);
        b.rcube(v(-13.1 + side * 0.76, 3.78, -6.61), v(1.1, 0.085, 0.085), Mat3::rot_z(-side * 0.55), &m.wood);
    }
    b.cube((-14.5, 3.34, -7.15), (-11.7, 3.48, -6.95), &m.wood);
    b.cyl(v(-13.1, 3.0, -6.9), 0.025, 0.6, &m.wood);
    b.cube((-13.22, 2.75, -7.03), (-12.98, 3.06, -6.79), &solid(0xFFD48A).glow(0.15 + station * 2.0));
    for (i, x) in [-14.8, -11.4].iter().enumerate() {
        b.cyl(v(*x, 0.45, -6.9), 0.25, 0.38, &solid(0xA45C38));
        b.cyl(v(*x, 0.79, -6.9), 0.28, 0.09, &solid(0xBC794B));
        hydrangea(b, m, v(*x, 0.87, -6.9), 0.35, (0xBC72B0, 0xF5DDE8), i as u64 + 800);
    }
    // Mensulas bajo los aleros y bajantes.
    for k in 0..9 {
        let x = -14.7 + k as f64 * 0.36;
        b.cube((x, 5.15, -8.68), (x + 0.09, 5.48, -8.32), &m.wood);
    }
    for x in [-18.35, -7.7] { b.cyl(v(x, 0.2, -8.55), 0.055, 5.35, &m.trim); }

}

fn flower_box(b: &mut Builder, m: &Mats, p: Vec3, width: f64, seed: u64) {
    b.cube((p.x - width / 2.0, p.y - 0.18, p.z - 0.2), (p.x + width / 2.0, p.y, p.z + 0.2), &m.wood);
    for i in 0..9u64 {
        let q = p + v(-width * 0.45 + width * 0.9 * i as f64 / 8.0, 0.0, 0.03);
        blossom(b, q + v(0.0, 0.18 + h1(seed + i) * 0.15, 0.0), 0.085, seed + i, &solid(if i % 2 == 0 { 0xE3AAC1 } else { 0xF4E9D2 }));
        b.leaf(q + v(0.0, 0.08, 0.0), 0.17, 0.065, Mat3::rot_y(i as f64 * 2.4).mul(&Mat3::rot_z(0.4)), &m.ivy);
    }
}

fn climbing_vine(b: &mut Builder, m: &Mats, p: Vec3, height: f64, seed: u64) {
    let mut prev = p;
    for i in 0..48u64 {
        let y = height * i as f64 / 47.0;
        let q = p + v((y * 2.1 + seed as f64).sin() * 0.22, y, 0.0);
        branch(b, m, prev, q, 0.012);
        for side in [-1.0, 1.0] {
            let r = Mat3::rot_x(PI / 2.0).mul(&Mat3::rot_y(side * 0.7 + (h1(i + seed) - 0.5) * 1.8));
            b.leaf(q + v(side * (0.08 + h1(i + seed * 7) * 0.22), h1(i + seed * 11) * 0.13, 0.045), 0.18 + h1(i + seed) * 0.05, 0.105, r, &m.ivy);
        }
        prev = q;
    }
}

// Roca irregular: tres elipsoides girados que se funden en una sola forma
fn rock(b: &mut Builder, m: &Mats, c: Vec3, r: f64, seed: u64) {
    for k in 0..3u64 {
        let off = v(h1(seed * 3 + k) - 0.5, (h1(seed * 5 + k) - 0.5) * 0.4, h1(seed * 7 + k) - 0.5) * r;
        let rot = Mat3::rot_y(h1(seed + k) * 6.0).mul(&Mat3::rot_z((h1(seed * 11 + k) - 0.5) * 0.6));
        b.rell(c + off, v(r * (0.9 + 0.5 * h1(seed + k + 1)), r * (0.55 + 0.3 * h1(seed + k + 2)), r * (0.8 + 0.3 * h1(seed + k + 3))), rot, &m.rock);
    }
}

// ---------- Kiosco hexagonal junto al lago ----------

fn gazebo(b: &mut Builder, lights: &mut Vec<Light>, m: &Mats, c: Vec3, t: f64, station: f64) {
    b.cyl(c, 1.75, 0.35, &m.stone);
    for k in 0..6 {
        let a = k as f64 * PI / 3.0;
        b.cyl(c + v(a.cos() * 1.4, 0.35, a.sin() * 1.4), 0.09, 2.6, &m.wood);
        if k != 1 {
            // Barandal entre postes (la entrada queda libre)
            let a2 = a + PI / 3.0;
            let mid = (v(a.cos(), 0.0, a.sin()) + v(a2.cos(), 0.0, a2.sin())) * 0.7;
            b.rcube(c + mid + v(0.0, 1.2, 0.0), v(0.7, 0.04, 0.04), Mat3::rot_y(-(a + PI / 6.0) - PI / 2.0), &m.wood);
        }
    }
    let tilt = 35f64.to_radians();
    for k in 0..6 {
        let a = k as f64 * PI / 3.0 + PI / 6.0;
        let rot = Mat3::rot_y(-a).mul(&Mat3::rot_z(-tilt));
        b.rcube(c + v(a.cos() * 0.95, 3.63, a.sin() * 0.95), v(1.15, 0.06, 1.0), rot, &m.slate);
    }
    b.cyl(c + v(0.0, 4.25, 0.0), 0.06, 0.4, &m.trim);
    b.ball(c + v(0.0, 4.7, 0.0), 0.1, &m.trim);
    // Farolito colgando del techo
    b.cyl(c + v(0.0, 2.5, 0.0), 0.01, 0.8, &solid(0x2A2A2A));
    b.cube((c.x - 0.15, 2.2, c.z - 0.15), (c.x + 0.15, 2.5, c.z + 0.15), &solid(0xFFD89A).glow(0.15 + 2.8 * station));
    warm_light(lights, b.map(c + v(0.0, 2.1, 0.0)), t, 3.5 * station, 7.0);
}

// ---------- Arboles ----------

// Alcanforero enorme: tronco, ramas y copa de muchas matas pintadas
fn big_tree(b: &mut Builder, m: &Mats, base: Vec3, h: f64, spread: f64, seed: u64) {
    let crown = base + v(0.35, h, 0.15);
    for k in 0..7 {
        let u = k as f64 / 7.0;
        let a = base + v(0.35 * u, h * u, 0.15 * u);
        let end = base + v(0.35 * (u + 1.0 / 7.0), h * (u + 1.0 / 7.0), 0.15 * (u + 1.0 / 7.0));
        tapered_branch(b, m, a, end, 0.70 * (1.0 - u * 0.72), 0.70 * (1.0 - (u + 1.0 / 7.0) * 0.72));
    }
    for k in 0..15u64 {
        let angle = k as f64 * 2.399 + h1(seed) * 6.0;
        let rad = spread * (0.45 + 0.48 * h1(seed * 19 + k));
        let joint = base + v(0.2, h * (0.56 + h1(seed + k) * 0.30), 0.0);
        let tip = crown + v(angle.cos() * rad, -0.9 + h1(seed * 31 + k) * 3.2, angle.sin() * rad);
        let elbow = joint.lerp(tip, 0.55) - v(0.0, 0.35, 0.0);
        branch(b, m, joint, elbow, 0.12);
        branch(b, m, elbow, tip, 0.065);
        for twig in 0..9u64 {
            let ph = angle + (h1(seed + k * 91 + twig) - 0.5) * 2.4;
            let twig_end = tip + v(ph.cos() * 1.5, (h1(k * 71 + twig) - 0.2) * 1.4, ph.sin() * 1.5);
            branch(b, m, elbow.lerp(tip, 0.5 + 0.5 * h1(twig + k)), twig_end, 0.018);
            for j in 0..32u64 {
                let id = seed * 13001 + k * 997 + twig * 37 + j;
                let q = twig_end + v((h1(id) - 0.5) * 1.5, (h1(id + 1) - 0.5) * 0.8, (h1(id + 2) - 0.5) * 1.5);
                let len = 0.18 + h1(id + 3) * 0.17;
                b.leaf(q, len, len * 0.38, Mat3::rot_y(h1(id + 4) * 6.28).mul(&Mat3::rot_z((h1(id + 5) - 0.5) * 1.5)), if j % 4 == 0 { &m.leaves_dark } else { &m.leaves });
            }
        }
    }
    for k in 0..7 {
        let a = k as f64 * 2.4 + seed as f64;
        branch(b, m, base + v(0.0, 0.35, 0.0), base + v(a.cos() * 1.8, 0.04, a.sin() * 1.8), 0.16);
    }
}

// Ramas entre dos puntos.

fn branch(b: &mut Builder, m: &Mats, a: Vec3, end: Vec3, radius: f64) {
    let d = end - a;
    let len = d.length();
    if len < 0.0001 { return; }
    let rot = Mat3::rot_y(-d.z.atan2(d.x)).mul(&Mat3::rot_z(-(d.y / len).clamp(-1.0, 1.0).acos()));
    b.rcyl(a, radius, len, rot, &m.bark);
}

fn tapered_branch(b: &mut Builder, m: &Mats, a: Vec3, end: Vec3, r0: f64, r1: f64) {
    let dir = (end - a).unit();
    let right = dir.cross(v(0.0, 0.0, 1.0)).unit();
    let forward = right.cross(dir).unit();
    for k in 0..12 {
        let t0 = k as f64 * PI / 6.0;
        let t1 = (k + 1) as f64 * PI / 6.0;
        let n0 = right * t0.cos() + forward * t0.sin();
        let n1 = right * t1.cos() + forward * t1.sin();
        let corners = [a + n0 * r0, a + n1 * r0, end + n1 * r1, end + n0 * r1];
        for ids in [[0, 1, 2], [0, 2, 3]] {
            let tri = ids.map(|i| corners[i]);
            b.triangle(tri, tri, &m.bark);
        }
    }
}

// Helechos con frondas arqueadas y foliolos, agrupados en lugares humedos.
fn fern(b: &mut Builder, m: &Mats, p: Vec3, size: f64, seed: u64) {
    for frond in 0..7u64 {
        let angle = frond as f64 * 2.399 + h1(seed) * 6.0;
        let rot = Mat3::rot_y(-angle);
        let mut prev = p;
        for step in 1..9 {
            let u = step as f64 / 9.0;
            let q = p + rot.mul_vec(v(u * size, size * (1.6 * u - u * u), 0.0));
            branch(b, m, prev, q, 0.008 * size);
            for side in [-1.0, 1.0] {
                let width = (1.0 - u) * size * 0.23;
                b.rell(q + rot.mul_vec(v(-width * 0.2, 0.0, side * width * 0.65)), v(width * 0.45, 0.012 * size, width), rot.mul(&Mat3::rot_y(side * 0.45)), &m.ivy);
            }
            prev = q;
        }
    }
}

// Pino japones con copas planas en capas
fn pine(b: &mut Builder, m: &Mats, base: Vec3, seed: u64) {
    tapered_branch(b, m, base, base + v(0.4, 8.5, 0.0), 0.38, 0.035);
    for k in 0..27u64 {
        let y = 2.0 + k as f64 * 0.23;
        let angle = k as f64 * 2.399 + seed as f64;
        let length = 3.0 * (1.0 - y / 10.0);
        let end = base + v(angle.cos() * length, y + 0.1, angle.sin() * length);
        branch(b, m, base + v(0.2, y, 0.0), end, 0.035);
        for j in 0..100u64 {
            let id = seed * 113 + k * 101 + j;
            let q = end + v((h1(id) - 0.5) * 1.3, (h1(id + 1) - 0.5) * 0.4, (h1(id + 2) - 0.5) * 1.3);
            b.leaf(q, 0.25, 0.025, Mat3::rot_y(h1(id + 3) * 6.28).mul(&Mat3::rot_z(h1(id + 4) * 0.7)), &m.pine);
        }
    }
}

// Bosque de fondo con hojas individuales.
fn far_tree(b: &mut Builder, m: &Mats, base: Vec3, h: f64, seed: u64) {
    branch(b, m, base, base + v(0.5, h * 0.8, 0.0), 0.28);
    for k in 0..26u64 {
        let a = k as f64 * 2.399 + seed as f64;
        let r = 1.5 + h1(seed + k) * 2.0;
        let end = base + v(a.cos() * r, h * (0.45 + 0.5 * h1(seed + k + 50)), a.sin() * r);
        branch(b, m, base + v(0.2, h * 0.45, 0.0), end, 0.045);
        for j in 0..90u64 {
            let id = seed * 127 + k * 131 + j;
            let q = end + v((h1(id) - 0.5) * 2.0, (h1(id + 1) - 0.5) * 1.7, (h1(id + 2) - 0.5) * 2.0);
            b.leaf(q, 0.4 + h1(id + 3) * 0.3, 0.22, Mat3::rot_y(h1(id + 4) * 6.28).mul(&Mat3::rot_z(h1(id + 5) - 0.5)), &m.leaves_far);
        }
    }
}

// Gato atigrado.

fn cat(b: &mut Builder, _m: &Mats, base: Vec3, facing: f64, s: f64, t: f64, station: f64) {
    let fur = Material::new(Texture::Fur { base: hex(0xCE8944), stripe: hex(0x925024), belly: hex(0xF3E4C8) }).specular(0.12, 12.0);
    let r = Mat3::rot_y(facing);
    let at = |p: Vec3| base + r.mul_vec(p * s);
    let cream = solid(0xEDE4D6).specular(0.1, 8.0);
    let pink = solid(0xE8A0A0).specular(0.3, 32.0);
    b.rell(at(v(0.0, 1.1, 0.0)), v(0.85, 1.1, 0.8) * s, r, &fur);
    b.rell(at(v(0.0, 1.35, 0.45)), v(0.6, 0.8, 0.5) * s, r, &fur);
    for sx in [-0.55, 0.55] {
        b.rell(at(v(sx, 0.55, -0.1)), v(0.5, 0.55, 0.75) * s, r, &fur);
        b.rcyl(at(v(sx * 0.55, 0.05, 0.55)), 0.16 * s, 1.2 * s, r, &fur);
        b.rell(at(v(sx * 0.55, 0.08, 0.72)), v(0.2, 0.1, 0.27) * s, r, &cream);
    }
    // Cola que se mece alrededor del cuerpo
    let sway = (t * 1.4).sin() * 0.5;
    for i in 0..10 {
        let k = i as f64 / 9.0;
        let ang = -PI / 2.0 - 0.3 + k * 2.1 + sway * k * k;
        let p = v(1.05 * ang.cos(), 0.18 + 0.12 * k * k + 0.25 * (k * PI).sin() * sway.abs(), 0.95 * ang.sin() - 0.1);
        b.ball(at(p), (0.17 - 0.05 * k) * s, &fur);
    }
    // Cabeza que gira; orejas que se mueven; parpadeo
    let turn = 0.3 * (t * 0.45).sin();
    let hr = r.mul(&Mat3::rot_y(turn));
    let pivot = v(0.0, 2.2, 0.3);
    let hat = |p: Vec3| base + r.mul_vec(pivot * s) + hr.mul_vec((p - pivot) * s);
    b.rell(hat(v(0.0, 2.55, 0.35)), v(0.72, 0.62, 0.62) * s, hr, &fur);
    b.rell(hat(v(0.0, 2.36, 0.88)), v(0.3, 0.2, 0.18) * s, hr, &cream);
    b.rell(hat(v(0.0, 2.5, 1.05)), v(0.08, 0.055, 0.045) * s, hr, &pink);
    let twitch = if (t * 0.7).fract() < 0.05 { 0.25 } else { 0.0 };
    let blink = if (t / 4.0).fract() > 0.965 { 0.15 } else { 1.0 };
    let eye = solid(0x9AC050).specular(1.0, 200.0).reflectivity(0.25).glow(0.05 + 0.9 * station);
    for sx in [-1.0, 1.0] {
        let ear = hr.mul(&Mat3::rot_z(-sx * (0.35 + twitch)));
        b.rell(hat(v(sx * 0.4, 3.05, 0.3)), v(0.24, 0.34, 0.09) * s, ear, &fur);
        b.rell(hat(v(sx * 0.4, 3.0, 0.36)), v(0.14, 0.22, 0.05) * s, ear, &pink);
        b.rell(hat(v(sx * 0.26, 2.66, 0.86)), v(0.13, 0.14 * blink, 0.07) * s, hr, &eye);
        b.rell(hat(v(sx * 0.26, 2.66, 0.92)), v(0.03, 0.11 * blink, 0.03) * s, hr, &solid(0x0A0A0A).specular(1.0, 200.0));
    }
}

// ---------- Pasto alto y flores silvestres ----------

// Mata de briznas delgadas inclinadas en distintas direcciones, meciendose con el viento
fn grass_tuft(b: &mut Builder, m: &Mats, p: Vec3, seed: u64, t: f64) {
    let mat = &m.blades[(h1(seed) * 3.0) as usize % 3];
    let wind = 0.12 * (t * 1.6 + p.x * 0.35 + p.z * 0.2).sin();
    for k in 0..4u64 {
        let h = 0.35 + 0.6 * h1(seed * 5 + k);
        let rot = Mat3::rot_y(h1(seed * 3 + k) * 2.0 * PI).mul(&Mat3::rot_z(0.15 + 0.35 * h1(seed * 7 + k) + wind));
        let off = v(0.12 * (h1(seed + k * 11) - 0.5), 0.0, 0.12 * (h1(seed + k * 13) - 0.5));
        b.leaf(p + off + rot.mul_vec(v(0.0, h * 0.5, 0.0)), h * 0.5, 0.016, rot.mul(&Mat3::rot_z(PI / 2.0)), mat);
    }
}

// Flor silvestre: amapola naranja, rudbeckia amarilla, ramillete rosa/blanco o cardo morado
fn blossom(b: &mut Builder, top: Vec3, radius: f64, seed: u64, petal: &Material) {
    let tilt = Mat3::rot_z((h1(seed + 71) - 0.5) * 0.65);
    let count = if seed % 3 == 0 { 6 } else { 10 };
    for j in 0..count {
        let angle = j as f64 * 2.0 * PI / count as f64;
        let rot = tilt.mul(&Mat3::rot_y(angle)).mul(&Mat3::rot_z(-0.18));
        b.petal(top + tilt.mul_vec(v(angle.cos(), 0.0, -angle.sin()) * radius * 0.52), radius * 0.54, radius * 0.24, rot, petal);
    }
    b.rell(top + tilt.mul_vec(v(0.0, radius * 0.10, 0.0)), v(radius * 0.23, radius * 0.11, radius * 0.23), tilt, &solid(0xBD8A29));
}

fn wildflower(b: &mut Builder, m: &Mats, p: Vec3, seed: u64, t: f64) {
    let h = 0.35 + 0.6 * h1(seed + 2);
    let wind = 0.05 * (t * 1.6 + p.x * 0.35 + p.z * 0.2).sin();
    let top = p + v(wind, h, wind * 0.5);
    b.rcyl(p, 0.009, h, Mat3::rot_z(-wind / h), &m.blades[2]);
    for k in 0..3 {
        let angle = seed as f64 + k as f64 * 2.4;
        b.leaf(p + v(angle.cos() * 0.075, h * (0.22 + k as f64 * 0.16), angle.sin() * 0.075), 0.14, 0.035, Mat3::rot_y(-angle).mul(&Mat3::rot_z(0.4)), &m.ivy);
    }
    let kind = seed % 5;
    if kind == 4 {
        // Espiga de lavanda: verticilos escalonados con pequenas corolas.
        for j in 0..7u64 {
            for side in 0..3 {
                let a = side as f64 * 2.1 + j as f64 * 0.6;
                let q = top + v(a.cos() * 0.035, j as f64 * 0.033, a.sin() * 0.035);
                blossom(b, q, 0.028 * (1.0 - j as f64 * 0.06), seed + j, &solid(0x977ABF));
            }
        }
    } else {
        let color = [0xF5F0DD, 0xE5B641, 0xD67C9E, 0xD7583B][kind as usize];
        blossom(b, top, 0.12 + h1(seed + 7) * 0.07, seed, &solid(color).specular(0.08, 16.0));
    }
}

// Hortensias: ramas con hojas nervadas y cabezas de pequenas flores de cuatro petalos.

fn hydrangea(b: &mut Builder, m: &Mats, c: Vec3, size: f64, colors: (u32, u32), seed: u64) {
    let (a, bb) = (solid(colors.0).specular(0.08, 24.0), solid(colors.1).specular(0.08, 24.0));
    for k in 0..7u64 {
        let ang = h1(seed * 13 + k) * 2.0 * PI;
        let rr = size * (0.25 + 0.55 * h1(seed * 17 + k));
        let head = c + v(ang.cos() * rr, size * (0.7 + 0.4 * h1(seed * 19 + k)), ang.sin() * rr);
        branch(b, m, c, head, size * 0.018);
        for l in 0..5u64 {
            let angle = ang + l as f64 * 2.4;
            let pos = c.lerp(head, 0.35 + l as f64 * 0.10) + v(angle.cos(), 0.0, angle.sin()) * size * 0.17;
            b.leaf(pos, size * 0.28, size * 0.14, Mat3::rot_y(-angle).mul(&Mat3::rot_z(0.35)), &m.ivy);
        }
        for j in 0..19u64 {
            let y = 1.0 - (j as f64 + 0.5) / 19.0 * 1.45;
            let rad = (1.0 - y * y).max(0.0).sqrt();
            let th = j as f64 * 2.39996 + k as f64;
            let center = head + v(rad * th.cos(), y, rad * th.sin()) * (size * 0.23);
            let mat = if (j + k) % 3 == 0 { &bb } else { &a };
            for pet in 0..4 {
                let angle = pet as f64 * PI * 0.5 + th;
                b.petal(center + v(angle.cos(), 0.05, angle.sin()) * size * 0.037, size * 0.045, size * 0.035, Mat3::rot_y(-angle).mul(&Mat3::rot_z(0.2)), mat);
            }
        }
    }
}

// Hiedra sobre la pared, con hojas puntiagudas.

fn ivy_patch(b: &mut Builder, m: &Mats, x0: f64, x1: f64, y0: f64, y1: f64, z: f64, on_x: bool, n: u64, seed: u64) {
    for i in 0..n * 3 {
        let a = h1(seed + i * 3);
        let w = x0 + (x1 - x0) * h1(seed + i * 3 + 1);
        let y = y0 + (y1 - y0) * a.powf(1.6);
        let r = Mat3::rot_x(PI / 2.0).mul(&Mat3::rot_y((h1(seed + i * 3 + 2) - 0.5) * 2.0));
        let rot = if on_x { Mat3::rot_y(PI / 2.0).mul(&r) } else { r };
        let q = if on_x { v(z, y, w) } else { v(w, y, z) };
        b.leaf(q, 0.20, 0.12, rot, &m.ivy);
    }
}

// Entrada diminuta de Arrietty.

// Segmento orientado para brazos, costuras y objetos de costura.
fn rod(b: &mut Builder, a: Vec3, end: Vec3, radius: f64, mat: &Material) {
    let d = end - a;
    let len = d.length();
    if len < 1e-8 { return; }
    let rot = Mat3::rot_y(-d.z.atan2(d.x)).mul(&Mat3::rot_z(-(d.y / len).clamp(-1.0, 1.0).acos()));
    b.rcyl(a, radius, len, rot, mat);
}

fn arrietty_figure(b: &mut Builder, base: Vec3, s: f64, t: f64) {
    let p = |x, y, z| base + v(x, y, z) * s;
    let skin = solid(0xF3CFAC).specular(0.08, 16.0);
    let dress = solid(0xB42E42).specular(0.06, 12.0);
    let hair = Material::new(Texture::Planks { a: hex(0x743E25), b: hex(0x9F6038), width: 0.018 }).specular(0.15, 24.0);
    let boot = solid(0x63482E);
    for side in [-1.0, 1.0] {
        rod(b, p(side * 0.085, 0.055, 0.0), p(side * 0.075, 0.39, 0.0), 0.038 * s, &skin);
        b.ell(p(side * 0.085, 0.055, 0.035), v(0.055, 0.052, 0.092) * s, &boot);
    }
    // Falda acampanada con pliegues.
    for k in 0..24 {
        let a = k as f64 * PI / 12.0;
        let a2 = (k + 1) as f64 * PI / 12.0;
        let point = |angle: f64, upper: bool| {
            let r = if upper { 0.13 } else { 0.235 + 0.012 * (angle * 12.0).cos() };
            p(r * angle.cos(), if upper { 0.67 } else { 0.30 + 0.012 * (angle * 3.0).sin() }, r * angle.sin())
        };
        let q = [point(a, false), point(a2, false), point(a2, true), point(a, true)];
        for ids in [[0, 1, 2], [0, 2, 3]] { let tri = ids.map(|i| q[i]); b.triangle(tri, tri, &dress); }
    }
    b.ell(p(0.0, 0.78, 0.0), v(0.135, 0.18, 0.095) * s, &dress);
    b.cyl(p(0.0, 0.65, 0.0), 0.134 * s, 0.035 * s, &solid(0x882736));
    b.cyl(p(0.0, 0.91, 0.0), 0.048 * s, 0.08 * s, &skin);
    // Un brazo levantado y el otro sujetando el alfiler.
    rod(b, p(-0.12, 0.88, 0.0), p(-0.27, 1.10, 0.0), 0.043 * s, &dress);
    rod(b, p(-0.27, 1.10, 0.0), p(-0.31, 1.38, 0.02), 0.035 * s, &dress);
    b.ell(p(-0.31, 1.43, 0.025), v(0.040, 0.064, 0.025) * s, &skin);
    rod(b, p(0.12, 0.88, 0.0), p(0.24, 0.71, 0.02), 0.043 * s, &dress);
    rod(b, p(0.24, 0.71, 0.02), p(0.36, 0.78, 0.10), 0.031 * s, &skin);
    b.ball(p(0.36, 0.78, 0.10), 0.035 * s, &skin);
    b.ell(p(0.0, 1.10, 0.0), v(0.155, 0.185, 0.14) * s, &skin);
    b.ell(p(0.0, 1.18, -0.05), v(0.17, 0.15, 0.14) * s, &hair);
    for k in 0..5 {
        b.rell(p(-0.13 + k as f64 * 0.052, 1.225 - 0.018 * (k % 2) as f64, 0.095), v(0.042, 0.060, 0.038) * s, Mat3::rot_z(-0.4), &hair);
    }
    // Coleta lateral visible y pinza rosa de dos brazos.
    b.rell(p(-0.17, 1.19, -0.12), v(0.11, 0.19, 0.09) * s, Mat3::rot_z(-0.6), &hair);
    b.rell(p(-0.24, 1.02, -0.11), v(0.075, 0.15, 0.065) * s, Mat3::rot_z(-0.5), &hair);
    let clip = solid(0xD9867D).specular(0.1, 20.0);
    for side in [-1.0, 1.0] {
        let rot = Mat3::rot_z(-side * 0.16);
        b.rcube(p(side * 0.055 - 0.025, 1.39, -0.035), v(0.037, 0.13, 0.028) * s, rot, &clip);
        for mark in 0..3 {
            b.rcube(p(side * 0.055 - 0.025, 1.39 + mark as f64 * 0.035, -0.004), v(0.022, 0.005, 0.003) * s, rot, &solid(0xA95753));
        }
        let blink = blink_open(t, 1.7);
        if blink > 0.12 {
        b.ell(p(side * 0.061, 1.11, 0.127), v(0.037, 0.046 * blink, 0.018) * s, &solid(0xFFF8E9));
        b.ell(p(side * 0.061, 1.109, 0.145), v(0.017, 0.030 * blink, 0.008) * s, &solid(0x4B4936));
        b.ell(p(side * 0.061, 1.110, 0.151), v(0.008, 0.023 * blink, 0.004) * s, &solid(0x211D19));
        b.ball(p(side * 0.061 - 0.005, 1.123, 0.156), 0.006 * s, &solid(0xFFFFFF));
        } else {
            rod(b, p(side * 0.061 - 0.026, 1.10, 0.145), p(side * 0.061 + 0.026, 1.10, 0.145), 0.007 * s, &hair);
        }
        rod(b, p(side * 0.061 - 0.030, 1.157, 0.123), p(side * 0.061 + 0.027, 1.160, 0.123), 0.007 * s, &hair);
        b.ell(p(side * 0.150, 1.09, 0.0), v(0.027, 0.043, 0.019) * s, &skin);
    }
    b.ell(p(0.0, 1.06, 0.144), v(0.018, 0.026, 0.020) * s, &skin);
    b.ell(p(0.0, 1.01, 0.121), v(0.025, 0.006, 0.007) * s, &solid(0xA46154));
    rod(b, p(-0.10, 0.89, 0.10), p(0.14, 0.59, 0.12), 0.012 * s, &solid(0xA58B56));
    b.ell(p(0.18, 0.57, 0.025), v(0.080, 0.10, 0.052) * s, &solid(0xAC8C51));
    rod(b, p(0.16, 0.25, 0.11), p(0.43, 0.94, 0.11), 0.008 * s, &solid(0xD3D7DA).specular(1.0, 200.0).reflectivity(0.65));
    b.ball(p(0.43, 0.94, 0.11), 0.055 * s, &solid(0xD8BC65).specular(0.9, 96.0).reflectivity(0.25));
}

fn borrowers_corner(b: &mut Builder, lights: &mut Vec<Light>, m: &Mats, station: f64) {
    let z = -7.15; // cara del cimiento
    let x = -6.0;
    // Puertita de madera con marco de piedritas y un farolito diminuto
    b.cube((x - 0.055, 0.0, z), (x + 0.055, 0.15, z + 0.012), &solid(0x2A1A10));
    b.cube((x - 0.045, 0.005, z + 0.012), (x + 0.045, 0.14, z + 0.02), &m.door);
    b.ball(v(x + 0.03, 0.07, z + 0.024), 0.005, &solid(0xC8A040).specular(1.0, 128.0).reflectivity(0.4));
    for k in 0..7 {
        let a = PI * k as f64 / 6.0;
        b.ell(v(x + a.cos() * 0.07, 0.075 + a.sin() * 0.085, z + 0.012), v(0.018, 0.014, 0.01), &m.stone);
    }
    b.cube((x + 0.07, 0.1, z + 0.012), (x + 0.085, 0.125, z + 0.03), &solid(0xFFD89A).glow(0.6 + 2.5 * station));
    lights.push(Light::point_range(v(x + 0.078, 0.11, z + 0.06), Color::new(1.0, 0.68, 0.38), 0.05 + 0.15 * station, 0.7));
    // Escalera hecha con fosforos
    for k in 0..4 {
        b.cube((x - 0.12, 0.0, z + 0.02 + k as f64 * 0.025), (x - 0.07, 0.035 - k as f64 * 0.008, z + 0.045 + k as f64 * 0.025), &solid(0xE8D0A0));
    }
    // Arrietty (10 cm): vestido rojo, pinza de ropa en el pelo, alfiler como espada
    let a = v(x + 0.02, 0.0, z + 0.14);
    // El personaje esta oculto en las plantas junto al lector; aqui queda su entrada.
    // Luz de relleno local: permite leer el rostro bajo las plantas gigantes.
    lights.push(Light::point_range(v(x - 0.09, 0.23, z + 0.38), Color::new(1.0, 0.86, 0.69), 0.8, 0.65));
    // Carrete de hilo humano, boton de cuatro agujeros y sobre junto a la entrada.
    let spool = v(x - 0.11, 0.0, z + 0.10);
    let wood = solid(0xC89659);
    b.cyl(spool, 0.029, 0.006, &wood);
    b.cyl(spool + v(0.0, 0.006, 0.0), 0.022, 0.050, &solid(0xBD6F54));
    for k in 0..18 { b.cyl(spool + v(0.0, 0.007 + k as f64 * 0.0026, 0.0), 0.023, 0.0008, &solid(0xDBA08A)); }
    b.cyl(spool + v(0.0, 0.056, 0.0), 0.029, 0.006, &wood);
    b.cyl(spool + v(0.0, 0.062, 0.0), 0.004, 0.0006, &solid(0x3C2719));
    let button = v(x + 0.09, 0.004, z + 0.23);
    b.cyl(button, 0.023, 0.004, &solid(0x73989C).specular(0.7, 96.0));
    for dx in [-0.006, 0.006] { for dz in [-0.006, 0.006] {
        b.cyl(button + v(dx, 0.0041, dz), 0.0025, 0.0003, &solid(0x172726));
    }}
    b.cube((x - 0.23, 0.002, z + 0.05), (x - 0.135, 0.004, z + 0.115), &solid(0xE5D6AD));
    b.ell(v(x, -0.004, z + 0.17), v(0.25, 0.006, 0.19), &Material::new(Texture::Stucco(hex(0x70573C))));
    // Cubo de azucar (el regalo de Sho) y una hoja grande recargada
    b.cube((x - 0.2, 0.0, z + 0.1), (x - 0.17, 0.03, z + 0.13), &Material::new(Texture::Stone { base: hex(0xF8F6F0), mortar: hex(0xDCD8CE), scale: 60.0 }).specular(0.6, 64.0).transparency(0.25, 1.55));
    b.rell(v(x + 0.16, 0.06, z + 0.1), v(0.02, 0.09, 0.05), Mat3::rot_z(0.4), &Material::new(Texture::Leaf { a: hex(0x4E8A32), b: hex(0x86B846) }).specular(0.4, 48.0));
    // Treboles y briznas alrededor, gigantes a su escala
    for k in 0..3 {
        let ang = k as f64 * 2.1 + 0.4;
        b.rell(v(x - 0.25 + ang.cos() * 0.035, 0.09, z + 0.3 + ang.sin() * 0.035), v(0.04, 0.004, 0.034), Mat3::rot_y(-ang), &m.leaves);
    }
    b.cyl(v(x - 0.25, 0.0, z + 0.3), 0.004, 0.09, &m.blades[0]);

    // Pasto de tamano normal alrededor: a la escala de Arrietty son arboles
    let cam_dir = v(0.34, 0.0, 0.94); // hacia donde queda la camara de su vista: ahi no se pone pasto
    for i in 0..70u64 {
        let q = v(x - 1.3 + h1(i + 12000) * 2.6, 0.0, z + 0.06 + h1(i + 12001) * 1.5);
        let rel = q - a;
        let along = rel.dot(cam_dir);
        let side = (rel - cam_dir * along).length();
        if (along > -0.05 && side < 0.25 + along * 0.35) || rel.length() < 0.12 {
            continue;
        }
        grass_tuft(b, m, q, i + 12100, 0.0);
    }
    // Piedritas, petalos caidos y una gota de rocio (refracta como una lupa)
    for i in 0..14u64 {
        let q = v(x - 0.5 + h1(i + 12200) * 1.0, 0.0, z + 0.04 + h1(i + 12201) * 0.6);
        let r = 0.008 + 0.015 * h1(i + 12202);
        b.rell(q + v(0.0, r * 0.4, 0.0), v(r * 1.3, r * 0.7, r), Mat3::rot_y(h1(i) * 6.0), &m.stone);
    }
    for i in 0..6u64 {
        let q = v(x - 0.4 + h1(i + 12300) * 0.8, 0.002, z + 0.08 + h1(i + 12301) * 0.4);
        b.rell(q, v(0.018, 0.002, 0.011), Mat3::rot_y(h1(i + 12302) * 6.0), &solid(0xF4A8C8).specular(0.3, 32.0));
    }
    b.ell(v(x - 0.1, 0.012, z + 0.2), v(0.014, 0.011, 0.014), &solid(0xF0F8FF).diffuse(0.1).specular(1.0, 400.0).reflectivity(0.05).transparency(0.9, 1.33));
    // Una flor gigante (margarita) a su lado
    b.cyl(v(x + 0.28, 0.0, z + 0.22), 0.004, 0.22, &m.blades[2]);
    for k in 0..10 {
        let ang = k as f64 * PI / 5.0;
        b.rell(v(x + 0.28 + ang.cos() * 0.025, 0.225, z + 0.22 + ang.sin() * 0.025), v(0.022, 0.003, 0.008), Mat3::rot_y(-ang), &solid(0xF8F6F0));
    }
    b.ell(v(x + 0.28, 0.228, z + 0.22), v(0.012, 0.006, 0.012), &solid(0xF2C020));
}

fn crow(b: &mut Builder, pos: Vec3, t: f64) {
    let black = solid(0x14161C).specular(0.8, 96.0).reflectivity(0.08);
    let s = 1.5;
    b.ell(pos + v(0.0, 0.18, 0.0) * s, v(0.26, 0.16, 0.14) * s, &black);
    b.rell(pos + v(-0.3, 0.2, 0.0) * s, v(0.16, 0.04, 0.08) * s, Mat3::rot_z(0.3), &black); // cola
    let turn = 0.6 * (t * 0.7).sin();
    let head = pos + v(0.22, 0.32, 0.0) * s;
    b.ball(head, 0.1 * s, &black);
    let dir = Mat3::rot_y(turn);
    b.rell(head + dir.mul_vec(v(0.12, -0.01, 0.0)) * s, v(0.09, 0.03, 0.03) * s, dir, &solid(0x3A3A3A).specular(0.6, 64.0));
    for sz in [-0.05, 0.05] {
        b.ball(head + dir.mul_vec(v(0.06, 0.03, sz)) * s, 0.015 * s, &solid(0xE8E0C0).glow(0.1));
        b.cyl(pos + v(0.02, 0.0, sz) * s, 0.008 * s, 0.06 * s, &solid(0x2A2A2A));
    }
}

// Golondrina volando en circulo con aleteo
fn swallow(b: &mut Builder, center: Vec3, radius: f64, speed: f64, phase: f64, t: f64) {
    let ang = t * speed + phase;
    let pos = center + v(ang.cos() * radius, 1.2 * (t * 0.7 + phase).sin(), ang.sin() * radius);
    let heading = Mat3::rot_y(-ang - PI / 2.0 * speed.signum()); // mira hacia donde vuela
    let body = solid(0x1E2A4A).specular(0.6, 64.0);
    let belly = solid(0xF0E8DC);
    b.rell(pos, v(0.18, 0.07, 0.07), heading, &body);
    b.rell(pos + v(0.0, -0.02, 0.0), v(0.14, 0.05, 0.06), heading, &belly);
    let flap = 0.9 * (t * 14.0 + phase * 5.0).sin();
    for side in [-1.0, 1.0] {
        let wing = heading.mul(&Mat3::rot_x(side * flap));
        b.rell(pos + wing.mul_vec(v(0.0, 0.0, side * 0.22)), v(0.1, 0.012, 0.22), wing, &body);
    }
    b.rell(pos + heading.mul_vec(v(-0.2, 0.0, 0.0)), v(0.1, 0.01, 0.05), heading, &body); // cola
}

// Gorrion en el suelo que picotea y da saltitos
fn sparrow(b: &mut Builder, pos: Vec3, facing: f64, t: f64, seed: f64) {
    let hop = (t * 2.3 + seed).sin().max(0.0).powi(8) * 0.12;
    let peck = if (t * 1.1 + seed).sin() > 0.7 { 0.05 } else { 0.0 };
    let r = Mat3::rot_y(facing);
    let p = pos + v(0.0, hop, 0.0);
    let brown = solid(0x8A5A34).specular(0.2, 16.0);
    b.rell(p + v(0.0, 0.07, 0.0), v(0.08, 0.055, 0.05), r, &brown);
    b.rell(p + v(0.0, 0.055, 0.0), v(0.06, 0.04, 0.045), r, &solid(0xE8DCC8));
    let head = p + r.mul_vec(v(0.07, 0.11 - peck, 0.0));
    b.ball(head, 0.035, &brown);
    b.rell(head + r.mul_vec(v(0.035, -0.005, 0.0)), v(0.018, 0.008, 0.008), r, &solid(0x2A2A2A));
    b.rell(p + r.mul_vec(v(-0.1, 0.09, 0.0)), v(0.06, 0.01, 0.025), r.mul(&Mat3::rot_z(0.4)), &brown);
}

// Mariposa revoloteando entre las flores
fn butterfly(b: &mut Builder, home: Vec3, t: f64, seed: u64) {
    let ph = h1(seed) * 20.0;
    let pos = home + v((t * 0.5 + ph).sin() * 1.6, 0.6 + 0.4 * (t * 1.3 + ph).sin(), (t * 0.37 + ph).cos() * 1.2);
    let heading = Mat3::rot_y(t * 0.5 + ph);
    const COLORS: [u32; 5] = [0xF8D040, 0xF4F0E8, 0x5A8AE0, 0xF08A2C, 0xE070B0];
    let wingm = solid(COLORS[(seed % 5) as usize]).specular(0.3, 32.0);
    let flap = 1.1 * (t * 18.0 + ph).sin().abs();
    b.rell(pos, v(0.03, 0.008, 0.008), heading, &solid(0x2A2018));
    for side in [-1.0, 1.0] {
        let wing = heading.mul(&Mat3::rot_x(side * (0.2 + flap)));
        b.rell(pos + wing.mul_vec(v(0.01, 0.0, side * 0.05)), v(0.045, 0.004, 0.055), wing, &wingm);
    }
}

// Pez koi nadando en circulo bajo el agua, ondulando la cola
fn koi(b: &mut Builder, center: Vec3, radius: f64, speed: f64, phase: f64, t: f64, seed: u32) {
    let ang = t * speed + phase;
    let pos = center + v(ang.cos() * radius, 0.1 * (t + phase).sin(), ang.sin() * radius * 0.7);
    let heading = Mat3::rot_y(-ang - PI / 2.0);
    let skin = Material::new(Texture::Koi(seed)).specular(0.7, 96.0);
    b.rell(pos, v(0.38, 0.1, 0.12), heading, &skin);
    let wag = 0.5 * (t * 6.0 + phase).sin();
    let tail = heading.mul(&Mat3::rot_y(wag));
    b.rell(pos + heading.mul_vec(v(-0.38, 0.0, 0.0)) + tail.mul_vec(v(-0.1, 0.0, 0.0)), v(0.14, 0.09, 0.02), tail, &skin);
    b.rell(pos + v(0.0, 0.1, 0.0), v(0.14, 0.05, 0.01), heading, &skin); // aleta dorsal
}

// Libelula sobre el agua: cuerpo largo azul y cuatro alas transparentes que vibran
fn dragonfly(b: &mut Builder, home: Vec3, t: f64, seed: u64) {
    let ph = h1(seed) * 30.0;
    let dart = ((t * 0.4 + ph).sin() * 3.0).tanh(); // se queda quieta y luego sale disparada
    let pos = home + v(dart * 2.0, 0.5 + 0.1 * (t * 3.0 + ph).sin(), (t * 0.3 + ph).cos() * 1.5);
    let heading = Mat3::rot_y(ph);
    b.rcyl(pos + heading.mul_vec(v(-0.12, 0.0, 0.0)), 0.008, 0.2, heading.mul(&Mat3::rot_z(-PI / 2.0)), &solid(0x2A6AC8).specular(0.9, 128.0).reflectivity(0.2));
    let wing = solid(0xE8F0F8).diffuse(0.3).specular(1.0, 200.0).transparency(0.7, 1.0);
    let buzz = 0.3 * (t * 60.0 + ph).sin();
    for (dx, side) in [(0.02, -1.0), (0.02, 1.0), (-0.03, -1.0), (-0.03, 1.0)] {
        let w = heading.mul(&Mat3::rot_x(side * buzz));
        b.rell(pos + heading.mul_vec(v(dx, 0.01, side * 0.08)), v(0.025, 0.002, 0.08), w, &wing);
    }
}

// Nenufares con flores de loto y una rana sobre uno de ellos
// Miembro redondeado con extremos solapados: evita las tapas cortadas de cilindros.
fn limb(b: &mut Builder, a: Vec3, end: Vec3, radius: f64, mat: &Material) {
    let d = end - a;
    let len = d.length();
    if len < 1e-8 { return; }
    let rot = Mat3::rot_y(-d.z.atan2(d.x)).mul(&Mat3::rot_z(-(d.y / len).clamp(-1.0, 1.0).acos()));
    b.rell((a + end) * 0.5, v(radius, len * 0.5 + radius * 0.65, radius), rot, mat);
}

// Cierre suave de 0.44 s; fases distintas para cada personaje.
fn blink_open(t: f64, phase: f64) -> f64 {
    let p = (t + phase).rem_euclid(4.1);
    if p < 0.44 { (((p - 0.22).abs() - 0.08) / 0.14).clamp(0.04, 1.0) } else { 1.0 }
}

fn sho_figure(b: &mut Builder, t: f64) {
    // Shō a escala humana (aprox. 1.65 m), recostado en el claro frente al kiosko.
    let origin = v(0.0, 0.0, -15.6);
    let p = |x, y, z| origin + v(x, y, z);
    let skin = solid(0xE9C5A1).specular(0.08, 16.0);
    let shirt = solid(0xE9E9DD).specular(0.08, 16.0);
    let pants = solid(0x475566);
    let hair = solid(0x293039).specular(0.05, 24.0);
    let blink = blink_open(t, 0.0);

    b.rell(p(0.38, 0.25, 0.0), v(0.36, 0.16, 0.23), Mat3::rot_z(-0.20), &shirt);
    b.ell(p(0.77, 0.18, 0.0), v(0.19, 0.13, 0.22), &pants);
    limb(b, p(0.14, 0.32, 0.0), p(0.065, 0.455, 0.0), 0.070, &skin);
    b.ell(p(0.18, 0.32, 0.0), v(0.095, 0.10, 0.15), &shirt);
    for side in [-1.0, 1.0] {
        let knee = p(1.08, 0.18 + if side < 0.0 { 0.10 } else { 0.0 }, side * 0.13);
        limb(b, p(0.79, 0.18, side * 0.11), knee, 0.108, &pants);
        b.ball(knee, 0.091, &pants);
        limb(b, knee, p(1.43, 0.095, side * 0.16), 0.085, &pants);
        b.ball(p(1.43, 0.095, side * 0.16), 0.060, &pants);
        b.ell(p(1.49, 0.07, side * 0.16 + 0.025), v(0.15, 0.065, 0.085), &solid(0x534A3F));
    }
    // Rostro inclinado hacia el libro y cabello oscuro.
    let head = p(0.035, 0.555, 0.0);
    let face = Mat3::rot_y(0.90).mul(&Mat3::rot_x(-0.18));
    let hp = |q| head + face.mul_vec(q);
    b.rell(head, v(0.137, 0.165, 0.128), face, &skin);
    b.rell(hp(v(0.0, 0.08, -0.045)), v(0.153, 0.145, 0.125), face, &hair);
    for i in 0..6 {
        b.rell(hp(v(-0.115 + i as f64 * 0.043, 0.104 - 0.008 * (i % 2) as f64, 0.091)),
            v(0.032, 0.059, 0.034), face.mul(&Mat3::rot_z(-0.32)), &hair);
    }
    for side in [-1.0, 1.0] {
        if blink > 0.12 {
            b.rell(hp(v(side * 0.052, 0.024, 0.119)), v(0.025, 0.021 * blink, 0.006), face, &solid(0xF4EDE1));
            b.rell(hp(v(side * 0.052 + 0.004, 0.022, 0.126)), v(0.010, 0.015 * blink, 0.003), face, &solid(0x38302A));
            b.rell(hp(v(side * 0.052, 0.029, 0.130)), v(0.003, 0.004 * blink, 0.002), face, &solid(0xFFFFFF));
        } else {
            rod(b, hp(v(side * 0.052 - 0.022, 0.022, 0.124)), hp(v(side * 0.052 + 0.022, 0.019, 0.124)), 0.0025, &hair);
        }
        rod(b, hp(v(side * 0.052 - 0.022, 0.057, 0.117)), hp(v(side * 0.052 + 0.022, 0.060, 0.117)), 0.003, &hair);
        b.ball(hp(v(side * 0.139, -0.015, 0.0)), 0.033, &skin);
    }
    b.rell(hp(v(0.0, -0.012, 0.124)), v(0.010, 0.018, 0.008), face, &skin);
    b.rell(hp(v(0.005, -0.063, 0.115)), v(0.019, 0.0025, 0.003), face, &solid(0x9F7361));
    for side in [-1.0, 1.0] {
        let shoulder = p(0.16, 0.27, side * 0.20);
        let elbow = p(0.40, 0.20, side * 0.32);
        let hand = p(if side < 0.0 { 0.38 } else { 0.76 }, 0.49, 0.16);
        b.ball(shoulder, 0.082, &shirt);
        limb(b, shoulder, shoulder.lerp(elbow, 0.72), 0.082, &shirt);
        limb(b, shoulder.lerp(elbow, 0.58), elbow, 0.056, &skin);
        b.ball(elbow, 0.052, &skin);
        limb(b, elbow, hand, 0.047, &skin);
        b.ell(hand, v(0.046, 0.044, 0.031), &skin);
    }
    // Libro abierto en V, con cubierta y paginas sin rotulos.
    for side in [-1.0, 1.0] {
        let rot = Mat3::rot_x(-0.55).mul(&Mat3::rot_y(side * 0.32));
        let c = p(0.57 + side * 0.11, 0.56, 0.10);
        b.rcube(c, v(0.115, 0.155, 0.015), rot, &solid(0x77533D));
        b.rcube(c + rot.mul_vec(v(0.0, 0.0, 0.019)), v(0.106, 0.145, 0.009), rot, &solid(0xEFE4C9));
    }
}

fn quiet_garden(b: &mut Builder, lights: &mut Vec<Light>, m: &Mats, station: f64) {
    let origin = v(0.0, 0.0, -15.6);
    let p = |x, y, z| origin + v(x, y, z);
    rock(b, m, p(-0.16, 0.1, -0.05), 0.26, 60101);
    // Gato dormido junto a su costado, con la cola recogida y ojos cerrados.
    let cp = p(0.53, 0.14, 0.49);
    b.ell(cp, v(0.27, 0.13, 0.16), &m.fur);
    b.ell(cp + v(0.20, 0.05, 0.035), v(0.12, 0.11, 0.10), &m.fur);
    for side in [-1.0, 1.0] {
        b.rell(cp + v(0.20 + side * 0.065, 0.15, 0.018), v(0.045, 0.07, 0.025), Mat3::rot_z(side * 0.2), &m.fur);
        rod(b, cp + v(0.20 + side * 0.046 - 0.022, 0.063, 0.131), cp + v(0.20 + side * 0.046 + 0.022, 0.058, 0.133), 0.005, &solid(0x30281F));
        b.ell(cp + v(0.18, -0.075, side * 0.1), v(0.085, 0.035, 0.045), &solid(0xDED6BF));
    }
    b.ball(cp + v(0.20, 0.028, 0.144), 0.012, &solid(0xAD7C73));
    let mut prev = cp + v(-0.23, -0.02, 0.0);
    for i in 1..13 {
        let a = 2.4 + i as f64 * 0.23;
        let next = cp + v(a.cos() * 0.28, -0.06, a.sin() * 0.20);
        rod(b, prev, next, 0.027, &m.fur);
        prev = next;
    }
    // Una sola Arrietty, de unos 13 cm, entre las hojas a un lado del claro.

    b.leaf(v(-0.46, 0.095, -14.70), 0.15, 0.062, Mat3::rot_z(0.9), &m.ivy);
    b.leaf(v(-0.31, 0.055, -14.70), 0.12, 0.045, Mat3::rot_z(-0.7), &m.ivy);
    // Grupos desiguales de flores, dejando libre el cuerpo y la entrada del kiosko.
    for group in 0..19u64 {
        let a = group as f64 * 2.399;
        let center = p(0.65 + a.cos() * (1.5 + h1(group + 70000) * 0.7), 0.0, a.sin() * 1.0);
        if center.x > 3.3 || center.z < -16.5 { continue; }
        for j in 0..(6 + group % 7) {
            let q = center + v((h1(group * 97 + j) - 0.5) * 0.7, 0.0, (h1(group * 113 + j) - 0.5) * 0.6);
            if q.z > -15.6 && q.x > -0.6 && q.x < 2.0 { continue; }
            b.xf = Some((q, 0.30 + h1(j + group * 37) * 0.35));
            wildflower(b, m, q, (group % 4) + j * 5 + 81000, 0.0);
            grass_tuft(b, m, q + v(0.1, 0.0, -0.1), j + group * 17, 0.0);
            b.xf = None;
        }
    }
    // Enredaderas sobre tres postes, sin cerrar el acceso ni ocultar el techo.
    for k in [0, 2, 4] {
        let a = k as f64 * PI / 3.0;
        let pos = GAZEBO + v(a.cos() * 2.25, 0.55, a.sin() * 2.25);
        climbing_vine(b, m, pos, 3.6, k as u64 + 85000);
        for j in 0..13u64 {
            let q = pos + v((j as f64 * 2.4).cos() * 0.20, 0.25 + j as f64 * 0.24, 0.15 + (j as f64 * 2.4).sin() * 0.12);
            blossom(b, q, 0.09 + h1(j + 91900) * 0.04, j, &solid(if j % 3 == 0 { 0xF3E6D7 } else { 0xDA91AF }));
        }
        hydrangea(b, m, pos + v(0.3, -0.5, 0.1), 0.5, (0xA588C7, 0xE3C6DF), k as u64 + 92100);
        fern(b, m, pos, 0.7, k as u64 + 85100);
    }
    warm_light(lights, p(0.0, 2.0, -0.3), 0.0, 1.1 + 1.7 * station, 4.5);
    // Jardineras en los barandales laterales, dejando libre el paso al claro.
    for (i, pos) in [v(-0.65, 1.8, -18.0), v(1.15, 1.8, -21.75)].iter().enumerate() {
        flower_box(b, m, *pos, 1.5, 95000 + i as u64 * 31);
        for j in 0..10u64 {
            let q = *pos + v(-0.65 + j as f64 * 0.14, -0.2 - h1(j + 96000) * 0.5, 0.20);
            b.leaf(q, 0.17, 0.085, Mat3::rot_z(-0.7), &m.ivy);
            blossom(b, q + v(0.0, 0.02, 0.04), 0.10, j, &solid(if j % 2 == 0 { 0xEAAFC3 } else { 0xF6E7C5 }));
        }
    }
    // Juncos, hojas de ribera y piedras pequenas interrumpen el borde recto.
    for group in 0..24u64 {
        let front = group % 2 == 0;
        let q = if front { v(5.0 + h1(group + 87000) * 18.0, 0.0, -9.6 + h1(group + 87001) * 0.45) }
            else { v(24.3 + h1(group + 87000) * 0.45, 0.0, -25.0 + h1(group + 87001) * 14.0) };
        rock(b, m, q + v(0.0, 0.02, 0.0), 0.15 + h1(group + 87002) * 0.23, group + 88000);
        for j in 0..7u64 {
            let at = q + v((h1(group * 31 + j) - 0.5) * 0.6, 0.0, (h1(group * 41 + j) - 0.5) * 0.4);
            let h = 0.7 + h1(group * 53 + j) * 0.85;
            rod(b, at, at + v(0.08, h, 0.02), 0.008, &m.blades[2]);
            b.leaf(at + v(0.07, h * 0.48, 0.0), h * 0.4, 0.035, Mat3::rot_z(1.15).mul(&Mat3::rot_y(j as f64)), &m.ivy);
            if j % 3 == 0 { b.ell(at + v(0.08, h, 0.02), v(0.025, 0.12, 0.025), &solid(0x64472D)); }
        }
    }
    // Vegetacion adicional en esquinas existentes; no cambia la arquitectura.
    for (x, z) in [(-22.4, -7.2), (-4.9, -7.2)] {
        climbing_vine(b, m, v(x, 0.5, z), 4.0, (x.abs() * 100.0) as u64);
    }
}

fn lily_pads(b: &mut Builder, m: &Mats) {
    let pad = Material::new(Texture::Leaf { a: hex(0x2E6A2A), b: hex(0x6AA83A) }).specular(0.6, 80.0).wet_gloss(0.5);
    let lotus = solid(0xF4B8D0).specular(0.3, 32.0);
    for i in 0..32u64 {
        let (x, z) = (5.5 + h1(i + 3000) * 17.0, -24.5 + h1(i + 3001) * 13.0);
        let r = 0.35 + 0.35 * h1(i + 3002);
        b.cyl(v(x, -0.26, z), r, 0.02, &pad);
        if i % 4 == 0 {
            for k in 0..6 {
                let a = k as f64 * PI / 3.0;
                b.rell(v(x + a.cos() * 0.08, -0.18, z + a.sin() * 0.08), v(0.11, 0.04, 0.05), Mat3::rot_y(-a).mul(&Mat3::rot_z(0.6)), &lotus);
            }
            b.ball(v(x, -0.17, z), 0.05, &solid(0xF8D040));
        }
    }
    // Rana verde sentada en un nenufar
    let (fx, fz) = (8.0, -12.2);
    b.cyl(v(fx, -0.26, fz), 0.45, 0.02, &pad);
    let frog = solid(0x5A9A3A).specular(0.7, 96.0).reflectivity(0.05);
    b.ell(v(fx, -0.14, fz), v(0.14, 0.09, 0.12), &frog);
    b.ell(v(fx + 0.1, -0.1, fz), v(0.08, 0.06, 0.09), &frog);
    for sz in [-0.05, 0.05] {
        b.ball(v(fx + 0.14, -0.05, fz + sz), 0.03, &frog);
        b.ball(v(fx + 0.165, -0.045, fz + sz), 0.015, &solid(0x1A1A10).specular(1.0, 200.0));
    }
    let _ = m;
}

// Geometria fija para un ambiente dado: terreno, lago, casa, kiosco, arboles, pasto y flores
fn build_fixed(station: f64) -> (World, Vec<Light>) {
    let rain = smoothstep(0.4, 1.0, station);
    let m = materials(0.0, station, rain);
    let mut b = Builder::new();
    let mut lights = Vec::new();

    // Terreno grande con un hueco para el lago (el agua es animada y va aparte)
    b.cube((-90.0, -3.0, -90.0), (4.0, 0.0, 90.0), &m.ground);
    b.cube((24.0, -3.0, -90.0), (90.0, 0.0, 90.0), &m.ground);
    b.cube((4.0, -3.0, -90.0), (24.0, 0.0, -26.0), &m.ground);
    b.cube((4.0, -3.0, -10.0), (24.0, 0.0, 90.0), &m.ground);
    b.cube((4.0, -3.0, -26.0), (24.0, -2.2, -10.0), &m.lakebed);

    // Rocas grandes con musgo bordeando el lago (y algunas dentro del agua)
    for i in 0..34u64 {
        let k = i as f64 / 34.0;
        let (x, z) = if k < 0.4 { (4.0 + 0.6 * (h1(i) - 0.5), -26.0 + k / 0.4 * 16.0) } else if k < 0.8 { (4.0 + (k - 0.4) / 0.4 * 20.0, -10.0 + 0.6 * (h1(i) - 0.5)) } else { (24.0, -10.0 - (k - 0.8) / 0.2 * 16.0) };
        let r = 0.6 + 0.9 * h1(i + 100);
        rock(&mut b, &m, v(x, r * 0.2 - 0.1, z), r, i);
    }
    for (x, z, r) in [(9.0, -18.0, 1.1), (15.5, -22.0, 1.6), (19.0, -14.0, 0.8), (7.0, -24.0, 1.3)] {
        rock(&mut b, &m, v(x, -0.6, z), r, (x * 10.0) as u64);
    }
    for i in 0..9u64 {
        let x = 3.0 + i as f64 * 2.8;
        let r = 2.2 + 1.6 * h1(i + 400);
        rock(&mut b, &m, v(x, r * 0.5, -28.5 - h1(i + 410) * 2.0), r, i + 430);
    }

    // Casa gigante (1.8x) y kiosco grande (1.6x): se construyen con escala alrededor de su ancla
    b.xf = Some((HOUSE_ANCHOR, HOUSE_SCALE));
    house(&mut b, &mut lights, &m, 0.0, station);
    b.xf = Some((GAZEBO, GAZEBO_SCALE));
    gazebo(&mut b, &mut lights, &m, GAZEBO, 0.0, station);
    b.xf = None;

    // Hortensias (azules, moradas y rosas) al pie de la casa, iconicas del jardin de la pelicula
    const HYD: [(u32, u32); 4] = [(0x5A7AD8, 0xA8C0F0), (0x8A6AD0, 0xC8B0F0), (0xD880B8, 0xF4C8E0), (0x4A9AD0, 0xB8E0F4)];
    for (i, x) in [-22.0, -19.6, -17.4, -10.2, -8.0, -3.2].iter().enumerate() {
        hydrangea(&mut b, &m, v(*x, 0.0, -7.6 - 0.3 * h1(i as u64)), 1.0 + 0.25 * h1(i as u64 + 7), HYD[i % 4], i as u64 + 40);
    }
    for (i, (x, z)) in [(-23.6, -12.0), (-23.6, -17.0), (-1.2, -12.5), (-2.8, -24.8), (-5.5, -24.0)].iter().enumerate() {
        hydrangea(&mut b, &m, v(*x, 0.0, *z), 1.1, HYD[(i + 2) % 4], i as u64 + 60);
    }
    // Hiedra trepando por la fachada y los costados
    ivy_patch(&mut b, &m, -22.6, -17.0, 1.1, 9.5, -8.9, false, 90, 7000);
    ivy_patch(&mut b, &m, -7.5, -4.0, 1.1, 7.0, -8.9, false, 50, 7400);
    ivy_patch(&mut b, &m, -21.6, -9.5, 1.1, 10.0, -22.1, true, 120, 7800);
    borrowers_corner(&mut b, &mut lights, &m, station);
    quiet_garden(&mut b, &mut lights, &m, station);
    lily_pads(&mut b, &m);
    for i in 0..32u64 {
        let p = v(1.3 + h1(i + 18000) * 1.6, 0.0, -10.0 - h1(i + 18100) * 16.0);
        if p.z > -17.0 && p.z < -13.0 { continue; }
        fern(&mut b, &m, p, 0.45 + h1(i + 18200) * 0.7, i);
        rock(&mut b, &m, p + v(-0.5, 0.04, 0.35), 0.12 + h1(i + 18300) * 0.24, i + 18400);
    }
    for i in 0..18u64 {
        fern(&mut b, &m, v(-23.8 + h1(i + 18500) * 18.0, 0.0, -6.7 + h1(i + 18600)), 0.5 + h1(i + 18700) * 0.6, i + 90);
    }
    // Faroles de piedra y banca junto al lago
    for (x, z) in [(3.0, -9.0), (24.8, -9.0)] {
        let tp = v(x, 0.0, z);
        b.cyl(tp, 0.35, 0.2, &m.stone);
        b.cyl(tp, 0.14, 1.1, &m.stone);
        b.cube((tp.x - 0.35, 1.1, tp.z - 0.35), (tp.x + 0.35, 1.2, tp.z + 0.35), &m.stone);
        b.cube((tp.x - 0.25, 1.2, tp.z - 0.25), (tp.x + 0.25, 1.6, tp.z + 0.25), &solid(0xFFD89A).glow(0.1 + 2.5 * station));
        b.rcube(tp + v(0.0, 1.72, 0.0), v(0.45, 0.12, 0.45), Mat3::rot_y(PI / 4.0), &m.stone);
        warm_light(&mut lights, tp + v(0.0, 1.4, 0.6), 0.0, 3.0 * station, 5.5);
    }
    b.cube((14.0, 0.45, -8.2), (16.4, 0.55, -7.6), &m.wood);
    for x in [14.2, 16.2] {
        b.cube((x - 0.06, 0.0, -8.1), (x + 0.06, 0.45, -7.7), &m.wood);
    }

    // Lajas desde la puerta hacia el frente
    for (i, (x, z)) in [(-13.2, -7.3), (-12.6, -5.8), (-11.6, -4.5), (-10.3, -3.4), (-8.6, -2.7), (-6.8, -2.1), (-5.0, -1.4), (-3.2, -0.6), (-1.6, 0.4)].iter().enumerate() {
        b.cyl(v(*x, 0.0, *z), 0.42 + 0.08 * h1(i as u64), 0.07, &m.stone);
    }

    // Arboles: alcanforeros gigantes, pino japones y bosque alrededor
    big_tree(&mut b, &m, v(13.0, 0.0, -4.0), 7.0, 5.5, 3);
    big_tree(&mut b, &m, v(-31.0, 0.0, -26.0), 9.0, 6.5, 7);
    big_tree(&mut b, &m, v(-26.0, 0.0, 2.0), 6.5, 5.0, 11);
    pine(&mut b, &m, v(-1.0, 0.0, -27.0), 21);
    for i in 0..44u64 {
        let a = i as f64 / 44.0 * 2.0 * PI + 0.1 * h1(i);
        let r = 36.0 + 10.0 * h1(i + 600);
        far_tree(&mut b, &m, v(a.cos() * r, 0.0, -8.0 + a.sin() * r), 12.0 + 8.0 * h1(i + 700), i * 13);
    }
    for i in 0..14u64 {
        // Arboles intermedios detras del lago y de la casa
        let x = -30.0 + i as f64 * 4.6 + 1.5 * h1(i + 800);
        far_tree(&mut b, &m, v(x, 0.0, -33.0 - 4.0 * h1(i + 810)), 10.0 + 6.0 * h1(i + 820), i * 29 + 5);
    }

    // Matas de arbustos sueltas
    for i in 0..40u64 {
        let (x, z) = (-24.0 + h1(i * 2 + 5000) * 48.0, -30.0 + h1(i * 2 + 5001) * 26.0);
        if blocked(x, z) {
            continue;
        }
        let r = 0.7 + 0.8 * h1(i + 5100);
        for j in 0..65u64 {
            let angle = j as f64 * 2.399;
            let radius = r * h1(i * 71 + j).sqrt();
            let q = v(x + angle.cos() * radius, r * (0.3 + 0.7 * h1(i * 91 + j)), z + angle.sin() * radius);
            b.leaf(q, 0.22, 0.10, Mat3::rot_y(angle).mul(&Mat3::rot_z(0.3)), &m.ivy);
        }
    }

    // Pasto alto y flores silvestres: capa decorativa (no proyecta sombra, mucho mas rapido)
    b.decor = true;
    // Macizos por grupos: flores de la misma especie crecen juntas.
    for bed in 0..26u64 {
        let center = if bed == 0 { v(-3.5, 0.0, 1.8) } else {
            v(-20.0 + h1(bed + 22000) * 24.0, 0.0, -6.0 + h1(bed + 22100) * 13.0)
        };
        if blocked(center.x, center.z) { continue; }
        for flower in 0..18u64 {
            let angle = flower as f64 * 2.399;
            let r = 0.75 * (flower as f64 / 18.0).sqrt();
            let p = center + v(angle.cos() * r, 0.0, angle.sin() * r);
            if !blocked(p.x, p.z) {
                wildflower(&mut b, &m, p, 25000 + bed % 5 + flower * 5 + bed * 100, 0.0);
            }
        }
    }
    let mut i = 0u64;
    let mut placed = 0;
    while placed < 1600 && i < 9000 {
        i += 1;
        let x = -22.0 + h1(i * 2 + 9000) * 44.0;
        let z = -28.0 + h1(i * 2 + 9001).powf(0.7) * 44.0;
        if blocked(x, z) {
            continue;
        }
        grass_tuft(&mut b, &m, v(x, 0.0, z), i, 0.0);
        placed += 1;
    }
    let mut placed = 0;
    while placed < 800 && i < 30000 {
        i += 1;
        let x = -22.0 + h1(i * 2 + 9000) * 44.0;
        let z = -26.0 + h1(i * 2 + 9001).powf(0.8) * 40.0;
        if blocked(x, z) || (x * 0.6 + (z * 0.4).sin()).sin() * (z * 0.55).cos() > 0.15 {
            continue;
        }
        wildflower(&mut b, &m, v(x, 0.0, z), i, 0.0);
        placed += 1;
    }

    let mut world = b.world;
    world.build();
    lights.retain(|l| match l {
        Light::Point(p) => p.intensity > 0.25, // al atardecer las luces calidas no se notan: no se trazan
        Light::Directional { .. } => true,
    });
    (world, lights)
}

// Cache de la parte fija: solo se reconstruye si cambia el ambiente (durante la transicion)
fn fixed_for(station: f64) -> (Arc<World>, Vec<Light>) {
    // Dos BVH persistentes precalculados; nunca reconstruir el jardin durante N.
    static CACHE: OnceLock<[(Arc<World>, Vec<Light>); 2]> = OnceLock::new();
    let states = CACHE.get_or_init(|| [0.0, 1.0].map(|s| {
        let (w, lights) = build_fixed(s);
        (Arc::new(w), lights)
    }));
    let (world, lights) = &states[if station >= 0.5 { 1 } else { 0 }];
    (world.clone(), lights.clone())
}

pub fn build_scene(t: f64, station: f64) -> SceneData {
    let station = station.clamp(0.0, 1.0);
    let rain = smoothstep(0.4, 1.0, station);
    // Relampagos: un destello doble cada ~11 s cuando llueve
    let ph = (t / 11.0).fract();
    let flash = rain * if ph < 0.05 { (ph * 260.0).sin().abs() } else if (0.09..0.12).contains(&ph) { 0.6 } else { 0.0 };
    let sky = SkyParams::new(station, t, flash);
    let (fixed, mut lights) = fixed_for(station);
    let m = materials(t, station, rain);
    let mut b = Builder::new();

    sho_figure(&mut b, t);
    arrietty_figure(&mut b, v(-5.98, 0.0, -7.01), 0.085, t);
    arrietty_figure(&mut b, v(-0.40, 0.0, -14.75), 0.085, t);
    // El gato sentado sobre una laja (proyecta sombra) y el cuervo en la cumbrera
    cat(&mut b, &m, v(-6.8, 0.07, -2.1), 0.25, 0.15, t, station);
    crow(&mut b, v(-8.5, 17.45, -15.3), t);
    for (i, (x, z)) in [(5.0, -9.0), (6.0, -9.2), (7.2, -9.0)].iter().enumerate() {
        sparrow(&mut b, v(*x, 0.0, *z), 0.7 + i as f64, t, i as f64 + 8.0);
    }
    // Gorriones picoteando en el camino
    for (i, (x, z)) in [(-4.2, -0.4), (-3.6, 0.2), (-11.0, -3.0), (-2.4, -1.4)].iter().enumerate() {
        sparrow(&mut b, v(*x, 0.0, *z), i as f64 * 1.7, t, i as f64 * 2.3);
    }

    // Lo demas animado no proyecta sombra: agua del lago, hojas y lluvia
    b.decor = true;
    b.cube((4.0, -2.2, -26.0), (24.0, -0.25, -10.0), &m.water);
    // Peces koi bajo el agua (se ven gracias a la refraccion) y libelulas encima
    for i in 0..10u64 {
        let c = v(14.0 + 3.0 * (h1(i + 60) - 0.5), -0.9 - 0.4 * h1(i + 61), -18.0 + 2.0 * (h1(i + 62) - 0.5));
        let sp = if i % 2 == 0 { 0.35 } else { -0.28 };
        koi(&mut b, c, 2.5 + 4.0 * h1(i + 63), sp, h1(i + 64) * 6.3, t, i as u32);
    }
    for i in 0..5u64 {
        dragonfly(&mut b, v(7.0 + i as f64 * 3.5, -0.25, -12.0 - 2.0 * h1(i + 70)), t, i + 70);
    }
    // Golondrinas volando en circulos y mariposas entre las flores (no de noche con lluvia)
    if station < 0.7 {
        for i in 0..7u64 {
            let sp = if i % 3 == 0 { -0.45 } else { 0.4 };
            swallow(&mut b, v(-2.0 + 4.0 * h1(i + 80), 11.0 + 4.0 * h1(i + 81), -10.0), 7.0 + 7.0 * h1(i + 82), sp, h1(i + 83) * 6.3, t);
        }
        for i in 0..16u64 {
            let home = v(-20.0 + h1(i + 90) * 30.0, 0.0, -6.0 + h1(i + 91) * 12.0);
            butterfly(&mut b, home, t, i);
        }
    }
    if station < 0.6 {
        for i in 0..8u64 {
            let fall = (t * 0.45 + h1(i) * 12.0).rem_euclid(12.0);
            let p = v(8.0 + h1(i + 10) * 10.0 + 0.8 * (t * 0.9 + i as f64).sin(), 12.0 - fall, -9.0 + h1(i + 20) * 10.0);
            let rot = Mat3::rot_y(t * 1.1 + i as f64).mul(&Mat3::rot_x(0.6 * (t * 1.7 + i as f64).sin()));
            b.rell(p, v(0.12, 0.01, 0.07), rot, &Material::new(Texture::Leaf { a: hex(0x6AA83A), b: hex(0xC8D860) }));
        }
    }
    let mut moving = b.world;
    moving.build();

    // Sol de la tarde y luz fria de la noche (con relampago)
    for (direction, color, intensity) in sky.lights() {
        if intensity > 0.01 {
            lights.push(Light::Directional { direction, color, intensity });
        }
    }

    SceneData {
        fixed,
        moving,
        lights,
        sky,
        fog_start: 25.0,
        wetness: rain,
        dry_zones: vec![(v(-24.0, 0.0, -23.0), v(-2.0, 18.0, -7.8)), (v(-1.8, 0.0, -22.6), v(4.2, 6.5, -16.4))],
    }
}

