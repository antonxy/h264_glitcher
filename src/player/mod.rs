pub mod decoder;
pub mod mask;
pub mod renderer;
pub mod uniforms;

pub use decoder::{DecodedFrame, start_decoder_thread_from_path};
pub use mask::{create_default_mask, load_mask_texture};
pub use renderer::{drain_latest_frame, Renderer};
pub use uniforms::{start_osc_listener, Uniforms};

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, Version};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::{SurfaceAttributesBuilder, WindowSurface};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasRawWindowHandle;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::{Fullscreen, WindowBuilder};
use os_pipe::PipeReader;
use std::os::fd::OwnedFd;
use std::os::fd::AsRawFd;

pub fn run(pipe_reader: PipeReader, mask_path: Option<&Path>) {
    // Start decoder reading from the pipe
    let fd : OwnedFd = pipe_reader.into();
    let input_path = format!("/dev/fd/{}", fd.as_raw_fd());
    let frame_receiver = start_decoder_thread_from_path(input_path);

    let event_loop = EventLoop::new().expect("Failed to create event loop");

    // Get primary monitor for borderless fullscreen
    let monitor = event_loop.primary_monitor().or_else(|| {
        event_loop.available_monitors().next()
    });

    let window_builder = WindowBuilder::new()
        .with_title("H264 Glitcher")
        .with_fullscreen(Some(Fullscreen::Borderless(monitor)));

    let template = ConfigTemplateBuilder::new()
        .with_alpha_size(8);

    let display_builder = DisplayBuilder::new().with_window_builder(Some(window_builder));

    let (window, gl_config) = display_builder
        .build(&event_loop, template, |configs| {
            configs
                .reduce(|accum, config| {
                    if config.num_samples() > accum.num_samples() {
                        config
                    } else {
                        accum
                    }
                })
                .unwrap()
        })
        .expect("Failed to create window");

    let window = window.expect("Failed to get window");
    let raw_window_handle = window.raw_window_handle();
    let gl_display = gl_config.display();

    let context_attributes = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
        .build(Some(raw_window_handle));

    let gl_context = unsafe {
        gl_display
            .create_context(&gl_config, &context_attributes)
            .expect("Failed to create GL context")
    };

    let size = window.inner_size();
    let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
        raw_window_handle,
        NonZeroU32::new(size.width).unwrap(),
        NonZeroU32::new(size.height).unwrap(),
    );

    let gl_surface = unsafe {
        gl_display
            .create_window_surface(&gl_config, &surface_attributes)
            .expect("Failed to create surface")
    };

    let gl_context = gl_context
        .make_current(&gl_surface)
        .expect("Failed to make context current");

    // Set swap interval for vsync
    gl_surface
        .set_swap_interval(&gl_context, glutin::surface::SwapInterval::Wait(NonZeroU32::new(1).unwrap()))
        .ok();

    let gl = unsafe {
        glow::Context::from_loader_function(|s| {
            gl_display.get_proc_address(&std::ffi::CString::new(s).unwrap()) as *const _
        })
    };

    // Load mask texture or create default
    let mask_texture = mask_path
        .and_then(|p| load_mask_texture(&gl, p))
        .unwrap_or_else(|| create_default_mask(&gl, 1920, 1080));

    let mut renderer = Renderer::new(gl, mask_texture);

    // Shared uniforms
    let uniforms = Arc::new(Mutex::new(Uniforms::default()));
    start_osc_listener(Arc::clone(&uniforms), 9001);

    let start_time = Instant::now();

    event_loop
        .run(move |event, target| {
            match event {
                Event::WindowEvent { event, .. } => match event {
                    WindowEvent::CloseRequested => target.exit(),
                    WindowEvent::RedrawRequested => {
                        // Drain to get latest frame
                        if let Some(frame) = drain_latest_frame(&frame_receiver) {
                            renderer.upload_frame(&frame);
                        }

                        // Update time uniform
                        {
                            let mut u = uniforms.lock().unwrap();
                            u.time = start_time.elapsed().as_secs_f32();
                        }

                        let u = uniforms.lock().unwrap().clone();
                        let size = window.inner_size();
                        renderer.draw(&u, size.width, size.height);

                        gl_surface.swap_buffers(&gl_context).expect("Failed to swap buffers");

                        // Request next frame
                        window.request_redraw();
                    }
                    WindowEvent::Resized(size) => {
                        if size.width > 0 && size.height > 0 {
                            gl_surface.resize(
                                &gl_context,
                                NonZeroU32::new(size.width).unwrap(),
                                NonZeroU32::new(size.height).unwrap(),
                            );
                        }
                    }
                    _ => {}
                },
                Event::AboutToWait => {
                    window.request_redraw();
                }
                _ => {}
            }
        })
        .expect("Event loop error");
}
