use std::sync::{Condvar, Mutex, OnceLock};

use crate::camera::Camera;
use crate::render;
use crate::rng::Rng;
use crate::scene::SceneData;
use crate::vec3::{Color, Vec3};

thread_local! { static BACKGROUND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

// La exportacion usa solo su hilo: nunca toma el mutex del pool interactivo.
pub fn background_render<T>(f: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset { fn drop(&mut self) { BACKGROUND.with(|v| v.set(self.0)); } }
    let _reset = Reset(BACKGROUND.with(|v| v.replace(true)));
    f()
}

// Trabajo actual del pool: referencia a un closure que vive en la pila de quien llamo a par_rows
type Job = &'static (dyn Fn() + Sync);

struct PoolState {
    job: Option<Job>,
    generation: u64, // cambia con cada trabajo nuevo para despertar a los hilos
    active: usize,   // hilos que aun no terminan el trabajo actual
}

// Pool de hilos persistente: crear hilos en Windows cuesta milisegundos, asi que se crean una sola vez
struct Pool {
    state: Mutex<PoolState>,
    work: Condvar,
    done: Condvar,
    workers: usize,
    submit: Mutex<()>, // un solo trabajo a la vez
}

fn pool() -> &'static Pool {
    static POOL: OnceLock<&'static Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        let workers = parallel_threads() - 1;
        let pool: &'static Pool = Box::leak(Box::new(Pool {
            state: Mutex::new(PoolState { job: None, generation: 0, active: 0 }),
            work: Condvar::new(),
            done: Condvar::new(),
            workers,
            submit: Mutex::new(()),
        }));
        for _ in 0..workers {
            std::thread::spawn(move || {
                let mut seen = 0u64;
                loop {
                    let job = {
                        let mut st = pool.state.lock().unwrap();
                        while st.generation == seen {
                            st = pool.work.wait(st).unwrap();
                        }
                        seen = st.generation;
                        st.job
                    };
                    if let Some(job) = job {
                        job();
                    }
                    let mut st = pool.state.lock().unwrap();
                    st.active -= 1;
                    if st.active == 0 {
                        pool.done.notify_all();
                    }
                }
            });
        }
        pool
    })
}

// Total incluye el hilo que llama. Se fija al iniciar el proceso para medir
// el mismo trabajo con distintos grados de paralelismo sin cambiar calidad.
pub fn parallel_threads() -> usize {
    static THREADS: OnceLock<usize> = OnceLock::new();
    *THREADS.get_or_init(|| {
        let available = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        std::env::var("ARRIETTY_THREADS").ok().and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(available).clamp(1, available)
    })
}

// Reparte las filas entre todos los nucleos: cada hilo toma la siguiente fila libre
pub fn par_rows<T: Send>(buf: &mut [T], row_len: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    if buf.len() < 8192 || BACKGROUND.with(|v| v.get()) || parallel_threads() == 1 {
        for (y, row) in buf.chunks_mut(row_len).enumerate() { f(y, row); }
        return;
    }
    assert!(row_len > 0);
    // Cada indice se entrega una sola vez. Evita bloquear todos los hilos
    // en un mutex por cada fila, especialmente en los pases de postprocesado.
    struct Rows<T> { ptr: *mut T, len: usize }
    unsafe impl<T: Send> Sync for Rows<T> {}
    impl<T> Rows<T> {
        unsafe fn row(&self, offset: usize, len: usize) -> &mut [T] {
            std::slice::from_raw_parts_mut(self.ptr.add(offset), len)
        }
    }
    let rows = Rows { ptr: buf.as_mut_ptr(), len: buf.len() };
    let next_row = std::sync::atomic::AtomicUsize::new(0);
    let count = rows.len.div_ceil(row_len);
    let work = || loop {
        let y = next_row.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if y >= count { break; }
        let offset = y * row_len;
        // SAFETY: fetch_add asigna filas disjuntas; buf vive hasta que
        // todos los trabajadores terminan (done), y no se usa por otra via.
        let row = unsafe { rows.row(offset, row_len.min(rows.len - offset)) };
        f(y, row);
    };
    let p = pool();
    let _turn = p.submit.lock().unwrap();
    let job: &(dyn Fn() + Sync) = &work;
    // SAFETY: esta funcion no regresa hasta que todos los hilos terminaron de usar `job`
    let job: Job = unsafe { std::mem::transmute::<&(dyn Fn() + Sync), Job>(job) };
    {
        let mut st = p.state.lock().unwrap();
        st.job = Some(job);
        st.active = p.workers;
        st.generation += 1;
        p.work.notify_all();
    }
    work(); // el hilo que llama tambien trabaja
    let mut st = p.state.lock().unwrap();
    while st.active > 0 {
        st = p.done.wait(st).unwrap();
    }
    st.job = None;
}

