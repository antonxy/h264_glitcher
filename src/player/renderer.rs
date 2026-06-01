use super::decoder::DecodedFrame;
use super::uniforms::Uniforms;
use glow::HasContext;
use std::sync::mpsc::Receiver;

pub struct Renderer {
    gl: glow::Context,
    program: glow::Program,
    vao: glow::VertexArray,
    tex_y: glow::Texture,
    tex_u: glow::Texture,
    tex_v: glow::Texture,
    tex_mask: glow::Texture,
    frame_width: u32,
    frame_height: u32,
    loc_time: Option<glow::UniformLocation>,
    loc_flash: Option<glow::UniformLocation>,
    loc_effect_color: Option<glow::UniformLocation>,
}

impl Renderer {
    pub fn new(gl: glow::Context, mask_texture: glow::Texture) -> Self {
        unsafe {
            let program = create_shader_program(&gl);
            let vao = gl.create_vertex_array().expect("Failed to create VAO");

            // Create YUV textures initialized to black (Y=0, U=128, V=128)
            let tex_y = create_texture(&gl);
            upload_texture(&gl, glow::TEXTURE0, tex_y, 1, 1, &[0u8], true);
            let tex_u = create_texture(&gl);
            upload_texture(&gl, glow::TEXTURE1, tex_u, 1, 1, &[128u8], true);
            let tex_v = create_texture(&gl);
            upload_texture(&gl, glow::TEXTURE2, tex_v, 1, 1, &[128u8], true);

            // Set texture units
            gl.use_program(Some(program));
            if let Some(loc) = gl.get_uniform_location(program, "tex_y") {
                gl.uniform_1_i32(Some(&loc), 0);
            }
            if let Some(loc) = gl.get_uniform_location(program, "tex_u") {
                gl.uniform_1_i32(Some(&loc), 1);
            }
            if let Some(loc) = gl.get_uniform_location(program, "tex_v") {
                gl.uniform_1_i32(Some(&loc), 2);
            }
            if let Some(loc) = gl.get_uniform_location(program, "tex_mask") {
                gl.uniform_1_i32(Some(&loc), 3);
            }

            let loc_time = gl.get_uniform_location(program, "time");
            let loc_flash = gl.get_uniform_location(program, "flash");
            let loc_effect_color = gl.get_uniform_location(program, "effect_color");

            Self {
                gl,
                program,
                vao,
                tex_y,
                tex_u,
                tex_v,
                tex_mask: mask_texture,
                frame_width: 0,
                frame_height: 0,
                loc_time,
                loc_flash,
                loc_effect_color,
            }
        }
    }

    pub fn upload_frame(&mut self, frame: &DecodedFrame) {
        let needs_resize = frame.width != self.frame_width || frame.height != self.frame_height;
        let uv_width = frame.width / 2;
        let uv_height = frame.height / 2;

        unsafe {
            upload_texture(&self.gl, glow::TEXTURE0, self.tex_y, frame.width, frame.height, &frame.y_plane, needs_resize);
            upload_texture(&self.gl, glow::TEXTURE1, self.tex_u, uv_width, uv_height, &frame.u_plane, needs_resize);
            upload_texture(&self.gl, glow::TEXTURE2, self.tex_v, uv_width, uv_height, &frame.v_plane, needs_resize);
        }

        self.frame_width = frame.width;
        self.frame_height = frame.height;
    }

    pub fn draw(&self, uniforms: &Uniforms, width: u32, height: u32) {
        unsafe {
            self.gl.viewport(0, 0, width as i32, height as i32);
            self.gl.clear_color(0.0, 0.0, 0.0, 1.0);
            self.gl.clear(glow::COLOR_BUFFER_BIT);

            self.gl.use_program(Some(self.program));
            self.gl.bind_vertex_array(Some(self.vao));

            // Bind textures
            self.gl.active_texture(glow::TEXTURE0);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_y));
            self.gl.active_texture(glow::TEXTURE1);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_u));
            self.gl.active_texture(glow::TEXTURE2);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_v));
            self.gl.active_texture(glow::TEXTURE3);
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.tex_mask));

            // Set uniforms
            if let Some(ref loc) = self.loc_time {
                self.gl.uniform_1_f32(Some(loc), uniforms.time);
            }
            if let Some(ref loc) = self.loc_flash {
                self.gl.uniform_1_f32(Some(loc), uniforms.flash);
            }
            if let Some(ref loc) = self.loc_effect_color {
                self.gl.uniform_3_f32(
                    Some(loc),
                    uniforms.effect_color[0],
                    uniforms.effect_color[1],
                    uniforms.effect_color[2],
                );
            }

            self.gl.draw_arrays(glow::TRIANGLES, 0, 6);
        }
    }
}

pub unsafe fn create_texture(gl: &glow::Context) -> glow::Texture {
    let texture = gl.create_texture().expect("Failed to create texture");
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
    gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    texture
}

pub unsafe fn upload_texture(
    gl: &glow::Context,
    unit: u32,
    texture: glow::Texture,
    width: u32,
    height: u32,
    data: &[u8],
    needs_resize: bool,
) {
    gl.active_texture(unit);
    gl.bind_texture(glow::TEXTURE_2D, Some(texture));
    if needs_resize {
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::R8 as i32,
            width as i32,
            height as i32,
            0,
            glow::RED,
            glow::UNSIGNED_BYTE,
            Some(data),
        );
    } else {
        gl.tex_sub_image_2d(
            glow::TEXTURE_2D,
            0,
            0,
            0,
            width as i32,
            height as i32,
            glow::RED,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(data),
        );
    }
}

unsafe fn create_shader_program(gl: &glow::Context) -> glow::Program {
    let vert_src = include_str!("shader.vert");
    let frag_src = include_str!("shader.frag");

    let vert_shader = gl.create_shader(glow::VERTEX_SHADER).expect("Failed to create vertex shader");
    gl.shader_source(vert_shader, vert_src);
    gl.compile_shader(vert_shader);
    if !gl.get_shader_compile_status(vert_shader) {
        panic!("Vertex shader error: {}", gl.get_shader_info_log(vert_shader));
    }

    let frag_shader = gl.create_shader(glow::FRAGMENT_SHADER).expect("Failed to create fragment shader");
    gl.shader_source(frag_shader, frag_src);
    gl.compile_shader(frag_shader);
    if !gl.get_shader_compile_status(frag_shader) {
        panic!("Fragment shader error: {}", gl.get_shader_info_log(frag_shader));
    }

    let program = gl.create_program().expect("Failed to create program");
    gl.attach_shader(program, vert_shader);
    gl.attach_shader(program, frag_shader);
    gl.link_program(program);
    if !gl.get_program_link_status(program) {
        panic!("Program link error: {}", gl.get_program_info_log(program));
    }

    gl.delete_shader(vert_shader);
    gl.delete_shader(frag_shader);

    program
}

pub fn drain_latest_frame(receiver: &Receiver<DecodedFrame>) -> Option<DecodedFrame> {
    let mut latest = None;
    while let Ok(frame) = receiver.try_recv() {
        latest = Some(frame);
    }
    latest
}
