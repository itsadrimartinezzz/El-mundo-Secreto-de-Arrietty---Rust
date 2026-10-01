use crate::texture::Texture;
use crate::vec3::Color;

// Material Blinn-Phong estilizado + reflexion + refraccion; cada bloque tiene su textura y parametros propios
#[derive(Clone)]
pub struct Material {
    pub texture: Texture,
    pub diffuse: f64,      // peso del albedo en la luz local
    pub specular: f64,     // intensidad del brillo especular
    pub shininess: f64,    // que tan concentrado es el brillo
    pub reflectivity: f64, // fraccion que viene del rayo reflejado
    pub transparency: f64, // fraccion que viene del rayo refractado
    pub ior: f64,          // indice de refraccion
    pub fuzz: f64,         // reflejo borroso (0 = espejo perfecto)
    pub glow: f64,         // brillo propio proporcional a la textura (bloque ?)
    pub bevel: f64,        // ancho del bisel en los bordes de cada bloque (0 = sin bisel)
    pub wet_gloss: f64,    // cuanto refleja cuando esta mojado (piedra, tejas, fierro)
    pub emission: Color,   // luz propia de color fijo
}

impl Material {
    pub fn new(texture: Texture) -> Self {
        Material {
            texture,
            diffuse: 1.0,
            specular: 0.0,
            shininess: 32.0,
            reflectivity: 0.0,
            transparency: 0.0,
            ior: 1.0,
            fuzz: 0.0,
            glow: 0.0,
            bevel: 0.0,
            wet_gloss: 0.0,
            emission: Color::ZERO,
        }
    }

    pub fn diffuse(mut self, d: f64) -> Self {
        self.diffuse = d;
        self
    }
    pub fn specular(mut self, s: f64, shininess: f64) -> Self {
        self.specular = s;
        self.shininess = shininess;
        self
    }
    pub fn reflectivity(mut self, r: f64) -> Self {
        self.reflectivity = r;
        self
    }
    pub fn fuzz(mut self, f: f64) -> Self {
        self.fuzz = f;
        self
    }
    pub fn transparency(mut self, t: f64, ior: f64) -> Self {
        self.transparency = t;
        self.ior = ior;
        self
    }
    pub fn glow(mut self, g: f64) -> Self {
        self.glow = g;
        self
    }
    pub fn bevel(mut self, b: f64) -> Self {
        self.bevel = b;
        self
    }
    pub fn wet_gloss(mut self, g: f64) -> Self {
        self.wet_gloss = g;
        self
    }
    pub fn emission(mut self, e: Color) -> Self {
        self.emission = e;
        self
    }
}
