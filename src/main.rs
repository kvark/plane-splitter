extern crate cgmath;
extern crate euclid;
extern crate env_logger;
extern crate mint;
extern crate plane_split;
extern crate ron;
#[macro_use]
extern crate serde_derive;
extern crate three;

use std::io::{Read, Seek, SeekFrom};
use std::fs::File;
use std::time::SystemTime;

use cgmath::prelude::*;
use three::{GfxPrimitive, gfx_state as state, gfx_preset as preset};
use plane_split::Splitter;


const STENCIL_PASS: state::StencilSide = state::StencilSide {
    fun: state::Comparison::Always,
    mask_read: 0,
    mask_write: 0,
    op_fail: state::StencilOp::Keep,
    op_depth_fail: state::StencilOp::Keep,
    op_pass: state::StencilOp::Keep,
};
const STENCIL_SET: state::StencilSide = state::StencilSide {
    fun: state::Comparison::Equal,
    mask_read: !0,
    mask_write: !0,
    op_fail: state::StencilOp::Keep,
    op_depth_fail: state::StencilOp::Keep,
    op_pass: state::StencilOp::IncrementClamp,
};


#[derive(Deserialize)]
struct Plane {
    pos: [f32; 3],
    rot: [f32; 3],
    scale: f32,
}

fn main() {
    env_logger::init().unwrap();

    let mut win = three::Window::new("Plane splitter", "../three-rs/data/shaders").build();
    let mut cam = win.factory.perspective_camera(60.0, 1.0, 10.0);
    let mut controls = three::OrbitControls::new(&cam, [0.0, 1.0, 3.0], [0.0, 0.0, 0.0]).build();
    win.scene.add(&cam);

    let pipeline_base = win.factory.basic_pipeline("data/poly",
        GfxPrimitive::TriangleList,
        state::Rasterizer::new_fill(),
        state::MASK_ALL,
        preset::blend::REPLACE,
        preset::depth::LESS_EQUAL_WRITE,
        state::Stencil { front: STENCIL_PASS, back: STENCIL_PASS, },
        ).unwrap();
    let pipeline_a = win.factory.basic_pipeline("data/poly",
        GfxPrimitive::TriangleList,
        state::Rasterizer::new_fill(),
        state::RED | state::GREEN,
        state::Blend { color: state::BlendChannel { equation: state::Equation::Sub, source: state::Factor::One, destination: state::Factor::One }, .. preset::blend::REPLACE },
        preset::depth::PASS_TEST,
        state::Stencil { front: STENCIL_SET, back: STENCIL_SET, },
        ).unwrap();
    let pipeline_b = win.factory.basic_pipeline("data/poly",
        GfxPrimitive::TriangleList,
        state::Rasterizer::new_fill(),
        state::RED | state::GREEN,
        state::Blend { color: state::BlendChannel { equation: state::Equation::RevSub, source: state::Factor::One, destination: state::Factor::One }, .. preset::blend::REPLACE },
        preset::depth::PASS_TEST,
        state::Stencil { front: STENCIL_SET, back: STENCIL_SET, },
        ).unwrap();

    let geometry = three::Geometry::new_plane(2.0, 2.0);
    let mut last_time = SystemTime::now();
    let mut file = File::open("data/poly.ron").expect("Unable to open scene description");
    let mut splitter = plane_split::BspSplitter::<f32, ()>::new();
    let mut meshes = Vec::new();
    let mut frame_id = 0usize;

    while win.update() && !three::KEY_ESCAPE.is_hit(&win.input) {
        let write_time = file.metadata().unwrap().modified().unwrap();
        if write_time != last_time {
            last_time = write_time;
            let mut contents = String::new();
            file.seek(SeekFrom::Start(0)).unwrap();
            file.read_to_string(&mut contents).unwrap();
            let planes: Vec<Plane> = match ron::de::from_str(&contents) {
                Ok(planes) => planes,
                Err(e) => {
                    println!("Unable to parse plane set: {:?}", e);
                    continue;
                }
            };

            let rect = euclid::Rect::new(
                euclid::TypedPoint2D::new(-1.0, -1.0),
                euclid::TypedSize2D::new(2.0, 2.0));
            splitter.reset();
            meshes.clear();

            for (i, plane) in planes.iter().enumerate() {
                let gray = (i + 1) * 0xFF / planes.len();
                let mat = three::Material::CustomBasicPipeline {
                    color: (gray as u32) * 0x010101,
                    map: None,
                    pipeline: pipeline_base.clone(),
                };
                let mut m = win.factory.mesh(geometry.clone(), mat);

                let euler = cgmath::Quaternion::from(cgmath::Euler::new(
                    cgmath::Deg(plane.rot[0]), cgmath::Deg(plane.rot[1]), cgmath::Deg(plane.rot[2])));
                m.set_transform(plane.pos, euler, plane.scale);
                win.scene.add(&m);
                meshes.push(m);

                let decomposed = cgmath::Decomposed {
                    disp: cgmath::Point3::from(plane.pos).to_vec(),
                    rot: cgmath::Quaternion::from(euler),
                    scale: plane.scale,
                };
                let transform = euclid::TypedTransform3D::from_row_arrays(cgmath::Matrix4::from(decomposed).into());
                let poly = plane_split::Polygon::from_transformed_rect(rect, transform, i);
                splitter.add(poly);
            }
        }

        let mut points = Vec::new();
        let mut temp = Vec::new();
        let view_dir = {
            let node = cam.sync(&win.scene);
            let dir = cgmath::Quaternion::from(node.world_transform.orientation) *
                cgmath::Vector3::unit_z();
            euclid::TypedVector3D::new(dir.x, dir.y, dir.z)
        };

        //reverse the order to draw front to back
        let results = splitter.sort(view_dir * -1.0);
        for poly in results {
            points.clear();
            for &k in &[0,1,2,2,3,0] {
                let p = &poly.points[k];
                points.push(mint::Point3::from([p.x, p.y, p.z]))
            }
            let start = points[0].clone();
            points.push(start);
            let geom = three::Geometry::from_vertices(points.clone());
            let gray = (poly.anchor + 1) * 0xFF / meshes.len();
            let mat = three::Material::CustomBasicPipeline {
                color: (gray as u32) * 0x010101,
                map: None,
                pipeline: if frame_id & 1 == 0 {
                    pipeline_a.clone()
                } else {
                    pipeline_b.clone()
                },
            };
            let mesh = win.factory.mesh(geom, mat);
            win.scene.add(&mesh);
            temp.push(mesh);
        }

        temp.reverse(); //HACK: ensure drop order
        if three::KEY_SPACE.is_hit(&win.input) {
            println!("{:#?}", results);
        }
        frame_id += 1;

        controls.update(&win.input);
        win.render(&cam);
    }
}
