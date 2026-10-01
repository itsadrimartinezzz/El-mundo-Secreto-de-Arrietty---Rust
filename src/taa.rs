use crate::camera::Camera;
use crate::renderer::par_rows;
use crate::vec3::Vec3;

// Secuencia de Halton: desplazamientos sub-pixel bien repartidos para el antialiasing temporal
fn halton(mut i: u32, base: u32) -> f64 {
    let (mut f, mut r) = (1.0, 0.0);
    while i > 0 {
        f /= base as f64;
        r += f * (i % base) as f64;
        i /= base;
    }
    r
}

// Antialiasing temporal: mezcla cada cuadro con el historial reproyectado desde la camara anterior
pub struct Taa {
    width: u32,
    height: u32,
    history: Vec<f32>,
    next: Vec<f32>,
    prev_cam: Option<Camera>,
    frame: u32,
    pub blend: f32, // peso del cuadro nuevo (menor = mas suave, mas estela)
}

impl Default for Taa {
    fn default() -> Self {
        Self::new()
    }

}

impl Taa {
    pub fn new() -> Self {
        Taa { width: 0, height: 0, history: Vec::new(), next: Vec::new(), prev_cam: None, frame: 0, blend: 0.30 }
    }

    // Desplazamiento sub-pixel en [-0.5, 0.5] para el siguiente cuadro
    pub fn jitter(&self) -> (f64, f64) {
        let i = self.frame % 16 + 1;
        (halton(i, 2) - 0.5, halton(i, 3) - 0.5)
    }

    // Olvida el historial (al cambiar de tamano o al desactivar el TAA)
    pub fn reset(&mut self) {
        self.prev_cam = None;
    }

    // Recibe el cuadro [rgb, posicion] y devuelve la imagen HDR resuelta (rgb por pixel)
    pub fn resolve(&mut self, frame: &[f32], w: u32, h: u32, cam: &Camera) -> &[f32] {
        self.resolve_impl(frame, w, h, cam, None)
    }

    // Igual, para un cuadro en tablero de ajedrez (renderer::render_checker_frame): los pixeles
    // con (x + y + parity) impar no se trazaron y se completan con el historial reproyectado
    pub fn resolve_checker(&mut self, frame: &[f32], w: u32, h: u32, cam: &Camera, parity: u32) -> &[f32] {
        self.resolve_impl(frame, w, h, cam, Some(parity as usize))
    }

