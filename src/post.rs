use std::cell::RefCell;

use crate::renderer::par_rows;

// Parametros del post-procesado
#[derive(Clone, Copy)]
pub struct PostParams {
    pub exposure: f32,
    pub bloom: f32,                  // intensidad del resplandor
    pub threshold: f32,              // desde que brillo empieza el bloom
    pub vignette: f32,
    pub aberration: f32,             // separacion de canales en los bordes (0 = apagada)
    pub saturation: f32,             // 1 = sin cambio, >1 colores mas vivos
    pub sharpen: f32,                // enfoque despues del TAA (0 = apagado)
    pub sun_uv: Option<(f32, f32)>,  // posicion del sol en pantalla (0..1) para los rayos de luz
    pub rays: f32,                   // intensidad de los rayos de luz
    pub bgra: bool,                  // salida BGRA de 32 bits para GDI (si no, RGB para PNG)
    pub glow_divisor: usize,         // solo resplandores; no reduce detalle de la imagen
}

impl Default for PostParams {
    fn default() -> Self {
        PostParams { exposure: 1.1, bloom: 0.25, threshold: 1.0, vignette: 0.18, aberration: 0.0, saturation: 1.12, sharpen: 0.45, sun_uv: None, rays: 0.0, bgra: false, glow_divisor: 4 }
    }
}

// Imagen RGB f32 simple con muestreo bilineal
struct Img {
    w: usize,
    h: usize,
    px: Vec<f32>,
}

impl Img {
    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        ImgRef { w: self.w, h: self.h, px: &self.px }.sample(x, y)
    }
}

// Vista prestada de una imagen (evita copiar el HDR completo)
struct ImgRef<'a> {
    w: usize,
    h: usize,
    px: &'a [f32],
}

impl ImgRef<'_> {
    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        let x = x.clamp(0.0, (self.w - 1) as f32);
        let y = y.clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let mut o = [0f32; 3];
        for (c, oc) in o.iter_mut().enumerate() {
            let p = |xx: usize, yy: usize| self.px[(yy * self.w + xx) * 3 + c];
            *oc = (p(x0, y0) * (1.0 - fx) + p(x1, y0) * fx) * (1.0 - fy) + (p(x0, y1) * (1.0 - fx) + p(x1, y1) * fx) * fy;
        }
        o
    }
}

// Buferes de trabajo que se reutilizan entre cuadros: en Windows pedir varios megas nuevos
// en cada cuadro cuesta milisegundos en fallos de pagina (la memoria llega vacia del sistema)
#[derive(Default)]
struct Scratch {
    near_raw: Vec<f32>,
    near: Vec<f32>,
    wide: Vec<f32>,
    rays: Vec<f32>,
    disp: Vec<f32>,
    soft: Vec<f32>,
    horizontal: Vec<f32>,
    blur_src: Vec<f32>,
}

thread_local! { static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::default()); }

// Bufer reutilizado con el tamano pedido (cada pasada escribe todos sus elementos)
fn reuse(mut v: Vec<f32>, n: usize) -> Vec<f32> {
    v.resize(n, 0.0);
    v
}

// Reduce la imagen promediando bloques de f x f, quedandose solo con lo que supera el umbral
fn downsample(src: &[f32], w: usize, h: usize, f: usize, threshold: f32, buf: Vec<f32>) -> Img {
    let (dw, dh) = ((w / f).max(1), (h / f).max(1));
    let mut px = reuse(buf, dw * dh * 3);
    par_rows(&mut px, dw * 3, |y, row| {
        for x in 0..dw {
            let mut acc = [0f32; 3];
            for sy in y * f..((y + 1) * f).min(h) {
                for sx in x * f..((x + 1) * f).min(w) {
                    let i = (sy * w + sx) * 3;
                    let luma = 0.2126 * src[i] + 0.7152 * src[i + 1] + 0.0722 * src[i + 2];
                    let k = if luma > threshold { (luma - threshold) / luma.max(1e-4) } else { 0.0 };
                    for c in 0..3 {
                        acc[c] += src[i + c] * k;
                    }
                }
            }
            let n = (f * f) as f32;
            for c in 0..3 {
                row[x * 3 + c] = acc[c] / n;
            }
        }
    });
    Img { w: dw, h: dh, px }
}

