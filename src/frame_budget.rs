//! Ajuste lento con margen para presentacion y variaciones de carga.
// 28 fps: margen sobre los 25 minimos, el resto del tiempo se usa en nitidez
pub const TARGET_FPS: f64 = 28.0;
pub struct FrameBudget { pixels: f64, average: f64, frames: u32 }
impl Default for FrameBudget {
    fn default() -> Self { Self { pixels: 480_000.0, average: 0.0, frames: 0 } }
}
impl FrameBudget {
    pub fn pixels(&self) -> f64 { self.pixels }
    pub fn observe(&mut self, seconds: f64) {
        self.observe_frame(seconds, true);
    }

    // Igual que `observe`, pero `can_grow` dice si ahora se permite subir la resolucion.
    // Cada cambio de resolucion se ve un instante borroso mientras el TAA se reenfoca, asi que:
    // - el margen es amplio (no oscila alrededor del objetivo cambiando cada pocos segundos)
    // - bajar solo cuando de verdad peligran los 25 fps; subir solo si sobra al menos un 20%
    // - la ventana solo deja subir mientras la camara se mueve, donde el reenfoque no se nota
    pub fn observe_frame(&mut self, seconds: f64, can_grow: bool) {
        if !seconds.is_finite() || seconds <= 0.0 { return; }
        self.frames += 1;
        self.average = if self.average == 0.0 { seconds } else { self.average * 0.9 + seconds * 0.1 };
        if self.frames < 120 { return; }
        let target = 1.0 / TARGET_FPS - 0.002;
        let too_slow = self.average > 1.0 / 25.5;
        let spare = self.average < target * 0.80;
        if !too_slow && !(spare && can_grow) {
            return; // se sigue midiendo; se decide en cuanto haga falta o se pueda
        }
        self.frames = 0;
        self.pixels = (self.pixels * (target / self.average).clamp(0.85, 1.08)).clamp(64_000.0, 1_000_000.0);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn responds_to_load_without_reacting_to_one_spike() {
        let mut b = FrameBudget::default();
        let initial = b.pixels();
        b.observe(0.1);
        assert_eq!(b.pixels(), initial);
        for _ in 0..240 { b.observe(0.05); }
        assert!(b.pixels() < initial);
        let slow = b.pixels();
        for _ in 0..240 { b.observe(0.015); }
        assert!(b.pixels() > slow);
        // Cerca del objetivo no cambia (no oscila), y con la camara quieta no sube
        let steady = b.pixels();
        for _ in 0..600 { b.observe(1.0 / TARGET_FPS); }
        assert_eq!(b.pixels(), steady);
        for _ in 0..600 { b.observe_frame(0.010, false); }
        assert_eq!(b.pixels(), steady);
        for _ in 0..600 { b.observe_frame(0.060, false); }
        assert!(b.pixels() < steady, "bajar si se permite aunque la camara este quieta");
    }
}


