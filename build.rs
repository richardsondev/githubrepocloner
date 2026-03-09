#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::env;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=img/icon.svg");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let ico_path = Path::new(&out_dir).join("icon.ico");

    generate_ico(&ico_path);

    // Embed icon as a Windows resource when targeting Windows
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(ico_path.to_str().expect("Invalid ICO path"));
        if let Err(e) = res.compile() {
            println!("cargo:warning=Failed to embed Windows icon resource: {e}");
        }
    }
}

/// Render the SVG at multiple sizes and write an ICO file.
fn generate_ico(ico_path: &Path) {
    let svg_data = std::fs::read("img/icon.svg").expect("Failed to read img/icon.svg");

    let opt = resvg::usvg::Options::default();
    let tree =
        resvg::usvg::Tree::from_data(&svg_data, &opt).expect("Failed to parse img/icon.svg");

    let sizes: &[u32] = &[16, 32, 48, 256];
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);

    for &size in sizes {
        let mut pixmap =
            resvg::tiny_skia::Pixmap::new(size, size).expect("Failed to create pixmap");

        let sx = size as f32 / tree.size().width();
        let sy = size as f32 / tree.size().height();
        let transform = resvg::tiny_skia::Transform::from_scale(sx, sy);

        resvg::render(&tree, transform, &mut pixmap.as_mut());

        let image = ico::IconImage::from_rgba_data(size, size, pixmap.data().to_vec());
        icon_dir.add_entry(
            ico::IconDirEntry::encode(&image).expect("Failed to encode ICO entry"),
        );
    }

    let file = File::create(ico_path).expect("Failed to create ICO file");
    let mut writer = BufWriter::new(file);
    icon_dir
        .write(&mut writer)
        .expect("Failed to write ICO file");
}