    fn resolve_impl(&mut self, frame: &[f32], w: u32, h: u32, cam: &Camera, checker: Option<usize>) -> &[f32] {
        let n = (w * h * 3) as usize;
        if (w, h) != (self.width, self.height) {
            // Cambio de resolucion (calidad automatica): el historial se reescala en vez de
            // perderse, asi la imagen no "salta" cuando la ventana ajusta la resolucion
            let resized = (self.prev_cam.is_some() && self.width > 0 && self.height > 0).then(|| resample(&self.history, self.width as usize, self.height as usize, w as usize, h as usize));
            self.width = w;
            self.height = h;
            self.next = vec![0.0; n];
            match resized {
                Some(hist) => self.history = hist,
                None => {
                    self.history = vec![0.0; n];
                    self.prev_cam = None;
                }
            }
        }
        let (wu, hu) = (w as usize, h as usize);
        let history = &self.history;
        let prev = self.prev_cam;
        let blend = self.blend;
        let stationary = prev.is_some_and(|pc| pc == *cam);

        par_rows(&mut self.next, wu * 3, |y, row| {
            for x in 0..wu {
                let i = (y * wu + x) * 6;
                let cur = [frame[i], frame[i + 1], frame[i + 2]];
                // Pixel no trazado en este cuadro (tablero de ajedrez): sale del historial
                let skipped = checker.is_some_and(|p| (x + y + p) % 2 == 1);
                // La mayor parte del jardin es estatica. No volver a reproyectar,
                // recortar y filtrar un pixel que ya tiene exactamente su color.
                let hi = (y * wu + x) * 3;
                if stationary && history[hi..hi+3] == cur {
                    row[x*3..x*3+3].copy_from_slice(&cur);
                    continue;
                }

                // Con camara fija, acumular en el mismo pixel evita filtrar de nuevo
                // el historial en cada desplazamiento subpixel del antialiasing.
                let projected = if stationary {
                    Some(((x as f64 + 0.5) / w as f64, 1.0 - (y as f64 + 0.5) / h as f64))
                } else {
                    prev.and_then(|pc| pc.project(Vec3::new(frame[i + 3] as f64, frame[i + 4] as f64, frame[i + 5] as f64)))
                };
                let out = match projected {
                    Some((s, t)) => {
                        // Posicion en pixeles del cuadro anterior
                        let px = s * w as f64 - 0.5;
                        let py = (1.0 - t) * h as f64 - 0.5;
                        if px < 0.0 || py < 0.0 || px > (wu - 1) as f64 || py > (hu - 1) as f64 {
                            cur
                        } else {
                            // Caja de color del vecindario 3x3: el historial se recorta a ella para evitar estelas
                            // (un pixel no trazado usa solo sus 4 vecinos en cruz, que si se trazaron)
                            let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
                            for dy in -1i32..=1 {
                                for dx in -1i32..=1 {
                                    if skipped && dx * dy != 0 || skipped && dx == 0 && dy == 0 {
                                        continue;
                                    }
                                    let nx = (x as i32 + dx).clamp(0, wu as i32 - 1) as usize;
                                    let ny = (y as i32 + dy).clamp(0, hu as i32 - 1) as usize;
                                    let j = (ny * wu + nx) * 6;
                                    for c in 0..3 {
                                        lo[c] = lo[c].min(frame[j + c]);
                                        hi[c] = hi[c].max(frame[j + c]);
                                    }
                                }
                            }
                            if skipped && !stationary {
                                // En movimiento, un pixel no trazado en un borde de alto contraste tiene en la cruz
                                // colores de ambos lados y la caja no lo limita (bordes dentados). Se usa solo el par
                                // de vecinos que sigue el borde: arriba/abajo o izquierda/derecha, el mas parecido.
                                let at = |dx: i32, dy: i32| {
                                    let nx = (x as i32 + dx).clamp(0, wu as i32 - 1) as usize;
                                    let ny = (y as i32 + dy).clamp(0, hu as i32 - 1) as usize;
                                    let j = (ny * wu + nx) * 6;
                                    [frame[j], frame[j + 1], frame[j + 2]]
                                };
                                let luma = |p: [f32; 3]| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2];
                                let (l, r, u, d) = (at(-1, 0), at(1, 0), at(0, -1), at(0, 1));
                                let (a, b) = if (luma(l) - luma(r)).abs() <= (luma(u) - luma(d)).abs() { (l, r) } else { (u, d) };
                                for c in 0..3 {
                                    lo[c] = a[c].min(b[c]);
                                    hi[c] = a[c].max(b[c]);
                                }
                            }
                            // Historial con Catmull-Rom: el bilineal promedia vecinos en cada cuadro y,
                            // repetido mientras la camara se mueve, difumina la imagen hasta que se detiene.
                            // (Con la camara quieta cae justo en el centro del pixel y es una copia exacta.)
                            let (x0, y0) = (px.floor() as usize, py.floor() as usize);
                            let (fx, fy) = ((px - x0 as f64) as f32, (py - y0 as f64) as f32);
                            let (wx, wy) = (catmull_rom(fx), catmull_rom(fy));
                            let xs: [usize; 4] = std::array::from_fn(|k| (x0 as isize + k as isize - 1).clamp(0, wu as isize - 1) as usize);
                            let ys: [usize; 4] = std::array::from_fn(|k| (y0 as isize + k as isize - 1).clamp(0, hu as isize - 1) as usize);
                            let mut o = [0f32; 3];
                            for c in 0..3 {
                                let mut raw = 0f32;
                                for (j, &yy) in ys.iter().enumerate() {
                                    if wy[j] == 0.0 { continue; }
                                    let row_sum: f32 = (0..4).map(|k| history[(yy * wu + xs[k]) * 3 + c] * wx[k]).sum();
                                    raw += row_sum * wy[j];
                                }
                                // Historial invalido (NaN) se descarta en vez de propagarse
                                let hist = if raw.is_finite() { raw.clamp(lo[c], hi[c]) } else { cur[c] };
                                o[c] = if skipped { hist } else { hist + (cur[c] - hist) * blend };
                            }
                            o
                        }
                    }
                    None => cur,
                };
                row[x * 3..x * 3 + 3].copy_from_slice(&out);
            }
        });

        std::mem::swap(&mut self.history, &mut self.next);
        self.prev_cam = Some(*cam);
        self.frame = self.frame.wrapping_add(1);
        &self.history
    }

}

// Pesos de Catmull-Rom para las 4 muestras alrededor de una posicion fraccionaria t (0..1)
fn catmull_rom(t: f32) -> [f32; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [-0.5 * t + t2 - 0.5 * t3, 1.0 - 2.5 * t2 + 1.5 * t3, 0.5 * t + 2.0 * t2 - 1.5 * t3, -0.5 * t2 + 0.5 * t3]
}

