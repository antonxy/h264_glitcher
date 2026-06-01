use super::renderer::create_texture;
use glow::HasContext;
use std::fs::File;
use std::path::Path;

pub fn load_mask_texture(gl: &glow::Context, path: &Path) -> Option<glow::Texture> {
    let file = File::open(path).ok()?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;

    if info.color_type != png::ColorType::Indexed && info.color_type != png::ColorType::Grayscale {
        eprintln!("Mask must be indexed or grayscale PNG, got {:?}", info.color_type);
        return None;
    }

    if info.bit_depth != png::BitDepth::Eight {
        eprintln!("Mask must be 8-bit, got {:?}", info.bit_depth);
        return None;
    }

    Some(upload_mask(gl, info.width, info.height, &buf))
}

pub fn create_default_mask(gl: &glow::Context, width: u32, height: u32) -> glow::Texture {
    let data = vec![255u8; (width * height) as usize];
    upload_mask(gl, width, height, &data)
}

fn upload_mask(gl: &glow::Context, width: u32, height: u32, data: &[u8]) -> glow::Texture {
    unsafe {
        let texture = create_texture(gl);
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::R8UI as i32,
            width as i32,
            height as i32,
            0,
            glow::RED_INTEGER,
            glow::UNSIGNED_BYTE,
            Some(data),
        );
        texture
    }
}
