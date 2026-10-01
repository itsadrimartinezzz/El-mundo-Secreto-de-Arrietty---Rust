# La casa de Arrietty

**Proyecto 2 · Gráficas por Computadora · Diorama con raytracing en Rust**

El jardín de la casa de *El mundo secreto de Arrietty* (Studio Ghibli), trazado con rayos en
tiempo real y con estilo de fondo de anime.

## Demo en video

link

## Galería

Renders en 4K (3840×2160, 64 muestras por píxel), cada vista en sus dos ambientes.

| Vista | Atardecer | Noche lluviosa |
|---|---|---|
| **Jardín** | ![Jardín al atardecer](renders/jardin_atardecer.png) | ![Jardín con lluvia](renders/jardin_lluvia.png) |
| **Casita de Arrietty** | ![Casita al atardecer](renders/casita_arrietty_atardecer.png) | ![Casita con lluvia](renders/casita_arrietty_lluvia.png) |
| **Lago** | ![Lago al atardecer](renders/lago_atardecer.png) | ![Lago con lluvia](renders/lago_lluvia.png) |
| **Shō leyendo junto al kiosko** | ![Kiosko al atardecer](renders/lectura_kiosko_atardecer.png) | ![Kiosko con lluvia](renders/lectura_kiosko_lluvia.png) |

## Recorrido por el jardín

**La casa.** Gigante, como la ven Arrietty y su familia: unos 18 m de ancho y 17 m hasta la cumbrera,
de madera amarilla estilo occidental antiguo. Techo de pizarra, chimenea de ladrillo, un vitral de
rombos de colores, ventana salediza, marcos rojizos, puerta con tejadillo y farol, e hiedra trepando
por las paredes.

**El rincón de los *borrowers*.** En el cimiento de la casa hay una puertita diminuta con marco de
piedritas, un farolito que se enciende de noche y una escalera hecha de fósforos. Al lado está
Arrietty a escala real (10 cm), con su vestido rojo, la pinza de ropa en el pelo y el alfiler de
costura como espada, junto al cubo de azúcar que le deja Shō. Desde la vista de la casita, la cámara
baja a su altura: el pasto, las margaritas y los tréboles se vuelven gigantes, y una gota de rocío
refracta como una lupa.

**El lago.** Hundido en el terreno, con agua turquesa que deja ver el fondo, las rocas sumergidas y
diez peces koi nadando en círculos. Encima flotan nenúfares con flores de loto, una rana y cinco
libélulas de alas transparentes; en la orilla, rocas grandes cubiertas de musgo.

**El kiosko.** Un kiosco hexagonal junto al agua, con faroles de piedra y una banca. A su lado, Shō
lee recostado en el césped con un gato dormido, mientras otra Arrietty lo observa escondida entre
el pasto.

**El bosque.** Alcanforeros enormes, un pino japonés y un bosque que rodea todo; miles de briznas de
pasto, 800 flores silvestres (amapolas, rudbeckias, cardos) y hortensias azules, moradas y rosas.

**Los habitantes.** Niya, el gato atigrado, sentado en una laja: mueve la cola, las orejas y la cabeza,
parpadea y de noche le brillan los ojos. El cuervo de la película vigila desde la cumbrera junto a la
chimenea. Golondrinas que vuelan en círculos, gorriones que picotean y dan saltitos en el camino,
mariposas entre las flores y hojas que caen de los árboles.

## Atardecer y noche lluviosa

Todo el ambiente cuelga de un solo valor que va de 0 a 1, así que el cambio de uno a otro es continuo.

- **Atardecer:** sol bajo y cálido, sombras frescas azul verdosas como en los fondos de Ghibli,
  rebote verde del pasto y rayos de luz que salen del sol entre las hojas.
- **Noche lluviosa:** cielo nublado y luz fría de luna; hasta 1400 gotas de lluvia que se esconden
  detrás de lo que está delante; piedras, tejas y rocas que se oscurecen al mojarse y reflejan el
  cielo; anillos de gotas en el lago; ventanas y faroles encendidos; y un relámpago doble cada ~11 s.
  Bajo los techos (el porche y el kiosko) no se moja nada.

## Sonido

- La **música de Arrietty** suena en bucle todo el tiempo (`assets/audio/`), con la API de multimedia
  de Windows (MCI).
