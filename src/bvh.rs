use crate::hittable::{Aabb, HitRecord, Object};
use crate::material::Material;
use crate::ray::Ray;
use crate::vec3::Vec3;

const LEAF_MAX: usize = 2; // objetos por hoja
const BINS: usize = 12; // cubetas para estimar el costo SAH

// Nodo aplanado de 32 bytes: si `count > 0` es hoja con objetos objects[first..first+count];
// si no, es interno y sus dos hijos son el par `first` (los hermanos van juntos).
// Las cajas se guardan en f32 para que caben mas nodos en cache
#[derive(Clone, Copy)]
struct Node {
    bmin: [f32; 3],
    bmax: [f32; 3],
    first: u32,
    count: u32,
}

// Dos hermanos en una sola linea de cache de 64 bytes: probar ambos hijos cuesta una lectura de memoria
#[derive(Clone, Copy)]
#[repr(C, align(64))]
struct Pair([Node; 2]);

// Nodo durante la construccion (hijos en cualquier lugar del arreglo)
struct BuildNode {
    node: Node,
    second: u32,
}

// Rayo en f32 para probar cajas rapido
struct Ray32 {
    o: [f32; 3],
    inv: [f32; 3],
}

impl Ray32 {
    fn new(r: &Ray) -> Self {
        Ray32 { o: [r.origin.x as f32, r.origin.y as f32, r.origin.z as f32], inv: [r.inv_dir.x as f32, r.inv_dir.y as f32, r.inv_dir.z as f32] }
    }
}

impl Node {
    // Redondeo hacia afuera para que la caja en f32 nunca quede mas chica que la original
    fn new(b: &Aabb, first: u32, count: u32) -> Self {
        let lo = |x: f64| (x - (x.abs() * 1.0e-6 + 1.0e-5)) as f32;
        let hi = |x: f64| (x + (x.abs() * 1.0e-6 + 1.0e-5)) as f32;
        Node { bmin: [lo(b.min.x), lo(b.min.y), lo(b.min.z)], bmax: [hi(b.max.x), hi(b.max.y), hi(b.max.z)], first, count }
    }

    // Prueba por slabs en f32; devuelve la distancia de entrada
    #[cfg(test)]
    #[inline]
    fn entry(&self, r: &Ray32, t_min: f64, t_max: f64) -> Option<f64> {
        let (mut lo, mut hi) = (t_min as f32, t_max.min(f32::MAX as f64) as f32);
        for a in 0..3 {
            let mut t0 = (self.bmin[a] - r.o[a]) * r.inv[a];
            let mut t1 = (self.bmax[a] - r.o[a]) * r.inv[a];
            if r.inv[a] < 0.0 {
                std::mem::swap(&mut t0, &mut t1);
            }
            lo = t0.max(lo);
            hi = t1.min(hi);
            if hi < lo {
                return None;
            }
        }
        Some(lo as f64)
    }
}

// Jerarquia de cajas guardada en un arreglo y recorrida con una pila (sin recursion ni punteros).
// Los objetos se reordenan para que cada hoja apunte a un tramo contiguo: objects[first..first+count]
// La raiz es pairs[0].0[0]; un nodo se nombra con 2 * par + lado
pub struct BvhNode {
    pairs: Vec<Pair>,
    wide: Vec<WideNode>,
}

