//! Reproduce la imagen de la ventana (1 muestra por cuadro + TAA), sin abrir Win32.
use arrietty_rt::{camera::Camera, png, post::{self, PostParams}, renderer, scene, taa::Taa};
fn main() {
    let w: u32 = std::env::args().nth(1).and_then(|v| v.parse().ok()).unwrap_or(1280);
    let h = w * 9 / 16;
    let view_index: usize = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(0);
    let view = &scene::VIEWS[view_index];
    let camera = Camera::orbit(view.target, view.dist, view.yaw, view.pitch, scene::CAM_VFOV, w as f64 / h as f64, 0.0);
    let data = scene::build_scene(3.0, 0.0);
    let mut frame = vec![0.0; (w * h * 6) as usize];
    let mut taa = Taa::new();
    let mut render_ms = 0.0;
    let mut post_ms = 0.0;
    let mut image = Vec::new();
    let params = PostParams { bloom: 0.25, sharpen: 0.10, sun_uv: scene::sun_screen(&camera), rays: 0.55, ..PostParams::default() };
    let start = std::time::Instant::now();
    let mut previous = Vec::new();
    for iteration in 0..16 {
        let tick = std::time::Instant::now();
        renderer::render_taa_frame(&camera, &data, w, h, 4, (0.0, 0.0), 0xA11CE, &mut frame);
        let hdr = taa.resolve(&frame, w, h, &camera);
        render_ms += tick.elapsed().as_secs_f64() * 1000.0;
        let tick = std::time::Instant::now();
        image = post::finish_scaled(hdr, w, h, 1280, 720, &params);
        post_ms += tick.elapsed().as_secs_f64() * 1000.0;
        if iteration > 0 { assert_eq!(image, previous, "Una escena fija debe producir cuadros identicos"); }
        previous.clone_from(&image);
    }
    std::fs::create_dir_all("renders").unwrap();
    png::write_png(&format!("renders/live_{w}_view{view_index}.png"), 1280, 720, &image).unwrap();
    println!("Promedio: render+TAA {:.1} ms, post {:.1} ms, {:.1} FPS", render_ms / 16.0, post_ms / 16.0, 16000.0 / (render_ms + post_ms));
    println!("16 cuadros nativos {w}x{h} + TAA: {:?}", start.elapsed());
}