// Reescala una imagen RGB con filtro bilineal (centros de pixel alineados)
fn resample(src: &[f32], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<f32> {
    let mut out = vec![0f32; dw * dh * 3];
    for y in 0..dh {
        let fy = ((y as f32 + 0.5) * sh as f32 / dh as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let (y0, ty) = (fy as usize, fy.fract());
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..dw {
            let fx = ((x as f32 + 0.5) * sw as f32 / dw as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let (x0, tx) = (fx as usize, fx.fract());
            let x1 = (x0 + 1).min(sw - 1);
            for c in 0..3 {
                let p = |xx: usize, yy: usize| src[(yy * sw + xx) * 3 + c];
                let top = p(x0, y0) * (1.0 - tx) + p(x1, y0) * tx;
                let bottom = p(x0, y1) * (1.0 - tx) + p(x1, y1) * tx;
                out[(y * dw + x) * 3 + c] = top * (1.0 - ty) + bottom * ty;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    #[test]
    fn stationary_camera_keeps_pixel_detail_across_jittered_frames() {
        let cam = Camera::orbit(Vec3::ZERO, 3.0, 80.0, 10.0, 55.0, 1.0, 0.0);
        let mut taa = Taa::new();
        let mut rng = Rng::new(123);
        let (w, h) = (8usize, 8usize);
        for _ in 0..16 {
            let (jx, jy) = taa.jitter();
            let mut frame = vec![0.0f32; w * h * 6];
            for y in 0..h { for x in 0..w {
                let i = (y * w + x) * 6;
                let value = ((x + y) % 2) as f32;
                let point = cam.get_ray((x as f64 + 0.5 + jx) / w as f64, 1.0 - (y as f64 + 0.5 + jy) / h as f64, &mut rng).at(1.0);
                frame[i..i+6].copy_from_slice(&[value, value, value, point.x as f32, point.y as f32, point.z as f32]);
            }}
            let result = taa.resolve(&frame, w as u32, h as u32, &cam);
            for y in 0..h { for x in 0..w {
                assert!((result[(y * w + x) * 3] - ((x + y) % 2) as f32).abs() < 1e-5);
            }}
        }
        // Al cambiar de resolucion el historial se reescala (una imagen constante sigue constante)
        let frame = vec![0.5; 4 * 4 * 6];
        taa.resolve(&frame, 4, 4, &cam);
        let frame = vec![0.5; 6 * 6 * 6];
        assert!(taa.resolve(&frame, 6, 6, &cam).iter().all(|v| (*v - 0.5).abs() < 1e-6));
        taa.reset();
        let frame = vec![0.25; w * h * 6];
        assert!(taa.resolve(&frame, w as u32, h as u32, &cam).iter().all(|v| (*v - 0.25).abs() < 1e-6));
    }

    #[test]
    fn checkerboard_fills_skipped_pixels_from_history() {
        let cam = Camera::orbit(Vec3::ZERO, 3.0, 80.0, 10.0, 55.0, 1.0, 0.0);
        let mut taa = Taa::new();
        let (w, h) = (6usize, 6usize);
        // Dos cuadros con paridades opuestas: cada pixel se traza una vez con su propio valor
        let value = |x: usize, y: usize| (x * 7 + y * 3) as f32 / 50.0;
        for parity in [0u32, 1] {
            let mut frame = vec![0.0f32; w * h * 6];
            for y in 0..h { for x in 0..w {
                let v = if (x + y + parity as usize) % 2 == 0 { value(x, y) } else { 0.0 };
                frame[(y * w + x) * 6..(y * w + x) * 6 + 3].copy_from_slice(&[v, v, v]);
            }}
            taa.blend = 1.0;
            let out = taa.resolve_checker(&frame, w as u32, h as u32, &cam, parity).to_vec();
            if parity == 1 {
                // Los no trazados en el segundo cuadro conservan lo del primero (recortado a sus vecinos)
                for y in 1..h - 1 { for x in 1..w - 1 {
                    let n = [value(x - 1, y), value(x + 1, y), value(x, y - 1), value(x, y + 1)];
                    let expect = value(x, y).clamp(n.iter().cloned().fold(f32::MAX, f32::min), n.iter().cloned().fold(f32::MIN, f32::max));
                    assert!((out[(y * w + x) * 3] - expect).abs() < 1e-6, "pixel {x},{y}");
                }}
            }
        }
    }
}