// Cuatro cajas en paralelo con SSE2 (parte de la CPU x86-64, sin librerias).
#[derive(Clone)]
#[repr(C, align(64))]
struct WideNode {
    min: [[f32;4];3],
    max: [[f32;4];3],
    first: [u32;4],
    count: [u32;4],
}
impl WideNode {
    fn empty() -> Self { Self {min:[[f32::INFINITY;4];3],max:[[f32::NEG_INFINITY;4];3],first:[0;4],count:[u32::MAX;4]} }
    #[inline]
    fn entries(&self, ray:&Ray32, low:f32, high:f32)->[f32;4] {
        #[cfg(target_arch="x86_64")]
        unsafe {
            use std::arch::x86_64::*;
            let mut near=_mm_set1_ps(low);
            let mut far=_mm_set1_ps(high);
            for a in 0..3 {
                let origin=_mm_set1_ps(ray.o[a]);
                let inv=_mm_set1_ps(ray.inv[a]);
                let t0=_mm_mul_ps(_mm_sub_ps(_mm_loadu_ps(self.min[a].as_ptr()),origin),inv);
                let t1=_mm_mul_ps(_mm_sub_ps(_mm_loadu_ps(self.max[a].as_ptr()),origin),inv);
                // Ordenar por signo conserva el tratamiento de NaN de slab (rayos paralelos).
                let (lo,hi)=if ray.inv[a]<0.0 {(t1,t0)}else{(t0,t1)};
                near=_mm_max_ps(lo,near);far=_mm_min_ps(hi,far);
            }
            let valid=_mm_cmpge_ps(far,near);
            let result=_mm_or_ps(_mm_and_ps(valid,near),_mm_andnot_ps(valid,_mm_set1_ps(f32::INFINITY)));
            let mut out=[0.0;4];_mm_storeu_ps(out.as_mut_ptr(),result);
            out
        }
        #[cfg(not(target_arch="x86_64"))]
        {
            std::array::from_fn(|i| {
                let (mut near,mut far)=(low,high);
                for a in 0..3 {
                    let (mut t0,mut t1)=((self.min[a][i]-ray.o[a])*ray.inv[a],(self.max[a][i]-ray.o[a])*ray.inv[a]);
                    if ray.inv[a]<0.0 {std::mem::swap(&mut t0,&mut t1);}
                    near=t0.max(near);far=t1.min(far);
                }
                if far>=near {near}else{f32::INFINITY}
            })
        }
    }
}

fn area(b: &Aabb) -> f64 {
    let d = b.max - b.min;
    if d.x < 0.0 {
        return 0.0;
    }
    2.0 * (d.x * d.y + d.y * d.z + d.z * d.x)
}

fn empty_box() -> Aabb {
    Aabb { min: Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY), max: Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY) }
}

#[cfg(test)]
mod wide_tests {
    use super::*;
    use crate::{hittable::{Cuboid,Sphere,Triangle},texture::Texture,rng::Rng};

    #[test]
    fn wide_traversal_matches_brute_force_and_binary_for_hits_and_shadows() {
        let mats=[Material::new(Texture::Solid(Vec3::ONE)),Material::new(Texture::Solid(Vec3::ONE)).transparency(0.8,1.33)];
        let mut rng=Rng::new(1987);
        let mut objects=Vec::new();
        for i in 0..96 {
            let c=rng.vec3_range(-5.0,5.0);
            let extent=Vec3::new(0.15,0.23,0.31);
            objects.push(match i%3 {
                0=>Object::Cuboid(Cuboid::new(c-extent,c+extent,(i%2) as u32)),
                1=>Object::Sphere(Sphere::ellipsoid(c,extent,(i%2) as u32)),
                _=>Object::Triangle(Triangle::new([c,c+Vec3::new(0.5,0.0,0.0),c+Vec3::new(0.0,0.7,0.0)],[Vec3::ZERO;3],(i%2) as u32)),
            });
        }
        let tree=BvhNode::build(&mut objects);
        for i in 0..5000 {
            let origin=rng.vec3_range(-6.0,6.0);
            let direction=if i%4==0 {Vec3::new(0.0,0.0,1.0)}else{rng.vec3_range(-1.0,1.0)};
            let ray=Ray::new(origin,direction);
            let limit=if i%2==0 {3.0}else{100.0};
            let brute=objects.iter().filter_map(|o|o.hit(&ray,0.001,limit,&mats)).min_by(|a,b|a.t.total_cmp(&b.t));
            let fast=tree.hit(&ray,0.001,limit,&objects,&mats);
            let binary=tree.binary_hit(&ray,0.001,limit,&objects,&mats);
            assert_eq!(fast.is_some(),brute.is_some());
            assert_eq!(fast.as_ref().map(|h|h.t),binary.as_ref().map(|h|h.t));
            if let (Some(a),Some(b))=(fast,brute) {assert!((a.t-b.t).abs()<1e-9);}
            let shadow=objects.iter().filter_map(|o|o.hit(&ray,0.001,limit,&mats)).any(|h|h.material.transparency<0.5);
            assert_eq!(tree.any_opaque_hit(&ray,0.001,limit,&objects,&mats),shadow);
            assert_eq!(tree.binary_any_opaque_hit(&ray,0.001,limit,&objects,&mats),shadow);
        }
        let empty=BvhNode::build(&mut Vec::new());
        assert!(empty.hit(&Ray::new(Vec3::ZERO,Vec3::ONE),0.0,10.0,&[],&mats).is_none());
    }
}

