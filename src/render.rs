use crate::light::Light;
use crate::ray::Ray;
use crate::rng::Rng;
use crate::scene::SceneData;
use crate::sky::{sky_color, smoothstep};
use crate::vec3::{Color, Point3, Vec3};

const BIAS: f64 = 1.0e-4;
const MIN_CONTRIBUTION: f64 = 0.05; // ramas que aportan menos que esto no se trazan
const AMBIENT: f64 = 0.32;
const FAR: f64 = 1.0e4; // distancia usada como "posicion" del cielo en la reproyeccion

// Fresnel de Schlick: cuanta luz se refleja segun el angulo
fn schlick(cosine: f64, ratio: f64) -> f64 {
    let r0 = ((1.0 - ratio) / (1.0 + ratio)).powi(2);
    r0 + (1.0 - r0) * (1.0 - cosine).powi(5)
}

// Inclina la normal cerca de los bordes de cada bloque de 1x1 para que se vea biselado
fn bevel_normal(n: Vec3, p_local: Vec3, width: f64) -> Vec3 {
    let axis_n = if n.x.abs() > 0.5 { 0 } else if n.y.abs() > 0.5 { 1 } else { 2 };
    let mut tilt = Vec3::ZERO;
    for a in 0..3 {
        if a == axis_n {
            continue;
        }
        let f = p_local[a].rem_euclid(1.0);
        let e = match a {
            0 => Vec3::new(1.0, 0.0, 0.0),
            1 => Vec3::new(0.0, 1.0, 0.0),
            _ => Vec3::new(0.0, 0.0, 1.0),
        };
        if f < width {
            tilt = tilt - e * (1.0 - f / width);
        } else if f > 1.0 - width {
            tilt = tilt + e * (1.0 - (1.0 - f) / width);
        }
    }
    (n + tilt * 0.55).unit()
}

// Rayo primario: devuelve el color y el punto del mundo que vio (para reproyectar en el TAA)
pub fn trace_primary(ray: &Ray, scene: &SceneData, depth: u32, rng: &mut Rng) -> (Color, Point3) {
    let (c, t) = shade(ray, scene, depth, rng, 1.0);
    let c = sanitize(c);
    let t = if t.is_finite() { t } else { FAR / ray.dir.length() };
    (c, ray.at(t))
}

pub fn ray_color(ray: &Ray, scene: &SceneData, depth: u32, rng: &mut Rng) -> Color {
    sanitize(shade(ray, scene, depth, rng, 1.0).0)
}

// Un NaN o infinito (p.ej. un rayo rasante degenerado) se vuelve negro en vez de contagiar al TAA y al bloom
fn sanitize(c: Color) -> Color {
    if c.x.is_finite() && c.y.is_finite() && c.z.is_finite() {
        Color::new(c.x.max(0.0), c.y.max(0.0), c.z.max(0.0))
    } else {
        Color::ZERO
    }
}

