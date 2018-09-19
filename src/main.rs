extern crate cgmath;
extern crate euclid;
extern crate env_logger;
extern crate gfx;
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
use gfx::state::{
    Blend, BlendChannel, Comparison, ColorMask, Equation, Factor,
    Rasterizer, Stencil, StencilOp, StencilSide,
};
use plane_split::Splitter;
use three::Object;


const STENCIL_PASS: StencilSide = StencilSide {
    fun: Comparison::Always,
    mask_read: 0,
    mask_write: 0,
    op_fail: StencilOp::Keep,
    op_depth_fail: StencilOp::Keep,
    op_pass: StencilOp::Keep,
};
const STENCIL_SET: StencilSide = StencilSide {
    fun: Comparison::Equal,
    mask_read: !0,
    mask_write: !0,
    op_fail: StencilOp::Keep,
    op_depth_fail: StencilOp::Keep,
    op_pass: StencilOp::IncrementClamp,
};
const EXTENT: f32 = 1.0;


#[derive(Deserialize)]
struct Plane {
    pos: [f32; 3],
    rot: [f32; 3],
    scale: f32,
}

fn main() {
    env_logger::init();

    let mut win = three::Window::new("Plane Splitter");
    let cam = win.factory.perspective_camera(60.0, 1.0 .. 10.0);
    let mut controls = three::controls::Orbit::builder(&cam)
        .position([0.0, 1.0, 3.0])
        .target([0.0, 0.0, 0.0])
        .build();
    win.scene.add(&cam);

    let pipeline_base = win.factory.basic_pipeline(
        "data",
        "poly",
        gfx::Primitive::TriangleList,
        Rasterizer::new_fill(),
        ColorMask::all(),
        gfx::preset::blend::REPLACE,
        gfx::preset::depth::LESS_EQUAL_WRITE,
        Stencil { front: STENCIL_PASS, back: STENCIL_PASS, },
        ).unwrap();
    let pipeline_a = win.factory.basic_pipeline(
        "data",
        "poly",
        gfx::Primitive::TriangleList,
        Rasterizer::new_fill(),
        ColorMask::RED | ColorMask::GREEN,
        Blend {
            color: BlendChannel {
                equation: Equation::Sub,
                source: Factor::One,
                destination: Factor::One,
            },
            .. gfx::preset::blend::REPLACE
        },
        gfx::preset::depth::PASS_TEST,
        Stencil { front: STENCIL_SET, back: STENCIL_SET, },
        ).unwrap();
    let pipeline_b = win.factory.basic_pipeline(
        "data",
        "poly",
        gfx::Primitive::TriangleList,
        Rasterizer::new_fill(),
        ColorMask::RED | ColorMask::GREEN,
        Blend {
            color: BlendChannel {
                equation: Equation::RevSub,
                source: Factor::One,
                destination: Factor::One,
            },
            .. gfx::preset::blend::REPLACE
        },
        gfx::preset::depth::PASS_TEST,
        Stencil { front: STENCIL_SET, back: STENCIL_SET, },
        ).unwrap();

    let geometry = three::Geometry::plane(2.0 * EXTENT, 2.0 * EXTENT);
    let base_rect = euclid::Rect::new(
        euclid::TypedPoint2D::new(-EXTENT, -EXTENT),
        euclid::TypedSize2D::new(2.0 * EXTENT, 2.0 * EXTENT),
    );

    let mut last_time = SystemTime::now();
    let mut file = File::open("data/poly.ron").expect("Unable to open scene description");
    let mut splitter = plane_split::BspSplitter::<f32, ()>::new();
    let mut meshes = Vec::new();
    let mut frame_id = 0usize;

    while win.update() && !win.input.hit(three::KEY_ESCAPE) {
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
            splitter.reset();
            meshes.clear();

            for (i, plane) in planes.iter().enumerate() {
                let gray = (i + 1) * 0xFF / planes.len();
                let mat = three::material::basic::Custom {
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
                if let Some(poly) = plane_split::Polygon::from_transformed_rect(base_rect, transform, i) {
                    splitter.add(poly);
                }
            }
        }

        let mut points = Vec::new();
        let mut temp = Vec::new();
        let view_direction = {
            let node = win.scene
                .sync_guard()
                .resolve_world(&cam);
            let dir: [f32; 3] = cgmath::Quaternion::from(node.transform.orientation)
                .rotate_vector(cgmath::Vector3::unit_z())
                .into();
            euclid::TypedVector3D::from(dir)
        };

        //println!("{:?}", splitter);

        //reverse the order to draw front to back
        let results = splitter.sort(view_direction);
        for poly in results.iter().rev() {
            points.clear();
            for &k in &[0,1,2,2,3,0] {
                let p = &poly.points[k];
                points.push(mint::Point3::from([p.x, p.y, p.z]))
            }
            let start = points[0].clone();
            points.push(start);
            let geom = three::Geometry::with_vertices(points.clone());
            let gray = (poly.anchor + 1) * 0xFF / meshes.len();
            let mat = three::material::basic::Custom {
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
        if win.input.hit(three::KEY_SPACE) {
            println!("{:#?}", results);
        }
        frame_id += 1;

        controls.update(&win.input);
        win.render(&cam);
    }
}