impl BvhNode {
    pub fn build(objects: &mut Vec<Object>) -> BvhNode {
        // Cajas y centroides se calculan una sola vez (la escena se rearma cada cuadro)
        let boxes: Vec<Aabb> = objects.iter().map(|o| o.bounding_box()).collect();
        let cents: Vec<Vec3> = boxes.iter().map(|b| b.centroid()).collect();
        let mut items: Vec<u32> = (0..objects.len() as u32).collect();
        let mut nodes = Vec::with_capacity(objects.len() * 2);
        let mut pairs = Vec::new();
        if !items.is_empty() {
            Self::build_node(&mut nodes, &mut items, 0, objects.len(), &boxes, &cents);
            // Se reacomoda el mismo arbol en pares de hermanos, en profundidad (subarboles cercanos en memoria)
            pairs.reserve(nodes.len() / 2 + 1);
            pairs.push(Pair([nodes[0].node; 2]));
            pairs[0].0[0] = Self::place(&nodes, 0, &mut pairs);
        }
        // Mismo orden que las hojas: el recorrido lee memoria contigua en vez de saltar por todo el arreglo
        let mut slots: Vec<Option<Object>> = objects.drain(..).map(Some).collect();
        objects.extend(items.iter().map(|&i| slots[i as usize].take().unwrap()));
        let mut tree=BvhNode { pairs, wide:Vec::new() };
        if !tree.pairs.is_empty() { tree.make_wide(*tree.node(0)); }
        #[cfg(not(test))]
        { tree.pairs = Vec::new(); }
        tree
    }

    fn make_wide(&mut self, root:Node)->u32 {
        let index=self.wide.len() as u32;
        self.wide.push(WideNode::empty());
        let mut children=vec![root];
        while children.len()<4 {
            let Some(i)=children.iter().enumerate().filter(|(_,n)| n.count==0)
                .max_by(|(_,a),(_,b)| {
                    let area=|n:&Node| {let d=std::array::from_fn::<_,3,_>(|i| n.bmax[i]-n.bmin[i]); d[0]*d[1]+d[0]*d[2]+d[1]*d[2]};
                    area(a).total_cmp(&area(b))
                }).map(|(i,_)|i) else {break;};
            let node=children.remove(i);
            let [left,right]=self.pairs[node.first as usize].0;
            children.insert(i,right);children.insert(i,left);
        }
        let mut wide=WideNode::empty();
        for (i,n) in children.into_iter().enumerate() {
            for a in 0..3 {wide.min[a][i]=n.bmin[a];wide.max[a][i]=n.bmax[a];}
            wide.count[i]=n.count;
            wide.first[i]=if n.count==0 {self.make_wide(n)}else{n.first};
        }
        self.wide[index as usize]=wide;
        index
    }

    // Copia el nodo `idx` del arbol de construccion; si es interno, reserva el par de sus hijos
    fn place(nodes: &[BuildNode], idx: u32, pairs: &mut Vec<Pair>) -> Node {
        let b = &nodes[idx as usize];
        let mut node = b.node;
        if node.count == 0 {
            let p = pairs.len();
            pairs.push(Pair([node; 2]));
            let left = Self::place(nodes, node.first, pairs);
            let right = Self::place(nodes, b.second, pairs);
            pairs[p] = Pair([left, right]);
            node.first = p as u32;
        }
        node
    }

    #[inline]
    fn node(&self, r: u32) -> &Node {
        &self.pairs[(r >> 1) as usize].0[(r & 1) as usize]
    }