// Desenfoque gaussiano separable (horizontal y luego vertical)
fn blur(img: &mut Img, radius: i32, src: &mut Vec<f32>) {
    let sigma = radius as f32 / 2.0;
    let weights: Vec<f32> = (-radius..=radius).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let norm: f32 = weights.iter().sum();
    let (w, h) = (img.w, img.h);
    for pass in 0..2 {
        src.resize(img.px.len(), 0.0);
        std::mem::swap(src, &mut img.px);
        let src = &*src;
        par_rows(&mut img.px, w * 3, |y, row| {
            for x in 0..w {
                let mut acc = [0f32; 3];
                for (k, wt) in weights.iter().enumerate() {
                    let o = k as i32 - radius;
                    let (sx, sy) = if pass == 0 { ((x as i32 + o).clamp(0, w as i32 - 1) as usize, y) } else { (x, (y as i32 + o).clamp(0, h as i32 - 1) as usize) };
                    let i = (sy * w + sx) * 3;
                    for c in 0..3 {
                        acc[c] += src[i + c] * wt;
                    }
                }
                for c in 0..3 {
                    row[x * 3 + c] = acc[c] / norm;
                }
            }
        });
    }
}

// Curva filmica ACES (aproximacion de Narkowicz)
fn aces(x: f32) -> f32 {
    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0)
}

// HDR lineal -> bytes de 8 bits: bloom, rayos de luz, saturacion, ACES, gamma, vineta, enfoque y dithering
pub fn finish(hdr: &[f32], w: u32, h: u32, p: &PostParams) -> Vec<u8> {
    finish_scaled(hdr, w, h, w, h, p)
}

// Igual que `finish`, pero escala a out_w x out_h dentro de la misma pasada
pub fn finish_scaled(hdr: &[f32], w: u32, h: u32, out_w: u32, out_h: u32, p: &PostParams) -> Vec<u8> {
    let mut out = Vec::new();
    finish_scaled_into(hdr, w, h, out_w, out_h, p, &mut out);
    out
}

// Version para la ventana en vivo: escribe en `out` y reutiliza los buferes de trabajo del hilo
pub fn finish_scaled_into(hdr: &[f32], w: u32, h: u32, out_w: u32, out_h: u32, p: &PostParams, out: &mut Vec<u8>) {
    SCRATCH.with(|s| finish_with(&mut s.borrow_mut(), hdr, w, h, out_w, out_h, p, out));
}

