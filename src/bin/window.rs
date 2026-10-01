//! Ventana en vivo de la casa de Arrietty: animada, atardecer / noche lluviosa, TAA y post-procesado (Win32 directo, sin crates).
//!
//! Controles:
//!   Arrastrar con clic izquierdo : orbitar (con inercia al soltar)
//!   Rueda del mouse / W S        : acercar / alejar
//!   Flechas o A D                : rotar e inclinar
//!   1 / 2                        : atardecer / noche lluviosa (transicion suave)
//!   N                            : alternar entre los dos ambientes
//!   L                            : alternar ambientes automaticamente
//!   Espacio                      : rotacion automatica de la camara
//!   P                            : pausar / reanudar la animacion
//!   T                            : activar / desactivar el antialiasing temporal
//!   V                            : cambiar de vista
//!   B / G                        : Arrietty / jardin
//!   F                            : guardar el cuadro 4K actual
//!   Q                            : calidad (pixeles trazados por cuadro: rapida .. nativa)
//!   R                            : volver a la vista actual
//!   Esc                          : salir
//!
//! Uso: cargo run --release --bin window

// Sin consola: solo se abre la ventana del diorama
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
fn main() {
    win::run();
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("La ventana en vivo usa Win32 y solo funciona en Windows; usa `cargo run --release` o `--bin anim`.");
}

#[cfg(target_os = "windows")]
mod win {
    #![allow(non_snake_case, non_camel_case_types, dead_code)]

    use std::cell::Cell;
    use std::ffi::c_void;
    use std::ptr;
    use std::time::{Duration, Instant};

    use arrietty_rt::camera::Camera;
    use arrietty_rt::post::{self, PostParams};
    use arrietty_rt::taa::Taa;

    use arrietty_rt::frame_budget::{FrameBudget, TARGET_FPS};
    use arrietty_rt::{audio, png, renderer, scene};

    // ---------- Bindings minimos a Win32 ----------

    type HWND = *mut c_void;
    type HDC = *mut c_void;
    type HINSTANCE = *mut c_void;
    type WNDPROC = Option<unsafe extern "system" fn(HWND, u32, usize, isize) -> isize>;

    #[repr(C)]
    struct WNDCLASSEXW {
        cb_size: u32,
        style: u32,
        lpfn_wnd_proc: WNDPROC,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        h_instance: HINSTANCE,
        h_icon: *mut c_void,
        h_cursor: *mut c_void,
        hbr_background: *mut c_void,
        lpsz_menu_name: *const u16,
        lpsz_class_name: *const u16,
        h_icon_sm: *mut c_void,
    }

    #[repr(C)]
    struct POINT {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct MSG {
        hwnd: HWND,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        pt: POINT,
    }

    #[repr(C)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    struct BITMAPINFOHEADER {
        bi_size: u32,
        bi_width: i32,
        bi_height: i32,
        bi_planes: u16,
        bi_bit_count: u16,
        bi_compression: u32,
        bi_size_image: u32,
        bi_x_pels_per_meter: i32,
        bi_y_pels_per_meter: i32,
        bi_clr_used: u32,
        bi_clr_important: u32,
    }

    #[repr(C)]
    struct BITMAPINFO {
        header: BITMAPINFOHEADER,
        colors: [u32; 1],
    }

    const WS_OVERLAPPEDWINDOW: u32 = 0x00CF_0000;
    const WS_VISIBLE: u32 = 0x1000_0000;
    const CW_USEDEFAULT: i32 = i32::MIN;
    const SW_SHOWNORMAL: i32 = 1;
    const PM_REMOVE: u32 = 1;
    const CS_HREDRAW: u32 = 0x0002;
    const CS_VREDRAW: u32 = 0x0001;
    const IDC_ARROW: usize = 32512;
    const HALFTONE: i32 = 4;
    const COLORONCOLOR: i32 = 3;
    const SRCCOPY: u32 = 0x00CC_0020;

    const WM_DESTROY: u32 = 0x0002;
    const WM_KILLFOCUS: u32 = 0x0008;
    const WM_QUIT: u32 = 0x0012;
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;
    const WM_SYSKEYDOWN: u32 = 0x0104;
    const WM_SYSKEYUP: u32 = 0x0105;
    const WM_MOUSEMOVE: u32 = 0x0200;
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_MOUSEWHEEL: u32 = 0x020A;
    const WM_CAPTURECHANGED: u32 = 0x0215;