    // Construye el nodo para items[start..end] y devuelve su indice
    fn build_node(nodes: &mut Vec<BuildNode>, items: &mut [u32], start: usize, end: usize, boxes: &[Aabb], cents: &[Vec3]) -> u32 {
        let mut bbox = empty_box();
        let mut cb = empty_box();
        for &i in &items[start..end] {
            bbox = Aabb::surrounding(&bbox, &boxes[i as usize]);
            let c = cents[i as usize];
            cb = Aabb::surrounding(&cb, &Aabb { min: c, max: c });
        }
        let idx = nodes.len() as u32;
        nodes.push(BuildNode { node: Node::new(&bbox, start as u32, (end - start) as u32), second: 0 });
        let n = end - start;
        if n <= LEAF_MAX {
            return idx;
        }

        // SAH con cubetas: prueba cortes en los tres ejes y se queda con el mas barato
        let ext = cb.max - cb.min;
        let mut best = (f64::INFINITY, 0usize, 0usize); // (costo, eje, cubeta)
        for axis in 0..3 {
            if ext[axis] < 1.0e-9 {
                continue;
            }
            let mut bin_box = [empty_box(); BINS];
            let mut bin_cnt = [0usize; BINS];
            let k = BINS as f64 / ext[axis];
            for &i in &items[start..end] {
                let b = (((cents[i as usize][axis] - cb.min[axis]) * k) as usize).min(BINS - 1);
                bin_cnt[b] += 1;
                bin_box[b] = Aabb::surrounding(&bin_box[b], &boxes[i as usize]);
            }
            // Areas acumuladas desde la izquierda y desde la derecha
            let mut left_area = [0.0; BINS];
            let mut left_cnt = [0usize; BINS];
            let (mut acc, mut cnt) = (empty_box(), 0);
            for b in 0..BINS {
                acc = Aabb::surrounding(&acc, &bin_box[b]);
                cnt += bin_cnt[b];
                left_area[b] = area(&acc);
                left_cnt[b] = cnt;
            }
            let (mut acc, mut cnt) = (empty_box(), 0);
            for b in (1..BINS).rev() {
                acc = Aabb::surrounding(&acc, &bin_box[b]);
                cnt += bin_cnt[b];
                let cost = left_area[b - 1] * left_cnt[b - 1] as f64 + area(&acc) * cnt as f64;
                if left_cnt[b - 1] > 0 && cnt > 0 && cost < best.0 {
                    best = (cost, axis, b);
                }
            }
        }

        let mid = if best.0.is_finite() {
            let (axis, split) = (best.1, best.2);
            let k = BINS as f64 / ext[axis];
            let slice = &mut items[start..end];
            let mut l = 0;
            for r in 0..slice.len() {
                let b = (((cents[slice[r] as usize][axis] - cb.min[axis]) * k) as usize).min(BINS - 1);
                if b < split {
                    slice.swap(l, r);
                    l += 1;
                }
            }
            start + l
        } else {
            start + n / 2 // todos los centroides coinciden: corte por la mitad
        };

        let left = Self::build_node(nodes, items, start, mid, boxes, cents);
        let right = Self::build_node(nodes, items, mid, end, boxes, cents);
        let b = &mut nodes[idx as usize];
        b.node.first = left;
        b.node.count = 0;
        b.second = right;
        idx
    }

