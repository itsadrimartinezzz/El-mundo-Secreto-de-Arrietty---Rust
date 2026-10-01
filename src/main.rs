use std::time::Instant;

use arrietty_rt::camera::Camera;
use arrietty_rt::post::{self, PostParams};
use arrietty_rt::{png, renderer, scene};

// Imagen fija en alta calidad:
//   cargo run --release -- [ancho] [alto] [muestras] [profundidad] [estacion: 0 atardecer, 1 noche lluviosa] [tiempo_s] [vista: 0 jardin, 1 Arrietty, 2 lago, 3 gatito]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let width: u32 = (arg_or(&args, 1, 3840.0) as u32).max(1);
    let height: u32 = (arg_or(&args, 2, 2160.0) as u32).max(1);
    let samples: u32 = arg_or(&args, 3, 64.0) as u32;
    let max_depth: u32 = arg_or(&args, 4, 10.0) as u32;
    let station = arg_or(&args, 5, 0.0);
    let time = arg_or(&args, 6, 1.3);
    let view = &scene::VIEWS[arg_or(&args, 7, 0.0) as usize % scene::VIEWS.len()];

    let scene_data = scene::build_scene(time, station);
    // Nitidez en todo el diorama por defecto; apertura opcional en el argumento 9.
    let aperture = arg_or(&args, 9, 0.0).max(0.0);
    let camera = Camera::orbit(view.target, view.dist, view.yaw, view.pitch, scene::CAM_VFOV, width as f64 / height as f64, aperture);

    println!("Renderizando la casa de Arrietty: {width}x{height}, {samples} muestras, profundidad {max_depth}, estacion {station:.2}, t={time:.1}s");
    let start = Instant::now();
    let mut hdr = renderer::render_hdr(&camera, &scene_data, width, height, samples, max_depth, 0x5EED_1234_ABCD);
    add_rain(&mut hdr, &camera, &scene_data, width, height, time, station);
    let params = PostParams { bloom: 0.25 + 0.4 * station as f32, sun_uv: scene::sun_screen(&camera), rays: 0.55 * (1.0 - station as f32), ..PostParams::default() };
    let image = post::finish(&hdr, width, height, &params);
    println!("Render terminado en {:.2?}", start.elapsed());

    let output = args.get(8).map(String::as_str).unwrap_or("output.png");
    png::write_png(output, width, height, &image).expect("no se pudo escribir el PNG");
    println!("Imagen guardada en {output}");
}

fn arg_or(args: &[String], idx: usize, default: f64) -> f64 {
    args.get(idx).and_then(|s| s.parse().ok()).unwrap_or(default)
}

// Trazos de lluvia encima de la imagen (solo en la noche lluviosa)
fn add_rain(hdr: &mut [f32], camera: &Camera, scene_data: &scene::SceneData, width: u32, height: u32, time: f64, station: f64) {
    let streaks = scene::rain_streaks(time, station);
    if streaks.is_empty() {
        return;
    }
    let depth = renderer::depth_map(camera, scene_data, width, height);
    let tint = scene_data.sky.ambient().0 * 1.4 + arrietty_rt::vec3::Vec3::new(0.25, 0.28, 0.33);
    renderer::draw_rain(hdr, width, height, camera, &|i| depth[i], &streaks, tint, 0.55);
}
