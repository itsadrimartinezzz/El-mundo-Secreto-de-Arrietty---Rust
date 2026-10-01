use arrietty_rt::{camera::Camera, renderer, scene, post::{self, PostParams}, taa::Taa};
use std::time::Instant;
fn main() {
    let w = 960; let h = 540;
    for view in [0, 3] {
        let v = &scene::VIEWS[view];
        let camera = Camera::orbit(v.target, v.dist, v.yaw, v.pitch, scene::CAM_VFOV, w as f64 / h as f64, 0.0);
        let data = scene::build_scene(3.0, 0.0);
        let mut frame = vec![0.0; (w*h*6) as usize];
        let mut taa = Taa::new();
        let params = PostParams { glow_divisor: 8, sharpen: 0.10, sun_uv: scene::sun_screen(&camera), rays: 0.55, ..PostParams::default() };
        let mut times = Vec::new();
        let mut checksum = 0u64;
        for i in 0..9 {
            let start = Instant::now();
            renderer::render_taa_frame(&camera, &data, w, h, 4, (0.0,0.0), 0xA11CE, &mut frame);
            let trace = start.elapsed().as_secs_f64()*1000.0;
            let hdr = taa.resolve(&frame,w,h,&camera);
            let image = post::finish_scaled(hdr,w,h,1280,720,&params);
            let total = start.elapsed().as_secs_f64()*1000.0;
            let hash = image.iter().fold(0xcbf29ce484222325u64, |h,b| (h ^ *b as u64).wrapping_mul(0x100000001b3));
            if i>0 { assert_eq!(checksum,hash); }
            checksum=hash;
            if i>=2 { times.push((trace,total)); }
        }
        times.sort_by(|a,b| a.1.total_cmp(&b.1));
        let median=times[3];
        println!("{},{},{:.3},{:.3},{:.3},{:.3},{:016x}",renderer::parallel_threads(),view,median.0,median.1,times[0].1,times[6].1,checksum);
    }
}