    // Impacto mas cercano; visita primero el hijo del lado de donde viene el rayo
    pub fn hit<'a>(&self, ray: &Ray, t_min: f64, t_max: f64, objects: &[Object], mats: &'a [Material]) -> Option<HitRecord<'a>> {
        if self.wide.is_empty() {return None;}
        let r=Ray32::new(ray);
        let mut closest=t_max;
        let mut result=None;
        let mut stack=[(0u32,0u32,0f32);256];
        stack[0]=(0,0,t_min as f32);
        let mut sp=1;
        while sp>0 {
            sp-=1;let (first,count,entry)=stack[sp];
            if entry as f64>closest {continue;}
            if count>0 {
                for obj in &objects[first as usize..(first+count) as usize] {
                    if let Some(hit)=obj.hit(ray,t_min,closest,mats) {closest=hit.t;result=Some(hit);}
                }
            } else {
                let node=&self.wide[first as usize];
                let entries=node.entries(&r,t_min as f32,closest.min(f32::MAX as f64) as f32);
                let begin=sp;
                for i in 0..4 {
                    if node.count[i]!=u32::MAX && entries[i].is_finite() {
                        let item=(node.first[i],node.count[i],entries[i]);
                        let mut j=sp;
                        while j>begin && stack[j-1].2<item.2 {stack[j]=stack[j-1];j-=1;}
                        stack[j]=item;sp+=1;
                    }
                }
            }
        }
        result
    }

    #[cfg(test)]
    fn binary_hit<'a>(&self, ray: &Ray, t_min: f64, t_max: f64, objects: &[Object], mats: &'a [Material]) -> Option<HitRecord<'a>> {
        if self.pairs.is_empty() {
            return None;
        }
        let mut closest = t_max;
        let mut result = None;
        // Pila de (nodo, distancia de entrada a su caja): cada caja se prueba una sola vez, al apilarla
        let r32 = Ray32::new(ray);
        let Some(root_t) = self.node(0).entry(&r32, t_min, closest) else { return None };
        let mut stack = [(0u32, 0.0f64); 128];
        stack[0] = (0, root_t);
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let (idx, enter) = stack[sp];
            if enter >= closest {
                continue; // ya se encontro algo mas cerca que esta caja
            }
            let node = self.node(idx);
            if node.count > 0 {
                for obj in &objects[node.first as usize..(node.first + node.count) as usize] {
                    if let Some(rec) = obj.hit(ray, t_min, closest, mats) {
                        closest = rec.t;
                        result = Some(rec);
                    }
                }
            } else {
                // Se prueban ambos hijos y el mas cercano se apila al final para visitarlo primero
                let [l, r] = &self.pairs[node.first as usize].0;
                let a = l.entry(&r32, t_min, closest).map(|t| (node.first * 2, t));
                let b = r.entry(&r32, t_min, closest).map(|t| (node.first * 2 + 1, t));
                match (a, b) {
                    (Some(x), Some(y)) => {
                        let (near, far) = if x.1 <= y.1 { (x, y) } else { (y, x) };
                        stack[sp] = far;
                        stack[sp + 1] = near;
                        sp += 2;
                    }
                    (Some(x), None) | (None, Some(x)) => {
                        stack[sp] = x;
                        sp += 1;
                    }
                    (None, None) => {}
                }
            }
        }
        result
    }

    // Para sombras: sale en cuanto encuentra cualquier objeto opaco
    pub fn any_opaque_hit(&self, ray: &Ray, t_min: f64, t_max: f64, objects: &[Object], mats: &[Material]) -> bool {
        if self.wide.is_empty() {return false;}
        let r=Ray32::new(ray);
        let mut stack=[0u32;256];let mut sp=1;
        while sp>0 {
            sp-=1;
            let node=&self.wide[stack[sp] as usize];
            let entries=node.entries(&r,t_min as f32,t_max.min(f32::MAX as f64) as f32);
            for i in 0..4 {
                if node.count[i]==u32::MAX || !entries[i].is_finite(){continue;}
                if node.count[i]==0 {stack[sp]=node.first[i];sp+=1;} else {
                    for obj in &objects[node.first[i] as usize..(node.first[i]+node.count[i]) as usize] {
                        if let Some(rec)=obj.hit(ray,t_min,t_max,mats) {if rec.material.transparency<0.5{return true;}}
                    }
                }
            }
        }
        false
    }

    #[cfg(test)]
    fn binary_any_opaque_hit(&self, ray: &Ray, t_min: f64, t_max: f64, objects: &[Object], mats: &[Material]) -> bool {
        if self.pairs.is_empty() {
            return false;
        }
        let r32 = Ray32::new(ray);
        if self.node(0).entry(&r32, t_min, t_max).is_none() {
            return false;
        }
        // Se apilan solo los hijos cuya caja toca el rayo
        let mut stack = [0u32; 128];
        let mut sp = 1;
        while sp > 0 {
            sp -= 1;
            let node = self.node(stack[sp]);
            if node.count > 0 {
                for obj in &objects[node.first as usize..(node.first + node.count) as usize] {
                    if let Some(rec) = obj.hit(ray, t_min, t_max, mats) {
                        if rec.material.transparency < 0.5 {
                            return true;
                        }
                    }
                }
            } else {
                let [l, r] = &self.pairs[node.first as usize].0;
                if l.entry(&r32, t_min, t_max).is_some() {
                    stack[sp] = node.first * 2;
                    sp += 1;
                }
                if r.entry(&r32, t_min, t_max).is_some() {
                    stack[sp] = node.first * 2 + 1;
                    sp += 1;
                }
            }
        }
        false
    }
}