fn row_rng(seed: u64, y: usize) -> Rng {
    Rng::new(seed ^ (y as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

// Imagen HDR (RGB lineal f32) con varias muestras por pixel: para la imagen fija y el turntable
#[allow(clippy::too_many_arguments)]
pub fn render_hdr(camera: &Camera, scene: &SceneData, width: u32, height: u32, samples: u32, max_depth: u32, seed: u64) -> Vec<f32> {
    let mut hdr = vec![0f32; width as usize * height as usize * 3];
    let n = samples.max(1);
    par_rows(&mut hdr, width as usize * 3, |y, row| {
        let mut rng = row_rng(seed, y);
        for x in 0..width as usize {
            let mut c = Color::ZERO;
            for _ in 0..n {
                let u = (x as f64 + rng.f64()) / width as f64;
                let v = 1.0 - (y as f64 + rng.f64()) / height as f64;
                c += render::ray_color(&camera.get_ray(u, v, &mut rng), scene, max_depth, &mut rng);
            }
            c = c / n as f64;
            row[x * 3] = c.x as f32;
            row[x * 3 + 1] = c.y as f32;
            row[x * 3 + 2] = c.z as f32;
        }
    });
    hdr
}

// Una muestra por pixel desplazada por `jitter`; guarda [r,g,b, x,y,z del punto visto] por pixel para el TAA
#[allow(clippy::too_many_arguments)]
pub fn render_taa_frame(camera: &Camera, scene: &SceneData, width: u32, height: u32, max_depth: u32, jitter: (f64, f64), seed: u64, out: &mut [f32]) {
    par_rows(out, width as usize * 6, |y, row| {
        let mut rng = row_rng(seed, y);
        for x in 0..width as usize {
            let u = (x as f64 + 0.5 + jitter.0) / width as f64;
            let v = 1.0 - (y as f64 + 0.5 + jitter.1) / height as f64;
            let (c, p) = render::trace_primary(&camera.get_ray(u, v, &mut rng), scene, max_depth, &mut rng);
            let o = &mut row[x * 6..x * 6 + 6];
            o.copy_from_slice(&[c.x as f32, c.y as f32, c.z as f32, p.x as f32, p.y as f32, p.z as f32]);
        }
    });
}

// Como render_taa_frame, pero traza solo la mitad de los pixeles en tablero de ajedrez
// ((x + y + parity) par); la paridad se alterna cada cuadro y Taa::resolve_checker completa el resto.
// Los no trazados quedan con el promedio de sus vecinos de fila y la posicion de uno de ellos (para reproyectar).
#[allow(clippy::too_many_arguments)]
pub fn render_checker_frame(camera: &Camera, scene: &SceneData, width: u32, height: u32, max_depth: u32, parity: u32, seed: u64, out: &mut [f32]) {
    let w = width as usize;
    par_rows(out, w * 6, |y, row| {
        let mut rng = row_rng(seed, y);
        let first = (y + parity as usize) % 2; // primer x trazado en esta fila
        for x in (first..w).step_by(2) {
            let u = (x as f64 + 0.5) / width as f64;
            let v = 1.0 - (y as f64 + 0.5) / height as f64;
            let (c, p) = render::trace_primary(&camera.get_ray(u, v, &mut rng), scene, max_depth, &mut rng);
            row[x * 6..x * 6 + 6].copy_from_slice(&[c.x as f32, c.y as f32, c.z as f32, p.x as f32, p.y as f32, p.z as f32]);
        }
        let o = camera.origin;
        let dist = |row: &[f32], i: usize| ((row[i * 6 + 3] as f64 - o.x).powi(2) + (row[i * 6 + 4] as f64 - o.y).powi(2) + (row[i * 6 + 5] as f64 - o.z).powi(2)).sqrt();
        for x in (1 - first..w).step_by(2) {
            let (l, r) = (x.checked_sub(1), (x + 1 < w).then_some(x + 1));
            let (a, b) = (l.or(r).unwrap_or(x), r.or(l).unwrap_or(x));
            // Posicion para reproyectar: el punto medio de los vecinos sobre la misma superficie;
            // en un borde de objeto (distancias muy distintas) la del vecino mas cercano a la camara
            let (da, db) = (dist(row, a), dist(row, b));
            let (wa, wb) = if (da - db).abs() < 0.05 * da.min(db) { (0.5, 0.5) } else if da < db { (1.0, 0.0) } else { (0.0, 1.0) };
            for k in 0..3 {
                row[x * 6 + k] = 0.5 * (row[a * 6 + k] + row[b * 6 + k]);
                row[x * 6 + 3 + k] = row[a * 6 + 3 + k] * wa + row[b * 6 + 3 + k] * wb;
            }
        }
    });
}

// Una muestra centrada por pixel: distancia desde la camara al primer objeto (para tapar la lluvia dibujada)
pub fn depth_map(camera: &Camera, scene: &SceneData, width: u32, height: u32) -> Vec<f32> {
    let mut depth = vec![0f32; width as usize * height as usize];
    par_rows(&mut depth, width as usize, |y, row| {
        let mut rng = row_rng(1, y);
        for (x, d) in row.iter_mut().enumerate() {
            let u = (x as f64 + 0.5) / width as f64;
            let v = 1.0 - (y as f64 + 0.5) / height as f64;
            let ray = camera.get_ray(u, v, &mut rng);
            *d = scene.hit(&ray, 0.001, f64::INFINITY).map_or(f32::INFINITY, |r| (r.t * ray.dir.length()) as f32);
        }
    });
    depth
}

// Dibuja trazos de lluvia sobre la imagen HDR respetando la profundidad (lo que esta delante los tapa)
#[allow(clippy::too_many_arguments)]
pub fn draw_rain(hdr: &mut [f32], width: u32, height: u32, camera: &Camera, depth: &dyn Fn(usize) -> f32, segments: &[(Vec3, Vec3)], color: Color, strength: f64) {
    let (w, h) = (width as f64, height as f64);
    let target = [color.x as f32, color.y as f32, color.z as f32];
    for &(a, b) in segments {
        let (Some((sa, ta)), Some((sb, tb))) = (camera.project(a), camera.project(b)) else { continue };
        let dist = (a - camera.origin).length();
        let alpha = (strength * (1.0 - dist / 45.0).clamp(0.15, 1.0)) as f32; // mas tenue a lo lejos
        let (x0, y0, x1, y1) = (sa * w, (1.0 - ta) * h, sb * w, (1.0 - tb) * h);
        let steps = ((x1 - x0).abs().max((y1 - y0).abs()).ceil() as usize).clamp(1, 400);
        for k in 0..=steps {
            let f = k as f64 / steps as f64;
            let (x, y) = (x0 + (x1 - x0) * f, y0 + (y1 - y0) * f);
            if x < 0.0 || y < 0.0 || x >= w || y >= h {
                continue;
            }
            let i = y as usize * width as usize + x as usize;
            if depth(i) < dist as f32 {
                continue; // detras de algo
            }
            let a = alpha * (0.35 + 0.65 * f as f32); // la cola se desvanece
            for c in 0..3 {
                let px = &mut hdr[i * 3 + c];
                *px += (target[c] - *px) * a;
            }
        }
    }
}

#[cfg(test)]
mod background_tests {
    use super::*;
    #[test]
    fn parallel_rows_cover_partial_and_empty_buffers_exactly_once() {
        for len in [0, 1, 7, 513, 4099] {
            let mut buf = vec![0usize; len];
            let counts: Vec<_> = (0..len.div_ceil(7)).map(|_| std::sync::atomic::AtomicUsize::new(0)).collect();
            par_rows(&mut buf, 7, |y, row| {
                counts[y].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                for (x, value) in row.iter_mut().enumerate() { *value = y * 7 + x + 1; }
            });
            assert_eq!(buf, (1..=len).collect::<Vec<_>>());
            assert!(counts.iter().all(|c| c.load(std::sync::atomic::Ordering::Relaxed) == 1));
        }
    }
    #[test]
    fn export_does_not_lock_interactive_pool() {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || background_render(|| {
            let rx = Mutex::new(release_rx);
            par_rows(&mut [0u8], 1, |_, _| { started_tx.send(()).unwrap(); rx.lock().unwrap().recv().unwrap(); });
        }));
        started_rx.recv().unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let foreground = std::thread::spawn(move || { par_rows(&mut [0u8; 4], 1, |_, row| row[0] = 1); done_tx.send(()).unwrap(); });
        let completed = done_rx.recv_timeout(std::time::Duration::from_secs(3)).is_ok();
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        foreground.join().unwrap();
        assert!(completed, "La exportacion bloqueo el pool interactivo");
    }
}
