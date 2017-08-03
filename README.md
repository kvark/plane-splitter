# plane-splitter

This is a testbed for [plane-split](https://crates.io/crates/plane-split) crate used by [WebRender](https://github.com/servo/webrender).

Visualization is done via [three-rs](https://github.com/kvark/three-rs). User can rotate the camera by holding the left mouse button and moving the mouse.

Input data is read from `data/poly.ron` file in [Rusty notation](https://github.com/ron-rs/ron), it's automatically re-loaded live whenever the file is saved.
