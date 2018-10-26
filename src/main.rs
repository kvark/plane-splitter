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
use cgmath::ApproxEq;
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


#[derive(Deserialize)]
enum Transform<T> {
    Component {
        pos: [T; 3],
        rot: [T; 3],
        scale: T,
    },
    Matrix {
        x: [T; 4],
        y: [T; 4],
        z: [T; 4],
    },
}

#[derive(Deserialize)]
struct Plane {
    transform: Transform<f32>,
    extent: [f32; 2],
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
                let geometry = three::Geometry::plane(2.0 * plane.extent[0], 2.0 * plane.extent[1]);
                let gray = (i + 1) * 0xFF / planes.len();
                let mat = three::material::basic::Custom {
                    color: (gray as u32) * 0x010101,
                    map: None,
                    pipeline: pipeline_base.clone(),
                };
                let mut mesh = win.factory.mesh(geometry.clone(), mat);

                let transform = match plane.transform {
                    Transform::Component { pos, rot, scale } => {
                        let quat = cgmath::Quaternion::from(cgmath::Euler::new(
                            cgmath::Deg(rot[0]), cgmath::Deg(rot[1]), cgmath::Deg(rot[2]))
                        );
                        mesh.set_transform(pos, quat, scale);

                        let decomposed = cgmath::Decomposed {
                            disp: cgmath::Vector3::from(pos),
                            rot: quat,
                            scale,
                        };
                        cgmath::Matrix4::from(decomposed)
                    }
                    Transform::Matrix { x, y, z } => {
                        let vx = cgmath::vec3(x[0], x[1], x[2]);
                        let vy = cgmath::vec3(y[0], y[1], y[2]);
                        let vz = cgmath::vec3(z[0], z[1], z[2]);

                        let scale3 = cgmath::vec3(
                            vx.magnitude(),
                            vy.magnitude(),
                            vz.magnitude(),
                        );
                        let eps = f32::default_epsilon();
                        let dist = f32::default_max_relative();
                        if !scale3.y.relative_eq(&scale3.x, eps, dist) || !scale3.z.relative_eq(&scale3.x, eps, dist) {
                            //warn!("Bad scale {:?} on plane [{}]", scale3, i);
                        }

                        let rot = cgmath::Quaternion::from(cgmath::Matrix3::from_cols(
                            vx / scale3.x,
                            vy / scale3.y,
                            vz / scale3.z,
                        ));
                        mesh.set_transform(
                            cgmath::Point3::new(x[3], y[3], z[3]),
                            rot,
                            (scale3.x + scale3.y + scale3.z) / 3.0,
                        );

                        cgmath::Matrix4::from_cols(
                            x.into(),
                            y.into(),
                            z.into(),
                            cgmath::vec4(0.0, 0.0, 0.0, 1.0),
                        )
                    }
                };

                win.scene.add(&mesh);
                meshes.push(mesh);

                let base_rect = euclid::Rect::new(
                    euclid::TypedPoint2D::new(-plane.extent[0], -plane.extent[1]),
                    euclid::TypedSize2D::new(2.0 * plane.extent[0], 2.0 * plane.extent[1]),
                );
                let transform = euclid::TypedTransform3D::from_row_arrays(transform.into());
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