// Raytracing de Whitted: luz local Blinn-Phong estilizada + rayo reflejado + rayo refractado, con niebla por distancia
fn shade(ray: &Ray, scene: &SceneData, depth: u32, rng: &mut Rng, weight: f64) -> (Color, f64) {
    if depth == 0 || weight < MIN_CONTRIBUTION {
        return (Color::ZERO, f64::INFINITY);
    }
    let rec = match scene.hit(ray, 0.001, f64::INFINITY) {
        Some(r) => r,
        None => return (sky_color(ray.dir, &scene.sky), f64::INFINITY),
    };

    let mat = rec.material;
    let mut albedo = mat.texture.value(rec.obj_point, rec.p, rec.local_normal);
    // Con lluvia las superficies expuestas se oscurecen (mojadas)
    let wet = if scene.wetness > 0.0 && mat.transparency < 0.1 && !scene.is_dry(rec.p) { scene.wetness } else { 0.0 };
    albedo = albedo * (1.0 - 0.35 * wet);
    let unit_dir = ray.dir.unit();
    let view_dir = -unit_dir;

    // Normal de sombreado: bisel de bloque y ondas del agua
    let mut outward = mat.texture.bump(rec.p, rec.outward);
    if mat.bevel > 0.0 && rec.outward == rec.local_normal {
        outward = bevel_normal(outward, rec.obj_point, mat.bevel); // solo en prismas sin rotar
    }
    let normal = if rec.front_face { outward } else { -outward };

    // Ambiente hemisferico segun la hora + brillo propio
    let (sky_amb, ground_amb) = scene.sky.ambient();
    let hemi = ground_amb.lerp(sky_amb, 0.5 + 0.5 * normal.y);
    let mut local = mat.emission + albedo * mat.glow + albedo.component_mul(hemi) * AMBIENT;

    for light in &scene.lights {
        let (light_dir, max_dist, intensity, light_color) = match light {
            Light::Point(pl) => {
                let to_light = pl.position - rec.p;
                let dist_squared = to_light.length_squared();
                if dist_squared > pl.range * pl.range || dist_squared < 1.0e-12 {
                    continue;
                }
                let dist = dist_squared.sqrt();
                // Atenuacion que llega suave a cero en el alcance
                let fade = 1.0 - (dist / pl.range).powi(2);
                (to_light / dist, dist - BIAS, pl.intensity * fade / (1.0 + dist * dist), pl.color)
            }
            Light::Directional { direction, color, intensity } => (*direction, 1.0e6, *intensity, *color),
        };
        if intensity < 1.0e-3 {
            continue;
        }
        let n_dot_l = normal.dot(light_dir);
        if n_dot_l <= 0.0 {
            continue;
        }
        // Luces direccionales muy tenues (la luna tras las nubes) no pagan rayo de sombra
        let cheap = matches!(light, Light::Directional { .. }) && intensity < 0.25;
        // Muy lejos (bosque del fondo, cubierto por la niebla) la sombra no se distingue: no se traza
        let cheap = cheap || rec.t * ray.dir.length() > 45.0;
        if !cheap && scene.is_shadowed(&Ray::new(rec.p + rec.normal * BIAS, light_dir), max_dist) {
            continue;
        }
        // Rampa estilo anime: el paso de luz a sombra es corto y marcado, pero sin perder volumen
        let ramp = 0.35 * n_dot_l + 0.65 * smoothstep(0.0, 0.3, n_dot_l);
        local += albedo.component_mul(light_color) * (mat.diffuse * ramp * intensity);
        if mat.specular > 0.0 {
            let half = (light_dir + view_dir).unit();
            local += light_color * (mat.specular * normal.dot(half).max(0.0).powf(mat.shininess) * intensity);
        }
    }

    let mut color = local * (1.0 - mat.reflectivity - mat.transparency).max(0.0);

    // Superficies mojadas que miran hacia arriba reflejan un poco el entorno (charcos finos, tejas brillantes)
    if wet > 0.0 && mat.wet_gloss > 0.0 && normal.y > 0.5 && mat.reflectivity < 0.3 {
        let k = wet * mat.wet_gloss * (0.15 + 0.85 * schlick(view_dir.dot(normal).clamp(0.0, 1.0), 1.0 / 1.33));
        if weight * k > MIN_CONTRIBUTION {
            let (refl, _) = shade(&Ray::new(rec.p + rec.normal * BIAS, unit_dir.reflect(normal)), scene, depth - 1, rng, weight * k);
            color = color * (1.0 - k) + refl * k;
        }
    }

    // Reflexion tipo espejo/metal
    if mat.reflectivity > MIN_CONTRIBUTION {
        let fuzz = if mat.fuzz > 0.0 { rng.in_unit_sphere() * mat.fuzz } else { Vec3::ZERO };
        let dir = (unit_dir.reflect(normal) + fuzz).unit();
        let (refl, _) = shade(&Ray::new(rec.p + rec.normal * BIAS, dir), scene, depth - 1, rng, weight * mat.reflectivity);
        // Los metales tinen su reflejo con su propio color (oro)
        color += refl.component_mul(Color::ONE.lerp(albedo * 1.6, 0.5)) * mat.reflectivity;
    }

    // Medio sin desviacion (gotas de lluvia): el rayo simplemente sigue de largo
    if mat.transparency > MIN_CONTRIBUTION && mat.ior == 1.0 {
        let through = Ray::new(rec.p + unit_dir * BIAS * 10.0, unit_dir);
        let (behind, _) = shade(&through, scene, depth - 1, rng, weight * mat.transparency);
        color += behind * mat.transparency;
    } else if mat.transparency > MIN_CONTRIBUTION {
        // Refraccion (Snell) mezclada con reflexion por Fresnel
        let cos_theta = view_dir.dot(normal).clamp(0.0, 1.0);
        let ratio = if rec.front_face { 1.0 / mat.ior } else { mat.ior };
        let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
        let fresnel = schlick(cos_theta, ratio).min(0.9);

        let reflect_ray = Ray::new(rec.p + rec.normal * BIAS, unit_dir.reflect(normal));
        let (reflect_col, _) = shade(&reflect_ray, scene, depth - 1, rng, weight * mat.transparency * fresnel);

        let dielectric = if ratio * sin_theta > 1.0 {
            reflect_col // reflexion interna total
        } else {
            let refract_ray = Ray::new(rec.p - rec.normal * BIAS, unit_dir.refract(normal, ratio));
            let (refract_col, _) = shade(&refract_ray, scene, depth - 1, rng, weight * mat.transparency * (1.0 - fresnel));
            // Lo que atraviesa el agua/hielo toma parte de su color (como si dispersara luz)
            reflect_col * fresnel + refract_col.lerp(albedo, 0.55) * (1.0 - fresnel)
        };
        color += dielectric * mat.transparency;
    }

    // Niebla: se mezcla con el color del horizonte segun la distancia recorrida
    let dist = rec.t * ray.dir.length();
    let fog = 1.0 - (-(dist - scene.fog_start).max(0.0) * scene.sky.fog_density()).exp();
    (color.lerp(scene.sky.horizon(), fog), rec.t)
}