#[allow(clippy::too_many_arguments)]
fn finish_with(s: &mut Scratch, hdr: &[f32], w: u32, h: u32, out_w: u32, out_h: u32, p: &PostParams, out: &mut Vec<u8>) {
    let (wu, hu) = (w as usize, h as usize);
    let (ow, oh) = (out_w as usize, out_h as usize);
    // Bloom en dos escalas: halo cercano (1/4) y resplandor amplio (1/16)
    let divisor = p.glow_divisor.clamp(4,8);
    let near_raw = downsample(hdr, wu, hu, divisor, p.threshold, std::mem::take(&mut s.near_raw));
    let mut near_px = std::mem::take(&mut s.near);
    near_px.clear();
    near_px.extend_from_slice(&near_raw.px);
    let mut near = Img { w: near_raw.w, h: near_raw.h, px: near_px };
    let mut wide = downsample(&near.px, near.w, near.h, 4, 0.0, std::mem::take(&mut s.wide));
    let radius = (20.0 / divisor as f32).round() as i32;
    blur(&mut near, radius, &mut s.blur_src);
    blur(&mut wide, radius, &mut s.blur_src);

    // Rayos de luz: desenfoque radial de lo brillante (cielo y sol entre las hojas) hacia el sol
    let rays = p.sun_uv.filter(|_| p.rays > 0.0).map(|(su, sv)| {
        let (rw, rh) = (near_raw.w, near_raw.h);
        let (sx, sy) = (su * rw as f32, sv * rh as f32);
        let mut px = reuse(std::mem::take(&mut s.rays), rw * rh * 3);
        par_rows(&mut px, rw * 3, |y, row| {
            const N: usize = 28;
            for x in 0..rw {
                let (mut qx, mut qy) = (x as f32, y as f32);
                let (dx, dy) = ((sx - qx) / N as f32 * 0.9, (sy - qy) / N as f32 * 0.9);
                let (mut acc, mut decay) = ([0f32; 3], 1.0f32);
                for _ in 0..N {
                    qx += dx;
                    qy += dy;
                    let c = near_raw.sample(qx, qy);
                    for k in 0..3 {
                        acc[k] += c[k] * decay;
                    }
                    decay *= 0.94;
                }
                for k in 0..3 {
                    row[x * 3 + k] = acc[k] / N as f32;
                }
            }
        });
        Img { w: rw, h: rh, px }
    });
    let src = ImgRef { w: wu, h: hu, px: hdr };

    // Paso 1, a la resolucion trazada: bloom, rayos, saturacion, exposicion, vineta, ACES y gamma -> color 0..1
    let mut disp = reuse(std::mem::take(&mut s.disp), wu * hu * 3);
    par_rows(&mut disp, wu * 3, |y, row| {
        for x in 0..wu {
            let (fx, fy) = (x as f32, y as f32);
            let (u, v) = ((fx + 0.5) / wu as f32 - 0.5, (fy + 0.5) / hu as f32 - 0.5);
            let r2 = u * u + v * v;
            let i = (y * wu + x) * 3;
            let mut col = [hdr[i], hdr[i + 1], hdr[i + 2]];
            if p.aberration > 0.0 {
                let shift = p.aberration * r2 * wu as f32;
                col[0] = src.sample(fx + u * shift, fy + v * shift)[0];
                col[2] = src.sample(fx - u * shift, fy - v * shift)[2];
            }
            let bn = near.sample((fx + 0.5) / divisor as f32 - 0.5, (fy + 0.5) / divisor as f32 - 0.5);
            let bw = wide.sample((fx + 0.5) / (divisor*4) as f32 - 0.5, (fy + 0.5) / (divisor*4) as f32 - 0.5);
            let ry = rays.as_ref().map(|r| r.sample((fx + 0.5) / divisor as f32 - 0.5, (fy + 0.5) / divisor as f32 - 0.5)).unwrap_or([0.0; 3]);
            let vig = 1.0 - p.vignette * r2 * 2.0;
            let mut c = [0f32; 3];
            for k in 0..3 {
                c[k] = col[k] + (bn[k] * 0.6 + bw[k] * 0.9) * p.bloom + ry[k] * p.rays;
            }
            let lum = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            for k in 0..3 {
                let v = (lum + (c[k] - lum) * p.saturation).max(0.0) * p.exposure * vig;
                row[x * 3 + k] = aces(v).sqrt();
            }
        }
    });

    // Enfoque: resalta lo que el TAA y el reescalado suavizan (mascara de desenfoque en cruz)
    if p.sharpen > 0.0 {
        s.soft.clear();
        s.soft.extend_from_slice(&disp);
        let soft = &s.soft;
        par_rows(&mut disp, wu * 3, |y, row| {
            let (yu, yd) = (y.saturating_sub(1), (y + 1).min(hu - 1));
            for x in 0..wu {
                let (xl, xr) = (x.saturating_sub(1), (x + 1).min(wu - 1));
                for k in 0..3 {
                    let c = soft[(y * wu + x) * 3 + k];
                    let avg = (soft[(yu * wu + x) * 3 + k] + soft[(yd * wu + x) * 3 + k] + soft[(y * wu + xl) * 3 + k] + soft[(y * wu + xr) * 3 + k]) * 0.25;
                    row[x * 3 + k] = (c + (c - avg) * p.sharpen).clamp(0.0, 1.0);
                }
            }
        });
    }

    // Catmull-Rom separable: conserva mejor el detalle que el bilineal.
    // Pesos precalculados por eje; ocho lecturas en vez de dieciseis por pixel.
    let native = wu == ow && hu == oh;
    let ys = if native { Vec::new() } else { resize_taps(hu, oh) };
    let horizontal = if native {
        std::mem::take(&mut s.horizontal)
    } else {
        let xs = resize_taps(wu, ow);
        let mut horizontal = reuse(std::mem::take(&mut s.horizontal), ow * hu * 3);
        par_rows(&mut horizontal, ow * 3, |y, row| {
            for (x, (indices, weights)) in xs.iter().enumerate() {
                for k in 0..3 {
                    row[x * 3 + k] = (0..4).map(|t| disp[(y * wu + indices[t]) * 3 + k] * weights[t]).sum();
                }
            }
        });
        horizontal
    };
    let chans = if p.bgra { 4 } else { 3 };
    out.resize(ow * oh * chans, 0);
    par_rows(out, ow * chans, |y, row| {
        for x in 0..ow {
            let c: [f32; 3] = if native {
                std::array::from_fn(|k| disp[(y * wu + x) * 3 + k])
            } else {
                let (indices, weights) = &ys[y];
                std::array::from_fn(|k| (0..4).map(|t| horizontal[(indices[t] * ow + x) * 3 + k] * weights[t]).sum())
            };
            let noise = (((x * 7919 + y * 104729) ^ (x * y)) % 97) as f32 / 97.0 - 0.5;
            let rgb = c.map(|v| (v * 255.0 + noise).clamp(0.0, 255.0) as u8);
            if p.bgra {
                row[x * 4..x * 4 + 4].copy_from_slice(&[rgb[2], rgb[1], rgb[0], 255]);
            } else {
                row[x * 3..x * 3 + 3].copy_from_slice(&rgb);
            }
        }
    });

    // Los buferes vuelven al almacen para el siguiente cuadro
    s.near_raw = near_raw.px;
    s.near = near.px;
    s.wide = wide.px;
    if let Some(r) = rays {
        s.rays = r.px;
    }
    s.disp = disp;
    s.horizontal = horizontal;
}