- Los **efectos se sintetizan por código** en `src/audio.rs`, sin archivos de sonido:
  - **lluvia** que sube y baja con el cambio de ambiente, siempre por debajo de la música;
  - **truenos** medio segundo después de cada relámpago, solo cuando llueve;
  - **pajaritos** con trinos, silbidos y gorjeos con un poco de eco, solo de día.


## Catálogo de materiales

Cada material tiene **su propia textura**, generada por código (ruido de Perlin, Voronoi y patrones),
y **sus propios valores** de difuso (albedo), especular, reflectividad y transparencia. "Mojada" es la
reflectividad que gana una superficie que mira hacia arriba cuando llueve.

| Material | Textura | Difuso | Especular (brillo) | Reflectividad | Transparencia (IOR) |
|---|---|:-:|:-:|:-:|:-:|
| **Agua del lago** | Ondas que se mueven y anillos de lluvia | 0.5 | 0.9 (220) | Fresnel | **0.72 (1.33)** |
| **Gota de rocío** | Color agua | 0.1 | 1.0 (400) | 0.05 | **0.9 (1.33)** |
| **Cubo de azúcar** | Granos de azúcar | 1.0 | 0.6 (64) | — | **0.25 (1.55)** |
| Vitral | Rombos de colores con plomo | 0.6 | 1.0 (300) | 0.20 | — |
| Vidrio de las ventanas | Color con brillo cálido de noche | 0.4 | 1.0 (300) | 0.35 | — |
| Pizarra del techo | Tejas en hileras con manchas | 1.0 | 0.35 (48) | 0.04 · mojada 0.8 | — |
| Piedra (lajas y cimientos) | Voronoi con mortero | 1.0 | 0.10 (16) | mojada 0.7 | — |
| Roca con musgo | Gris con musgo arriba | 1.0 | 0.15 (24) | mojada 0.6 | — |
| Hiedra | Hoja con nervio | 1.0 | 0.45 (60) | mojada 0.4 | — |
| Tablas de la casa | Tablas horizontales amarillas | 1.0 | 0.10 (16) | — | — |
| Ladrillo de la chimenea | Voronoi rojizo | 1.0 | 0.10 (16) | — | — |
| Madera de la puerta | Tablones verticales | 1.0 | 0.30 (32) | — | — |
| Follaje de los árboles | Hojas en tres tonos | 1.0 | 0.08 (12) | — | — |
| Corteza | Vetas verticales | 1.0 | 0.05 (8) | — | — |
| Pasto | De base oscura a punta clara | 1.0 | 0.25 (24) | — | — |
| Tierra | Pasto y tierra mezclados | 1.0 | 0.04 (8) | — | — |
| Pelaje de Niya | Atigrado con panza clara | 1.0 | 0.12 (12) | — | — |

## Acerda de

**Raytracing de Whitted.** Un rayo primario por píxel; en cada impacto, luz local con sombras más un
rayo reflejado y uno refractado según el material (ley de Snell, Fresnel de Schlick y reflexión
interna total). Hasta 4 rebotes en vivo y 10 en las imágenes fijas; las ramas que aportan menos del
5% al píxel se cortan.

**Geometría.** Cubos (con rotación y bordes biselados), cilindros con tapas, elipsoides y triángulos.
Las hojas de los árboles son hojas de verdad: seis triángulos con punta y nervio plegado. Los
vectores, las matrices de rotación y toda la aritmética están en `src/vec3.rs`.

**Iluminación con aire de anime.** Una rampa que acorta la transición de luz a sombra sin perder
volumen, luz ambiente hemisférica (cielo arriba, pasto abajo) y niebla por distancia. El sol o la
luna son luces direccionales; faroles y ventanas, puntuales con un alcance que se apaga suave. Lo que
no proyecta sombra (pasto, flores, agua, lluvia) va en una capa aparte que los rayos de sombra no
recorren.

**Aceleración.** Un BVH construido con la heurística de área (SAH), aplanado en un arreglo y recorrido
con una pila. Sus cajas se prueban de a cuatro al mismo tiempo con instrucciones SSE2 del procesador.
La geometría fija se construye una sola vez al arrancar; por cuadro solo se arma lo animado.

