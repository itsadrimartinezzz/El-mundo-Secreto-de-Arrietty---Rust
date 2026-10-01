//! Genera los cuadros PNG de un video: la camara da la vuelta completa a la casa de Arrietty, se acerca y aleja,
//! la escena esta animada y pasa del atardecer a la noche lluviosa. El video se arma aparte (p.ej. con ffmpeg).
//!
//! Uso:
//!   cargo run --release --bin anim -- [cuadros] [ancho] [alto] [muestras] [profundidad] [fps]
//!
//! Los cuadros se guardan en `frames/frame_0000.png`, `frame_0001.png`, ...

use std::f64::consts::PI;
use std::fs;
use std::time::Instant;

use arrietty_rt::camera::Camera;
use arrietty_rt::post::{self, PostParams};
use arrietty_rt::{png, renderer, scene};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let num_frames = arg_or(&args, 1, 360.0) as u32;
    let width = arg_or(&args, 2, 1280.0) as u32;
    let height = arg_or(&args, 3, 720.0) as u32;
    let samples = arg_or(&args, 4, 12.0) as u32;
    let max_depth = arg_or(&args, 5, 6.0) as u32;
    let fps = arg_or(&args, 6, 30.0);

    fs::create_dir_all("frames").expect("no se pudo crear la carpeta frames/");
    println!("Generando {num_frames} cuadros a {width}x{height}, {samples} muestras, profundidad {max_depth}, {fps} fps");
    let start = Instant::now();

    for i in 0..num_frames {
        let u = i as f64 / num_frames as f64; // avance del video en [0,1)
        let time = i as f64 / fps; // segundos de animacion

        // Suaviza el inicio y el final del giro para que no arranque de golpe
        let ease = 0.5 - 0.5 * (PI * u).cos();
        let yaw = scene::CAM_YAW + 360.0 * ease;
        let pitch = scene::CAM_PITCH + 6.0 * (2.0 * PI * u).sin();
        let distance = scene::CAM_DISTANCE * (1.0 - 0.3 * (0.5 - 0.5 * (4.0 * PI * u).cos())); // dos acercamientos
        // Primera parte al atardecer, transicion y luego noche lluviosa
        let station = arrietty_rt::sky::smoothstep(0.4, 0.6, u);

        let scene_data = scene::build_scene(time, station);
        let camera = Camera::orbit(scene::CAM_TARGET, distance, yaw, pitch, scene::CAM_VFOV, width as f64 / height as f64, 0.0);
        let t0 = Instant::now();
        let mut hdr = renderer::render_hdr(&camera, &scene_data, width, height, samples, max_depth, 0x5EED ^ i as u64);
        let streaks = scene::rain_streaks(time, station);
        if !streaks.is_empty() {
            let depth = renderer::depth_map(&camera, &scene_data, width, height);
            let tint = scene_data.sky.ambient().0 * 1.4 + arrietty_rt::vec3::Vec3::new(0.25, 0.28, 0.33);
            renderer::draw_rain(&mut hdr, width, height, &camera, &|k| depth[k], &streaks, tint, 0.55);
        }
        let params = PostParams { bloom: 0.25 + 0.4 * station as f32, sun_uv: scene::sun_screen(&camera), rays: 0.55 * (1.0 - station as f32), ..PostParams::default() };
        let image = post::finish(&hdr, width, height, &params);

        let path = format!("frames/frame_{i:04}.png");
        png::write_png(&path, width, height, &image).expect("no se pudo escribir el PNG");
        println!("  cuadro {:>4}/{} ({:.2}s) -> {}", i + 1, num_frames, t0.elapsed().as_secs_f64(), path);
    }

    println!("Listo en {:.1?}. Para armar el video:", start.elapsed());
    println!("  ffmpeg -y -framerate {fps} -i frames/frame_%04d.png -c:v libx264 -pix_fmt yuv420p -crf 18 renders/arrietty.mp4");
}

fn arg_or(args: &[String], idx: usize, default: f64) -> f64 {
    args.get(idx).and_then(|s| s.parse().ok()).unwrap_or(default)
}