    const VK_ESCAPE: usize = 0x1B;
    const VK_SPACE: usize = 0x20;
    const VK_LEFT: usize = 0x25;
    const VK_UP: usize = 0x26;
    const VK_RIGHT: usize = 0x27;
    const VK_DOWN: usize = 0x28;
    const VK_A: usize = 0x41;
    const VK_1: usize = 0x31;
    const VK_2: usize = 0x32;
    const VK_D: usize = 0x44;
    const VK_F: usize = 0x46;
    const VK_B: usize = 0x42;
    const VK_G: usize = 0x47;
    const VK_C: usize = 0x43;
    const VK_L: usize = 0x4C;
    const VK_N: usize = 0x4E;
    const VK_P: usize = 0x50;
    const VK_Q: usize = 0x51;
    const VK_T: usize = 0x54;
    const VK_V: usize = 0x56;
    const VK_R: usize = 0x52;
    const VK_S: usize = 0x53;
    const VK_W: usize = 0x57;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassExW(wc: *const WNDCLASSEXW) -> u16;
        fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: HWND, menu: *mut c_void, inst: HINSTANCE, param: *mut c_void) -> HWND;
        fn DefWindowProcW(hwnd: HWND, msg: u32, w: usize, l: isize) -> isize;
        fn PostQuitMessage(code: i32);
        fn PeekMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32, remove: u32) -> i32;
        fn TranslateMessage(msg: *const MSG) -> i32;
        fn DispatchMessageW(msg: *const MSG) -> isize;
        fn ShowWindow(hwnd: HWND, cmd: i32) -> i32;
        fn GetDC(hwnd: HWND) -> HDC;
        fn ReleaseDC(hwnd: HWND, hdc: HDC) -> i32;
        fn LoadCursorW(inst: HINSTANCE, name: *const u16) -> *mut c_void;
        fn GetClientRect(hwnd: HWND, rect: *mut RECT) -> i32;
        fn SetCapture(hwnd: HWND) -> HWND;
        fn ReleaseCapture() -> i32;
        fn SetProcessDPIAware() -> i32;
        fn SetWindowTextW(hwnd: HWND, text: *const u16) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn StretchDIBits(hdc: HDC, xd: i32, yd: i32, wd: i32, hd: i32, xs: i32, ys: i32, ws: i32, hs: i32, bits: *const c_void, bmi: *const BITMAPINFO, usage: u32, rop: u32) -> i32;
        fn SetStretchBltMode(hdc: HDC, mode: i32) -> i32;
        fn SetDIBitsToDevice(hdc: HDC, xd: i32, yd: i32, w: u32, h: u32, xs: i32, ys: i32, start: u32, lines: u32, bits: *const c_void, bmi: *const BITMAPINFO, usage: u32) -> i32;
        fn SetBrushOrgEx(hdc: HDC, x: i32, y: i32, old: *mut POINT) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[link(name = "winmm")]
    extern "system" {
        fn mciSendStringW(command: *const u16, ret: *mut u16, ret_len: u32, callback: HWND) -> u32;
        fn timeBeginPeriod(period: u32) -> u32;
        fn timeEndPeriod(period: u32) -> u32;
    }

    // Evita que esperas cortas del limitador se redondeen a ~16 ms en Windows.
    struct TimerResolution(bool);
    impl Drop for TimerResolution {
        fn drop(&mut self) { if self.0 { unsafe { timeEndPeriod(1); } } }
    }

    // ---------- Audio: musica de Arrietty en bucle + lluvia, truenos y pajaritos ----------

    // Comando MCI ("open", "play", "setaudio"...); devuelve true si Windows lo acepto
    fn mci(cmd: &str) -> bool {
        let c = wide(cmd);
        unsafe { mciSendStringW(c.as_ptr(), ptr::null_mut(), 0, ptr::null_mut()) == 0 }
    }

    // Busca assets/audio subiendo desde la carpeta actual y desde la del ejecutable (target/release)
    fn audio_dir() -> Option<std::path::PathBuf> {
        let mut starts = vec![std::env::current_dir().ok()?];
        if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
            starts.push(exe);
        }
        starts.into_iter().flat_map(|s| s.ancestors().map(|a| a.join("assets").join("audio")).collect::<Vec<_>>()).find(|d| d.is_dir())
    }

    // Cada sonido es un "dispositivo" MCI con su alias; Windows los mezcla y pueden sonar a la vez
    struct Sound {
        rain_volume: i32,
        next_bird: f64,
        last_flash: i64,
        thunder_at: Option<(f64, i32)>, // (tiempo de animacion, volumen) del proximo trueno
        bird: usize,
        rng: u64,
    }

    impl Sound {
        const BIRDS: usize = 3;

        fn new() -> Sound {
            let sound = Sound { rain_volume: -1, next_bird: 2.0, last_flash: i64::MIN, thunder_at: None, bird: 0, rng: 0x5EED };
            // Musica: el primer .mp3 o .wav que haya en assets/audio, en bucle todo el juego
            let music = audio_dir().and_then(|d| std::fs::read_dir(d).ok()).and_then(|entries| {
                let mut files: Vec<_> = entries.flatten().map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp3") || x.eq_ignore_ascii_case("wav"))).collect();
                files.sort();
                files.into_iter().next()
            });
            if let Some(path) = music {
                if mci(&format!("open \"{}\" type mpegvideo alias musica", path.display())) {
                    mci("setaudio musica volume to 700");
                    mci("play musica repeat");
                }
            }
            // Efectos sintetizados: se escriben una vez en la carpeta temporal y se abren
            let dir = std::env::temp_dir().join("arrietty_rt_audio");
            let mut effects = vec![("lluvia", audio::rain(8.0, 7)), ("trueno", audio::thunder(11))];
            effects.extend((0..Self::BIRDS).map(|i| (["pajaro0", "pajaro1", "pajaro2"][i], audio::birds(21 + i as u64))));
            if std::fs::create_dir_all(&dir).is_ok() {
                for (name, samples) in effects {
                    let path = dir.join(format!("{name}.wav"));
                    if std::fs::write(&path, audio::wav(&samples)).is_ok() {
                        mci(&format!("open \"{}\" type mpegvideo alias {name}", path.display()));
                    }
                }
            }
            mci("setaudio lluvia volume to 0");
            mci("play lluvia repeat");
            sound
        }

        fn random(&mut self) -> f64 {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            (self.rng >> 11) as f64 / (1u64 << 53) as f64
        }

        // Se llama cada cuadro con el tiempo de animacion y el ambiente (0 atardecer, 1 noche lluviosa)
        fn update(&mut self, t: f64, station: f64, paused: bool) {
            let rain = arrietty_rt::sky::smoothstep(0.4, 1.0, station);
            // La lluvia sube y baja con la transicion de ambiente; queda por debajo de la musica (700)
            let volume = (rain * 350.0) as i32;
            if (volume - self.rain_volume).abs() >= 20 || (volume == 0) != (self.rain_volume == 0) {
                mci(&format!("setaudio lluvia volume to {volume}"));
                self.rain_volume = volume;
            }
            if paused {
                return;
            }
            // Trueno: un poco despues de cada relampago (cada ~11 s, igual que en scene::build_scene)
            let flash = (t / 11.0).floor() as i64;
            if flash != self.last_flash {
                if self.last_flash != i64::MIN && rain > 0.3 {
                    self.thunder_at = Some((t + 0.5 + self.random() * 0.6, (rain * 1000.0) as i32));
                }
                self.last_flash = flash;
            }
            if let Some((when, vol)) = self.thunder_at {
                if t >= when {
                    mci(&format!("setaudio trueno volume to {vol}"));
                    mci("play trueno from 0");
                    self.thunder_at = None;
                }
            }
            // Pajaritos al atardecer, cada tantos segundos
            if t >= self.next_bird {
                if station < 0.5 {
                    let name = format!("pajaro{}", self.bird);
                    mci(&format!("setaudio {name} volume to {}", (350.0 + self.random() * 400.0) as i32));
                    mci(&format!("play {name} from 0"));
                    self.bird = (self.bird + 1) % Self::BIRDS;
                }
                self.next_bird = t + 2.5 + self.random() * 5.0;
            }
        }
    }

    impl Drop for Sound {
        fn drop(&mut self) {
            mci("close all");
        }
    }

    // ---------- Estado de entrada que escribe el wndproc y lee el bucle ----------

    #[derive(Clone, Copy)]
    struct Input {
        left: bool,
        right: bool,
        up: bool,
        down: bool,
        zoom_in: bool,
        zoom_out: bool,
        dragging: bool,
        last_x: i32,
        last_y: i32,
        drag_dx: f64, // pixeles arrastrados desde el ultimo cuadro
        drag_dy: f64,
        wheel: i32, // muescas de la rueda acumuladas
        pressed: [bool; 256], // teclas recien presionadas (sin auto-repeticion) desde el ultimo cuadro
    }

    impl Default for Input {
        fn default() -> Self {
            Input { left: false, right: false, up: false, down: false, zoom_in: false, zoom_out: false, dragging: false, last_x: 0, last_y: 0, drag_dx: 0.0, drag_dy: 0.0, wheel: 0, pressed: [false; 256] }
        }
    }

    thread_local! {
        static INPUT: Cell<Input> = Cell::new(Input::default());
    }

    fn edit_input(f: impl FnOnce(&mut Input)) {
        INPUT.with(|c| {
            let mut i = c.get();
            f(&mut i);
            c.set(i);
        });
    }

    fn set_key(i: &mut Input, vk: usize, down: bool) {
        match vk {
            VK_LEFT | VK_A => i.left = down,
            VK_RIGHT | VK_D => i.right = down,
            VK_UP => i.up = down,
            VK_DOWN => i.down = down,
            VK_W => i.zoom_in = down,
            VK_S => i.zoom_out = down,
            _ => {}
        }
    }

    // Coordenadas del mouse empaquetadas en lParam (con signo)
    fn mouse_xy(l: isize) -> (i32, i32) {
        ((l & 0xFFFF) as i16 as i32, ((l >> 16) & 0xFFFF) as i16 as i32)
    }

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: usize, l: isize) -> isize {
        match msg {
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                let repeat = (l >> 30) & 1 != 0;
                if w == VK_ESCAPE {
                    PostQuitMessage(0);
                }
                edit_input(|i| {
                    if !repeat && w < 256 {
                        i.pressed[w] = true;
                    }
                    set_key(i, w, true);
                });
                0
            }
            WM_KEYUP | WM_SYSKEYUP => {
                edit_input(|i| set_key(i, w, false));
                0
            }
            WM_KILLFOCUS => {
                // Al perder el foco se sueltan todas las teclas para que nada quede "pegado"
                edit_input(|i| {
                    let (wheel, pressed) = (i.wheel, i.pressed);
                    *i = Input { wheel, pressed, ..Input::default() };
                });
                0
            }
            WM_LBUTTONDOWN => {
                SetCapture(hwnd);
                let (x, y) = mouse_xy(l);
                edit_input(|i| {
                    i.dragging = true;
                    i.last_x = x;
                    i.last_y = y;
                });
                0
            }
            WM_LBUTTONUP => {
                ReleaseCapture();
                edit_input(|i| i.dragging = false);
                0
            }
            WM_CAPTURECHANGED => {
                edit_input(|i| i.dragging = false);
                0
            }
            WM_MOUSEMOVE => {
                let (x, y) = mouse_xy(l);
                edit_input(|i| {
                    if i.dragging {
                        i.drag_dx += (x - i.last_x) as f64;
                        i.drag_dy += (y - i.last_y) as f64;
                    }
                    i.last_x = x;
                    i.last_y = y;
                });
                0
            }
            WM_MOUSEWHEEL => {
                let delta = ((w >> 16) & 0xFFFF) as i16 as i32;
                edit_input(|i| i.wheel += delta);
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }

    // Copia el framebuffer BGRA a la ventana (1:1 si ya tiene su tamano)
    unsafe fn present(hwnd: HWND, buf: &[u8], w: u32, h: u32, cw: i32, ch: i32) {
        let bmi = BITMAPINFO {
            header: BITMAPINFOHEADER {
                bi_size: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                bi_width: w as i32,
                bi_height: -(h as i32), // negativo = filas de arriba hacia abajo
                bi_planes: 1,
                bi_bit_count: 32,
                bi_compression: 0,
                bi_size_image: 0,
                bi_x_pels_per_meter: 0,
                bi_y_pels_per_meter: 0,
                bi_clr_used: 0,
                bi_clr_important: 0,
            },
            colors: [0],
        };
        let hdc = GetDC(hwnd);
        if w as i32 == cw && h as i32 == ch {
            // Ya viene al tamano de la ventana: copia directa, sin pasar por el escalador de GDI
            SetDIBitsToDevice(hdc, 0, 0, w, h, 0, 0, 0, h, buf.as_ptr() as *const c_void, &bmi, 0);
        } else {
            SetStretchBltMode(hdc, HALFTONE);
            SetBrushOrgEx(hdc, 0, 0, ptr::null_mut());
            StretchDIBits(hdc, 0, 0, cw, ch, 0, 0, w as i32, h as i32, buf.as_ptr() as *const c_void, &bmi, 0, SRCCOPY);
        }
        ReleaseDC(hwnd, hdc);
    }

    // ---------- Camara orbital ----------

    #[derive(Clone, Copy, PartialEq)]
    struct Orbit {
        yaw: f64,
        pitch: f64,
        dist: f64,
    }

    // Orbita inicial de cada vista predefinida
    fn home(view: &scene::View) -> Orbit {
        Orbit { yaw: view.yaw, pitch: view.pitch, dist: view.dist }
    }

    // Acerca `cur` a `goal` de forma exponencial (independiente de los fps) y lo fija al llegar
    fn approach(cur: f64, goal: f64, k: f64, eps: f64) -> f64 {
        let next = cur + (goal - cur) * k;
        if (goal - next).abs() < eps { goal } else { next }
    }

    // Tamano interno de render: multiplo de 4 (el DIB de 24 bits no necesita relleno) y misma proporcion que la ventana
    pub fn run() {
        const DEPTH: u32 = 4;
        // Millones de pixeles que se trazan por cuadro (0 = nativa); la imagen se escala y enfoca al presentarla.
        // Auto ajusta el presupuesto gradualmente; Q permite elegir presupuestos fijos.
        const QUALITY: [(f64, &str); 6] = [(0.48, "auto"), (0.16, "rapida"), (0.26, "fluida"), (0.42, "media"), (0.7, "alta"), (0.0, "nativa")];



        unsafe {
            let _timer_resolution = TimerResolution(timeBeginPeriod(1) == 0);
            SetProcessDPIAware(); // evita que Windows estire (y emborrone) la ventana con escala de pantalla
            let hinstance = GetModuleHandleW(ptr::null());
            let class_name = wide("ArriettyRT");
            let wc = WNDCLASSEXW {
                cb_size: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfn_wnd_proc: Some(wndproc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: hinstance,
                h_icon: ptr::null_mut(),
                h_cursor: LoadCursorW(ptr::null_mut(), IDC_ARROW as *const u16),
                hbr_background: ptr::null_mut(),
                lpsz_menu_name: ptr::null(),
                lpsz_class_name: class_name.as_ptr(),
                h_icon_sm: ptr::null_mut(),
            };
            if RegisterClassExW(&wc) == 0 {
                panic!("no se pudo registrar la clase de ventana");
            }
            let title = wide("El mundo secreto de Arrietty");
            let hwnd = CreateWindowExW(0, class_name.as_ptr(), title.as_ptr(), WS_OVERLAPPEDWINDOW | WS_VISIBLE, CW_USEDEFAULT, CW_USEDEFAULT, 1280, 760, ptr::null_mut(), ptr::null_mut(), hinstance, ptr::null_mut());
            if hwnd.is_null() {
                panic!("no se pudo crear la ventana");
            }
            ShowWindow(hwnd, SW_SHOWNORMAL);
            let mut sound = Sound::new();

            let mut view_idx = 0usize;
            let mut cam = home(&scene::VIEWS[view_idx]);
            let mut goal = cam;
            let mut target = scene::VIEWS[view_idx].target; // punto al que mira la camara
            let (mut yaw_vel, mut pitch_vel) = (0.0f64, 0.0f64); // inercia tras soltar el mouse
            let (mut drag_vx, mut drag_vy) = (0.0f64, 0.0f64); // velocidad del arrastre en grados/s
            let mut was_dragging = false;
            let mut auto_rotate = false;

            // Animacion y ambiente (0 = atardecer, 1 = noche lluviosa)
            let mut anim_time = 0.0f64;
            let mut paused = false;
            let mut station = 0.0f64;
            let mut station_goal = 0.0f64;
            let mut day_cycle = false;

            let mut taa = Taa::new();

            let mut frame_budget = FrameBudget::default();
            let mut checker_parity = 0u32;
            let mut taa_on = true;
            let mut frame_buf: Vec<f32> = Vec::new();
            let mut hdr: Vec<f32> = Vec::new();
            let mut image: Vec<u8> = Vec::new();
            let (mut img_w, mut img_h) = (0u32, 0u32);
            let mut idle_frames = 0u32;
            let mut quality = 0usize; // muestreo estable y objetivo automatico de 32.5 FPS
            let (mut fps_frames, mut fps_timer) = (0u32, Instant::now());
            static EXPORTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

            let mut msg = MSG { hwnd: ptr::null_mut(), message: 0, w_param: 0, l_param: 0, time: 0, pt: POINT { x: 0, y: 0 } };
            let mut last = Instant::now();

            'main: loop {
                while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                    if msg.message == WM_QUIT {
                        break 'main;
                    }
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                let now = Instant::now();
                let elapsed = (now - last).as_secs_f64().clamp(1.0e-4, 1.0);
                let dt = elapsed.min(0.1);
                last = now;

                // Se toma la entrada del cuadro y se limpian los acumulados
                let mut input = Input::default();
                edit_input(|i| {
                    input = *i;
                    i.drag_dx = 0.0;
                    i.drag_dy = 0.0;
                    i.wheel = 0;
                    i.pressed = [false; 256];
                });
                let pressed = |vk: usize| input.pressed[vk];

                // ----- Teclas de un toque -----
                if pressed(VK_SPACE) {
                    auto_rotate = !auto_rotate;
                }
                if pressed(VK_V) || pressed(VK_B) || pressed(VK_G) || pressed(VK_C) {
                    view_idx = if pressed(VK_C) { 5 } else if pressed(VK_B) { 1 } else if pressed(VK_G) { 0 } else { (view_idx + 1) % scene::VIEWS.len() };
                    goal = home(&scene::VIEWS[view_idx]);
                    yaw_vel = 0.0;
                    pitch_vel = 0.0;
                    // Se da la vuelta por el camino mas corto
                    let d = (goal.yaw - cam.yaw).rem_euclid(360.0);
                    goal.yaw = cam.yaw + if d > 180.0 { d - 360.0 } else { d };
                }
                if pressed(VK_R) {
                    goal = home(&scene::VIEWS[view_idx]);
                    yaw_vel = 0.0;
                    pitch_vel = 0.0;
                    auto_rotate = false;
                }
                if pressed(VK_N) {
                    day_cycle = false;
                    station_goal = if station_goal > 0.5 { 0.0 } else { 1.0 };
                }
                if pressed(VK_1) {
                    day_cycle = false;
                    station_goal = 0.0;
                }
                if pressed(VK_2) {
                    day_cycle = false;
                    station_goal = 1.0;
                }
                if pressed(VK_L) {
                    day_cycle = !day_cycle;
                }
                if pressed(VK_P) {
                    paused = !paused;
                }
                if pressed(VK_Q) {
                    quality = (quality + 1) % QUALITY.len();
                }
                if pressed(VK_T) {
                    taa_on = !taa_on;
                    taa.reset();

                }

                // ----- Objetivo de la camara -----
                let axis = |pos: bool, neg: bool| (pos as i32 - neg as i32) as f64;
                goal.yaw += axis(input.right, input.left) * 75.0 * dt;
                goal.pitch += axis(input.up, input.down) * 40.0 * dt;
                goal.dist *= (axis(input.zoom_out, input.zoom_in) * 1.1 * dt).exp();
                goal.dist *= 0.88f64.powf(input.wheel as f64 / 120.0);

                if input.dragging {
                    let (dyaw, dpitch) = (input.drag_dx * 0.3, input.drag_dy * 0.2);
                    goal.yaw += dyaw;
                    goal.pitch += dpitch;
                    // Velocidad del arrastre suavizada, para lanzar la camara al soltar
                    drag_vx += (dyaw / dt - drag_vx) * 0.35;
                    drag_vy += (dpitch / dt - drag_vy) * 0.35;
                    yaw_vel = 0.0;
                    pitch_vel = 0.0;
                } else if was_dragging {
                    yaw_vel = drag_vx.clamp(-400.0, 400.0);
                    pitch_vel = (drag_vy * 0.5).clamp(-150.0, 150.0);
                    drag_vx = 0.0;
                    drag_vy = 0.0;
                }
                was_dragging = input.dragging;

                // Inercia que se frena sola
                goal.yaw += yaw_vel * dt;
                goal.pitch += pitch_vel * dt;
                let friction = (-3.5 * dt).exp();
                yaw_vel *= friction;
                pitch_vel *= friction;
                if yaw_vel.abs() < 0.5 {
                    yaw_vel = 0.0;
                }
                if pitch_vel.abs() < 0.5 {
                    pitch_vel = 0.0;
                }
                if auto_rotate {
                    goal.yaw += 15.0 * dt;
                }
                let view = &scene::VIEWS[view_idx];
                let (pitch_range, dist_range) = (view.pitch_range, view.dist_range);
                goal.pitch = goal.pitch.clamp(pitch_range.0, pitch_range.1);
                goal.dist = goal.dist.clamp(dist_range.0, dist_range.1);
                if goal.pitch <= pitch_range.0 || goal.pitch >= pitch_range.1 {
                    pitch_vel = 0.0;
                }

                // ----- La camara sigue al objetivo con suavizado -----
                let traveling = (target - scene::VIEWS[view_idx].target).length() > 0.3;
                let rate = if input.dragging { 16.0 } else if traveling { 3.0 } else { 7.0 };
                let k = 1.0 - (-rate * dt).exp();
                let before = cam;
                cam.yaw = approach(cam.yaw, goal.yaw, k, 0.01);
                cam.pitch = approach(cam.pitch, goal.pitch, k, 0.01);
                cam.dist = approach(cam.dist.ln(), goal.dist.ln(), k, 2.0e-4).exp(); // zoom en escala logaritmica
                // El punto de mira se desliza a la vista elegida (mas lento al viajar lejos, se siente como un vuelo)
                let tgoal = scene::VIEWS[view_idx].target;
                let kt = 1.0 - (-3.0 * dt).exp();
                target = arrietty_rt::vec3::Vec3::new(approach(target.x, tgoal.x, kt, 1e-4), approach(target.y, tgoal.y, kt, 1e-4), approach(target.z, tgoal.z, kt, 1e-4));
                if cam.yaw.abs() > 3600.0 {
                    let wrap = (cam.yaw / 360.0).trunc() * 360.0;
                    cam.yaw -= wrap;
                    goal.yaw -= wrap;
                }

                // ----- Tiempo de animacion y ambiente -----
                if !paused {
                    anim_time += elapsed;
                }
                let station_before = station;
                if day_cycle {
                    // Cambia de ambiente cada ~20 s de animacion, con transicion suave
                    let c = 0.5 - 0.5 * (anim_time * 2.0 * std::f64::consts::PI / 40.0).cos();
                    station = arrietty_rt::sky::smoothstep(0.25, 0.75, c);
                    station_goal = if station > 0.5 { 1.0 } else { 0.0 };
                } else {
                    station = approach(station, station_goal, 1.0 - (-6.0 * elapsed).exp(), 0.002);
                }

                sound.update(anim_time, station, paused);

                let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetClientRect(hwnd, &mut client);
                let (cw, ch) = (client.right - client.left, client.bottom - client.top);
                if cw <= 0 || ch <= 0 {
                    std::thread::sleep(Duration::from_millis(30)); // minimizada
                    continue;
                }

                // Si nada cambia (pausado, camara quieta, hora fija) el TAA converge y luego se deja de trazar
                let changing = !paused || before != cam || target != scene::VIEWS[view_idx].target || station != station_before || input.pressed.iter().any(|&p| p);
                idle_frames = if changing { 0 } else { idle_frames + 1 };
                if idle_frames > 40 && img_w == cw as u32 && img_h == ch as u32 {
                    present(hwnd, &image, img_w, img_h, cw, ch);
                    std::thread::sleep(Duration::from_millis(16));
                    continue;
                }

                // En auto se mide trabajo real, excluyendo la espera del limitador.
                let budget = if quality == 0 { frame_budget.pixels() } else { QUALITY[quality].0 * 1.0e6 };
                let scale = if budget > 0.0 { (budget / (cw as f64 * ch as f64)).sqrt().min(1.0) } else { 1.0 };
                let w = ((cw as f64 * scale) as u32).min(1600).max(64);
                let w = if quality == 0 { w / 8 * 8 } else { w };
                let h = ((w as f64 * ch as f64 / cw as f64).round() as u32).max(36);

                // Al cambiar de resolucion el TAA reescala su historial (no hace falta reiniciarlo)
                if pressed(VK_Q) || (station - station_before).abs() > 0.005 {
                    taa.reset();
                }
                if (w * h * 6) as usize != frame_buf.len() {
                    frame_buf = vec![0.0; (w * h * 6) as usize];
                }
                let camera = Camera::orbit(target, cam.dist, cam.yaw, cam.pitch, scene::CAM_VFOV, w as f64 / h as f64, 0.0);
                let scene_data = scene::build_scene(anim_time, station);
                // Con TAA se traza en tablero de ajedrez: la mitad de los pixeles por cuadro, alternando,
                // y el TAA completa la otra mitad (doble de resolucion por el mismo costo). Cada pixel
                // siempre se muestrea en su centro, asi que los bordes y las hojas no bailan.
                checker_parity ^= 1;
                if taa_on {
                    renderer::render_checker_frame(&camera, &scene_data, w, h, DEPTH, checker_parity, 0xA11CE, &mut frame_buf);
                } else {
                    renderer::render_taa_frame(&camera, &scene_data, w, h, DEPTH, (0.0, 0.0), 0xA11CE, &mut frame_buf);
                }

                let params = PostParams { bloom: 0.25 + 0.4 * station as f32, sun_uv: scene::sun_screen(&camera), rays: 0.55 * (1.0 - station as f32), sharpen: 0.15, glow_divisor: 8, bgra: true, ..PostParams::default() };
                // Reescalar la vista interactiva al area cliente.
                let (ow, oh) = (cw as u32, ch as u32);
                // Buferes reutilizados entre cuadros (pedir megas nuevos en cada cuadro causa tirones en Windows)
                hdr.clear();
                let (rw,rh) = (w,h);
                if taa_on {
                    taa.blend = 0.65;
                    hdr.extend_from_slice(taa.resolve_checker(&frame_buf, w, h, &camera, checker_parity));
                } else {
                    hdr.extend(frame_buf.chunks_exact(6).flat_map(|p| [p[0], p[1], p[2]]));
                }
                // Lluvia dibujada despues del TAA (asi no deja estela), tapada por lo que esta delante
                let streaks = scene::rain_streaks(anim_time, station);
                if !streaks.is_empty() {
                    let o = camera.origin;
                    let fb = &frame_buf;
                    let depth = move |i: usize| {

                        ((fb[i * 6 + 3] as f64 - o.x).powi(2) + (fb[i * 6 + 4] as f64 - o.y).powi(2) + (fb[i * 6 + 5] as f64 - o.z).powi(2)).sqrt() as f32
                    };
                    let tint = scene_data.sky.ambient().0 * 1.4 + arrietty_rt::vec3::Vec3::new(0.25, 0.28, 0.33);
                    renderer::draw_rain(&mut hdr, rw, rh, &camera, &depth, &streaks, tint, 0.55);
                }
                post::finish_scaled_into(&hdr, rw, rh, ow, oh, &params, &mut image);
                img_w = ow;
                img_h = oh;
                present(hwnd, &image, ow, oh, cw, ch);
                if quality == 0 && idle_frames == 0 {
                    // Solo sube de resolucion con la camara en movimiento (el reenfoque no se nota)
                    let camera_moving = before != cam || target != scene::VIEWS[view_idx].target;
                    frame_budget.observe_frame(now.elapsed().as_secs_f64(), camera_moving);
                }
                if quality == 0 {
                    let remaining = Duration::from_secs_f64(1.0 / TARGET_FPS).saturating_sub(now.elapsed());
                    if !remaining.is_zero() { std::thread::sleep(remaining); }
                }

                // FPS y calidad en el titulo, cada medio segundo
                fps_frames += 1;
                let since = fps_timer.elapsed().as_secs_f64();
                if since > 0.5 {
                    let t = wide(&format!(
                        "El mundo secreto de Arrietty - {:.0} fps",
                        fps_frames as f64 / since
                    ));
                    SetWindowTextW(hwnd, t.as_ptr());
                    fps_frames = 0;
                    fps_timer = Instant::now();
                }

                if pressed(VK_F) && !EXPORTING.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    let export_camera = Camera::orbit(target, cam.dist, cam.yaw, cam.pitch, scene::CAM_VFOV, 16.0 / 9.0, 0.0);
                    let export_time = anim_time;
                    let export_station = station;
                    std::thread::spawn(move || {
                        let result = std::panic::catch_unwind(|| renderer::background_render(|| {
                            let scene_data = scene::build_scene(export_time, export_station);
                            let mut hdr = renderer::render_hdr(&export_camera, &scene_data, 3840, 2160, 8, 8, 0x5EED);
                            let streaks = scene::rain_streaks(export_time, export_station);
                            if !streaks.is_empty() {
                                let depth = renderer::depth_map(&export_camera, &scene_data, 3840, 2160);
                                let tint = scene_data.sky.ambient().0 * 1.4 + arrietty_rt::vec3::Vec3::new(0.25, 0.28, 0.33);
                                renderer::draw_rain(&mut hdr, 3840, 2160, &export_camera, &|i| depth[i], &streaks, tint, 0.55);
                            }
                            let params = PostParams { bloom: 0.25 + 0.4 * export_station as f32, sun_uv: scene::sun_screen(&export_camera), rays: 0.55 * (1.0 - export_station as f32), ..PostParams::default() };
                            let rgb = post::finish(&hdr, 3840, 2160, &params);
                            std::fs::create_dir_all("renders")?;
                            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis();
                            let path = format!("renders/captura_4k_{stamp}.png");
                            png::write_png(&path, 3840, 2160, &rgb)?;
                            println!("Captura guardada: {path}");
                            Ok::<(), std::io::Error>(())
                        }));
                        if !matches!(result, Ok(Ok(()))) { eprintln!("No se pudo guardar la captura 4K"); }
                        EXPORTING.store(false, std::sync::atomic::Ordering::SeqCst);
                    });
                }


            }
        }
    }
}