**Imagen en vivo.** Cada cuadro traza la mitad de los píxeles en patrón de tablero de ajedrez y el
antialiasing temporal completa la otra mitad reproyectando el cuadro anterior: el doble de resolución
por los mismos rayos. El historial se lee con Catmull-Rom para que nada se difumine al mover la cámara.
Una resolución automática mantiene unos 28 cuadros por segundo y solo sube mientras la cámara se mueve.

**Post-procesado** (en HDR, en la CPU): bloom en dos escalas, rayos de luz hacia el sol, saturación,
tonemapping ACES, gamma, viñeta, enfoque y reescalado a la ventana.

## Optimización

Medido en un Intel Core i9-13900HX (32 hilos), con la ventana a 1264×721:

| | Antes | Después |
|---|---|---|
| Resolución trazada en vivo | 675×385 | 1064×607 (jardín) · 952×543 (lago) |
| Cuadros por segundo | 20–25 | ~28 (mediana) |
| Post-procesado por cuadro | ~8.5 ms | ~4.3 ms |

1. **Reutilizar la memoria entre cuadros.** En Windows, un búfer grande recién pedido llega vacío y
   cada página falla la primera vez que se toca: uno nuevo de 6 MB costaba 2 ms, reutilizado 0.2 ms.
2. **Objetos más compactos:** de 160 a 96 bytes, compartiendo las coordenadas de textura de las hojas
   y separando las rotaciones, que casi ningún objeto usa.
3. **Tablero de ajedrez:** el doble de píxeles por casi el mismo costo.
4. **Pool de hilos propio:** los hilos se crean una sola vez y se reparten las filas de la imagen.

## Instalación y uso

Requisito: Rust. La ventana en vivo funciona en Windows.

```bash
cargo run --release --bin window
```

| Entrada | Qué hace |
|---|---|
| Arrastrar con clic izquierdo | Orbitar (con inercia al soltar) |
| Rueda / `W` `S` | Acercar y alejar |
| Flechas / `A` `D` | Girar e inclinar |
| `V` | Siguiente vista (jardín, casita, lago, gatito, flores, kiosko, Arrietty y Shō) |
| `B` · `G` · `C` | Casita de Arrietty · jardín · Shō leyendo |
| `1` · `2` · `N` | Atardecer · noche lluviosa · alternar |
| `L` | Alternar ambientes solo, cada ~20 s |
| `Espacio` | Giro automático |
| `P` | Pausa |
| `T` | Antialiasing temporal |
| `Q` | Calidad: automática, rápida, fluida, media, alta, nativa |
| `F` | Captura 4K en `renders/` (en segundo plano) |
| `R` | Volver al encuadre de la vista |
| `Esc` | Salir |

**Imagen fija y video:**

```bash
# ancho alto muestras profundidad ambiente(0 atardecer, 1 lluvia) segundo vista(0..6) [salida.png]
cargo run --release -- 3840 2160 64 10 1 1.3 2 renders/lago_lluvia.png

# cuadros de una vuelta de 360° pasando del atardecer a la lluvia (en frames/), y armar el video
cargo run --release --bin anim -- 360 1280 720 12 6 30
ffmpeg -y -framerate 30 -i frames/frame_%04d.png -c:v libx264 -pix_fmt yuv420p -crf 18 arrietty.mp4
```

**Pruebas:**

```bash
cargo test --release
cargo run --release --example verify_live -- 680 0   # 16 cuadros idénticos con la escena quieta
powershell -File benchmark_parallel.ps1               # velocidad con 1 a 32 hilos
```

## Archivos del proyecto

```
src/
  vec3.rs · rng.rs · perlin.rs   Matemática: vectores, matrices, azar y ruido
  texture.rs · material.rs       Texturas procedurales y materiales
  ray.rs · camera.rs · light.rs  Rayo, cámara orbital y luces
  hittable.rs · bvh.rs           Primitivas y BVH con SSE2
  sky.rs                         Skybox de los dos ambientes
  render.rs · renderer.rs        Trazado y reparto en hilos
  taa.rs · frame_budget.rs       Antialiasing temporal y resolución automática
  post.rs · png.rs               Post-procesado y codificador PNG
  audio.rs                       Lluvia, truenos y pajaritos sintetizados
  scene.rs                       Toda la escena
  main.rs                        Imagen fija
  bin/anim.rs · bin/window.rs    Cuadros de video · ventana en vivo
assets/audio/                    Música
renders/                         Galería en 4K y mediciones
```


