extern crate cgmath;
extern crate euclid;
extern crate mint;
extern crate plane_split;
extern crate ron;
#[macro_use]
extern crate serde_derive;
extern crate three;

use std::io::Read;
use std::fs::File;
use cgmath::prelude::*;
use plane_split::Splitter;

#[derive(Deserialize)]
struct Plane {
    pos: [f32; 3],
    rot: [f32; 3],
    scale: f32,
}

fn main() {
    let mut win = three::Window::new("Plane splitter", "../three-rs/data/shaders").build();
    let mut cam = win.factory.perspective_camera(60.0, 1.0, 10.0);
    let mut controls = three::OrbitControls::new(&cam, [0.0, 2.0, 5.0], [0.0, 0.0, 0.0]).build();
    win.scene.add(&cam);

    let mut contents = String::new();
    File::open("poly.ron").expect("Unable to open scene description")
         .read_to_string(&mut contents).unwrap();
    let planes: Vec<Plane> = ron::de::from_str(&contents).unwrap();

    let material = three::Material::MeshBasic{ color: 0xffffff, map: None, wireframe: true };
    let geometry = three::Geometry::new_plane(2.0, 2.0);
    let mut meshes: Vec<_> = planes.iter().map(|plane| {
        let mut m = win.factory.mesh(geometry.clone(), material.clone());
        let euler = cgmath::Quaternion::from(cgmath::Euler::new(
            cgmath::Deg(plane.rot[0]), cgmath::Deg(plane.rot[1]), cgmath::Deg(plane.rot[2])));
        m.set_transform(plane.pos, euler, plane.scale);
        m
    }).collect();

    for mesh in &meshes {
        win.scene.add(mesh);
    }

    let mut splitter = plane_split::BspSplitter::<f32, ()>::new();
    let rect = euclid::Rect::new(
        euclid::TypedPoint2D::new(-1.0, -1.0),
        euclid::TypedSize2D::new(2.0, 2.0));
    for mesh in &mut meshes {
        let node = mesh.sync(&win.scene);
        let decomposed = cgmath::Decomposed {
            disp: cgmath::Point3::from(node.world_transform.position).to_vec(),
            rot: cgmath::Quaternion::from(node.world_transform.orientation),
            scale: node.world_transform.scale,
        };
        let transform = euclid::TypedTransform3D::from_row_arrays(cgmath::Matrix4::from(decomposed).into());
        let poly = plane_split::Polygon::from_transformed_rect(rect, transform, 0);
        splitter.add(poly);
    }

    while win.update() && !three::KEY_ESCAPE.is_hit(&win.input) {
        let mut temp = Vec::new();
        let mut points = Vec::new();
        let view_dir = {
            let node = cam.sync(&win.scene);
            let dir = cgmath::Quaternion::from(node.world_transform.orientation) *
                cgmath::Vector3::unit_z();
            euclid::TypedVector3D::new(dir.x, dir.y, dir.z)
        };

        let results = splitter.sort(view_dir);
        for (i, poly) in results.iter().enumerate() {
            points.clear();
            for &k in &[0,1,2,2,3,0] {
                let p = &poly.points[k];
                points.push(mint::Point3::from([p.x, p.y, p.z]))
            }
            let start = points[0].clone();
            points.push(start);
            let geom = three::Geometry::from_vertices(points.clone());
            let red = (i + 1) * 0xFF / results.len();
            let mat = three::Material::MeshBasic{
                color: (red<<16) as u32 + 0x00ff00,
                map: None,
                wireframe: false,
            };
            let mesh = win.factory.mesh(geom, mat);
            win.scene.add(&mesh);
            temp.push(mesh);
        }

        controls.update(&win.input);
        win.render(&cam);
    }
}