fn resize_taps(input: usize, output: usize) -> Vec<([usize; 4], [f32; 4])> {
    (0..output).map(|i| {
        let p = ((i as f32 + 0.5) * input as f32 / output as f32 - 0.5).clamp(0.0, (input - 1) as f32);
        let base = p.floor() as isize;
        let t = p - base as f32;
        let t2 = t * t;
        let t3 = t2 * t;
        let indices = std::array::from_fn(|j| (base + j as isize - 1).clamp(0, input as isize - 1) as usize);
        (indices, [-0.5*t + t2 - 0.5*t3, 1.0 - 2.5*t2 + 1.5*t3, 0.5*t + 2.0*t2 - 1.5*t3, -0.5*t2 + 0.5*t3])
    }).collect()
}

#[cfg(test)]
mod resize_tests {
    use super::*;
    #[test]
    fn reconstruction_preserves_constants_and_native_pixels() {
        for (input, output) in [(1, 9), (7, 7), (7, 19), (19, 7)] {
            for (x, (indices, weights)) in resize_taps(input, output).iter().enumerate() {
                assert!(indices.iter().all(|i| *i < input));
                assert!((weights.iter().sum::<f32>() - 1.0).abs() < 1e-5);
                if input == output {
                    let value: f32 = (0..4).map(|j| indices[j] as f32 * weights[j]).sum();
                    assert!((value - x as f32).abs() < 1e-5);
                }
            }
        }
    }
}
