#![allow(unused_imports)]
use ::core::panic;
use ::std::{ptr::{self, null_mut}, sync::mpsc::SyncSender, thread::JoinHandle};
use ::imgui_glow_renderer::glow::NativeTexture;
use ::log::error;
use ::rsplayer::app::start_analyzer;
use rusty_ffmpeg::ffi::{self, av_frame_unref};

use imgui::Context;
use ::imgui::{sys::{ImGuiSizeCallbackData, ImVec2}, TextureId};
use imgui_glow_renderer::{
    glow::{self, HasContext},
    AutoRenderer,
};
use imgui_sdl2_support::SdlPlatform;
use rsplayer::{
    analyzer_state::AnalyzerContext,
    app::open_movie,
    record_state::{FrameWrapper as RecordFrameWrapper, RecordState},
};

use sdl2::{
    event::Event,
    video::{GLProfile, Window},
};

// Create a new glow context.
fn glow_context(window: &Window) -> glow::Context {
    unsafe {
        glow::Context::from_loader_function(|s| window.subsystem().gl_get_proc_address(s) as _)
    }
}

fn main() {
    let mut clog = colog::default_builder();
    clog.filter(None, log::LevelFilter::Error);
    clog.init();

    /* initialize SDL and its video subsystem */
    let sdl = sdl2::init().unwrap();
    let video_subsystem = sdl.video().unwrap();

    /* hint SDL to initialize an OpenGL 3.3 core profile context */
    let gl_attr = video_subsystem.gl_attr();

    gl_attr.set_context_version(3, 3);
    gl_attr.set_context_profile(GLProfile::Core);

    /* create a new window, be sure to call opengl method on the builder when using glow! */
    let window = video_subsystem
        .window("Hello imgui-rs!", 1280, 720)
        .allow_highdpi()
        .opengl()
        .position_centered()
        .resizable()
        .build()
        .unwrap();

    /* create a new OpenGL context and make it current */
    let gl_context = window.gl_create_context().unwrap();
    window.gl_make_current(&gl_context).unwrap();

    /* enable vsync to cap framerate */
    window.subsystem().gl_set_swap_interval(1).unwrap();

    /* create new glow and imgui contexts */
    let gl = glow_context(&window);

    let _texture_id = create_texture(&gl);

    /* create context */
    let mut imgui = Context::create();

    /* disable creation of files on disc */
    imgui.set_ini_filename(None);
    imgui.set_log_filename(None);

    /* setup platform and renderer, and fonts to imgui */
    imgui
        .fonts()
        .add_font(&[imgui::FontSource::DefaultFontData { config: None }]);

    /* create platform and renderer */
    let mut platform = SdlPlatform::new(&mut imgui);
    let mut renderer = AutoRenderer::new(gl, &mut imgui).unwrap();

    // let gl = renderer.gl_context();
    /* start main loop */
    let mut event_pump = sdl.event_pump().unwrap();


    /* setup video analyzer */
    let mut analyzer_ctx = setup_video_ctx();
    let tx = unsafe { start_analyzer(&mut analyzer_ctx) };
    analyzer_ctx.pause();
    // event_loop(&mut analyzer_ctx, &mut subsystem, tx);

    'main: loop {
        unsafe {
            event_loop(&mut analyzer_ctx, &tx, &renderer.gl_context(), &_texture_id);
        };


        for event in event_pump.poll_iter() {
            /* pass all events to imgui platfrom */
            platform.handle_event(&mut imgui, &event);

            if let Event::Quit { .. } = event {
                break 'main;
            }
        }

        /* call prepare_frame before calling imgui.new_frame() */
        platform.prepare_frame(&mut imgui, &window, &event_pump);
        let ui = imgui.new_frame();
        /* create imgui UI here */
        setup_ui(&ui, &_texture_id);


        /* render */
        let draw_data = imgui.render();

        unsafe { renderer.gl_context().clear(glow::COLOR_BUFFER_BIT) };
        renderer.render(draw_data).unwrap();
        // imgui_sdl2_support::SdlPlatform::new(imgui)
        // unsafe {
        // ::sdl2::sys::SDL_RenderPresent(g_renderer);
        // };

        window.gl_swap_window();
    }
}
extern "C" {
    pub fn ImGui_ImplSDLRenderer2_NewFrame();
    pub fn ImGui_ImplSDL2_NewFrame();
}


fn setup_ui(ui: &imgui::Ui, n_tex: &NativeTexture) {

    unsafe {
        ::imgui::sys::igSetNextWindowSizeConstraints([100., 100.].into(), [1380., 1380.].into(), Some(aspect_ratio_callback), null_mut());
    }
    let vwindow = ui.window("Video Render");
    vwindow
        .size([435.0, 460.0], ::imgui::Condition::FirstUseEver)
        .position([100.0, 100.0], ::imgui::Condition::Appearing)
        // .size_constraints(size_min, size_max)
        .build(|| {
            let mouse_pos = ui.io().mouse_pos;
            // ui.text(format!(
            //     "Mouse Position: ({:.1},{:.1})",
            //     mouse_pos[0], mouse_pos[1]
            // ));
            let mut available_size = ui.content_region_avail();
            // ui.text(format!(
            //     "Content Area: ({:.1},{:.1})",
            //     available_size[0], available_size[1]
            // ));
            // ui.separator();
            available_size[0] = available_size[1] * 16. / 9.;
            let _img = ::imgui::Image::new(TextureId::new(n_tex.0.get() as _), [available_size[0], available_size[1]]);
            _img.build(ui);
        });
}

extern fn aspect_ratio_callback(data: *mut ImGuiSizeCallbackData) {
    // let mut data: &ImGuiSizeCallbackData = unsafe { &mut *(data as *mut ImGuiSizeCallbackData) };
    let data: &mut ImGuiSizeCallbackData = unsafe { data.as_mut().unwrap() };
    let ratio = 16. / 9.;
    if data.DesiredSize.x != data.CurrentSize.x {
        data.DesiredSize.y = data.DesiredSize.x / ratio;
        data.DesiredSize.x = data.DesiredSize.x;
    } else {
        data.DesiredSize.x = data.DesiredSize.y * ratio;
        data.DesiredSize.y = data.DesiredSize.y;
    }
}

fn create_texture(gl: &::imgui_glow_renderer::glow::Context) -> NativeTexture {
    const WIDTH: usize = 1024;
    const HEIGHT: usize = 768;
    let mut data = Vec::with_capacity(WIDTH * HEIGHT);
    for i in 0..WIDTH {
        for j in 0..HEIGHT {
            // Insert RGB values
            data.push(i as u8);
            data.push(j as u8);
            data.push((i + j) as u8);
        }
    }
    // let mut textures: imgui::Textures<NativeTexture> = imgui::Textures::new();
    let gl_texture = unsafe { gl.create_texture() }.expect("unable to create GL texture");
    let _texture_id = TextureId::new(gl_texture.0.get() as _);
    unsafe {
        gl.active_texture(glow::TEXTURE2); 
        gl.bind_texture(glow::TEXTURE_2D, Some(gl_texture));
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGB as _, // When generating a texture like this, you're probably working in linear color space
            WIDTH as _,
            HEIGHT as _,
            0,
            glow::BGR,
            glow::UNSIGNED_BYTE,
            // None,
            Some(&data),
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as _,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as _,
        );
    }
    gl_texture
}

fn setup_video_ctx() -> AnalyzerContext {
    let args: Vec<String> = std::env::args().collect();
    let default_file = String::from("test_vid.mp4");
    let mut analyzer_ctx = AnalyzerContext::new();

    unsafe {
        let filepath: std::ffi::CString =
            std::ffi::CString::new(args.get(1).unwrap_or(&default_file).as_str()).unwrap();
        let filepath_2 = std::ffi::CString::new("test_vid_r30.mp4").unwrap();
        open_movie(&mut analyzer_ctx, filepath.as_ptr());
        open_movie(&mut analyzer_ctx, filepath_2.as_ptr());
        // let tx = play_movie(&mut analyzer_ctx);

        analyzer_ctx.set_loop(true);
        // event_loop(&mut analyzer_ctx, &mut subsystem, tx);
        // AnalyzerContext::close(analyzer_ctx);
        // analyzer_ctx.close();
        // drop(analyzer_ctx);
    }
    analyzer_ctx
}


pub unsafe fn event_loop(
    analyzer_ctx: &mut AnalyzerContext,
    tx: &std::sync::mpsc::Sender<String>,
    gl: &::imgui_glow_renderer::glow::Context,
    n_tex: &NativeTexture
) {
    // let _ = sdl2::video::drivers().map(|d: &'static str| {
    //     eprintln!("driver {}", d);
    // });

    // info!("available video drivers: ");
    // let mut iter = sdl2::video::drivers();
    // iter.for_each(|i| info!(" * {}", iter.next().unwrap()));

    // info!("available audio drivers: ");
    // let _ = sdl2::audio::drivers().for_each(|d: &'static str| info!(" * {}", d));
    // .map(|d: &'static str| info!(" * audio driver: {}", d));

    // let mut event_pump = sdl.event_pump().unwrap();
    let mut i = 0;
    // let mut last_pts = 0;
    // let mut last_clock = ffi::av_gettime_relative();
    // let clock = ffi::av_gettime();
    // let mut record_tx: Option<SyncSender<RecordFrameWrapper>> = None;

    // let mut the_record_state = RecordState::new();

    // let fmt_ctx = movie_state.format_context.lock().unwrap().ptr;
    // let dest_frame = ffi::av_frame_alloc()
    //     .as_mut()
    //     .expect("failed to allocated memory for AVFrame");

    let mut record_handle: Option<JoinHandle<()>> = None;
    // 'running: loop {
        // i = (i + 1) % 255;
        i = i + 1;
        // for event in event_pump.poll_iter() {
        //     // println!("event: {:?}", event);
        //     match event {
        //         Event::Quit { .. } => {
        //             // keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
        //             tx.send("quit".to_string()).unwrap();
        //             break 'running;
        //         },
        //         Event::KeyDown{ .. } => {
        //             if let Event::KeyDown{keycode, ..} = event {
        //                 match keycode {
        //                     Some(Keycode::Space) => {
        //                         tx.send("pause".to_string()).unwrap();
        //                         analyzer_ctx.pause();
        //                     }
        //                     Some(Keycode::Period) => {
        //                         info!("analyzer step");
        //                         tx.send("step".to_string()).unwrap();
        //                         analyzer_ctx.step();
        //                     }
        //                     Some(Keycode::Q) | Some(Keycode::Escape) => {
        //                         record_tx = None;
        //                         // the_record_state.stop_recording_thread();
        //                         if let Some(record_handle) = record_handle {
        //                             record_handle.join().unwrap();
        //                         }
        //                         tx.send("quit".to_string()).unwrap();
        //                         break 'running;
        //                     }
        //                     _ => {}
        //                 }
        //             }
        //         }
        //         _ => {}
        //     }
        // }
        // The rest of the game loop goes here...
        // draw_ui(
        //     &mut subsystem.canvas,
        //     &mut ui_texture,
        //     subsystem.is_recording,
        // );

        // if analyzer_ctx.is_paused() == true {
        //     std::thread::yield_now();
        //     screen_cap(subsystem, &mut record_tx, i);
        //     continue;
        // }

        if analyzer_ctx.is_paused() == false || analyzer_ctx.force_render == false {
        let mut frame_remaining = 1./60.;
        let mut nearest_frame = -1.;

        let mut current_clock = unsafe {ffi::av_gettime_relative()};
        analyzer_ctx.update_current(current_clock);
        for index in 0..analyzer_ctx.movie_count() {
            if let (remaining, Some(mut dest_frame)) = analyzer_ctx.dequeue_frame(index as _) {
                if nearest_frame < remaining {
                    nearest_frame = remaining;
                }
                if index == 0 {
                    frame_to_texture(dest_frame.as_mut().unwrap(), &gl, &n_tex).unwrap_or_default();
                }
                // } else {
                //     frame_to_texture(dest_frame.as_mut().unwrap(), &mut movie_texture2).unwrap_or_default();
                // }
                ffi::av_frame_unref(dest_frame as *mut _);
                ffi::av_frame_free(&mut dest_frame as *mut *mut _);
            };
        };

        if nearest_frame < 0. {
            // info!("full frame sleep");
            ::std::thread::sleep(std::time::Duration::from_secs_f64(frame_remaining));
            // ::std::thread::yield_now();
            return;
        }
        if nearest_frame > 0. {
            // info!("delta frame sleep {}", nearest_frame);
            ::std::thread::sleep(std::time::Duration::from_secs_f64((nearest_frame - 0.0001).max(0.)));
        }
 
        let mut current_clock = unsafe {ffi::av_gettime_relative()};
        analyzer_ctx.update_clock(current_clock);
        }

        // last_clock = ffi::av_gettime_relative();
        // subsystem.canvas.present();
        analyzer_ctx.force_render = false;

        ::std::thread::yield_now();
    // }
    // drop(tx);
    // drop(record_tx);
    // ffi::av_free(dest_frame.opaque);
}

fn frame_to_texture(
    movie_frame: &mut ffi::AVFrame,
    gl: &::imgui_glow_renderer::glow::Context,
    tex: &NativeTexture
) -> Result<(), String> {

    const WIDTH: usize = 1280;
    const HEIGHT: usize = 720;
    unsafe {

        let vertex_program = gl.create_shader(glow::VERTEX_SHADER).unwrap();
        gl.shader_source(vertex_program, vertex_shader_source().as_str());
        gl.compile_shader(vertex_program);
        // error!("{}", gl.get_shader_compile_status(vertex_program));
        // error!("{}", gl.get_shader_info_log(vertex_program));

        let shader_program = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
        gl.shader_source(shader_program, yuv_rgb_shader_source().as_str());
        gl.compile_shader(shader_program);

        // error!("{}", gl.get_shader_compile_status(shader_program));
        let program = gl.create_program();
        let program = match program {
            Ok(p) => p,
            Err(s) => panic!("{}", s),
        };
        gl.attach_shader(program, vertex_program);
        gl.attach_shader(program, shader_program);
        gl.link_program(program);
        gl.use_program(Some(program));
        // gl.bind_frag_data_location(program, 0, "diffuseColor");



        // gl.active_texture(glow::TEXTURE2);
        // let frame_buff = gl.create_framebuffer().unwrap();
        // gl.bind_framebuffer(glow::FRAMEBUFFER, Some(frame_buff));
        // gl.bind_texture(glow::TEXTURE_2D , Some(*tex));
        // gl.tex_parameter_i32(
        //     glow::TEXTURE_2D,
        //     glow::TEXTURE_MIN_FILTER,
        //     glow::LINEAR as _,
        // );
        // gl.tex_parameter_i32(
        //     glow::TEXTURE_2D,
        //     glow::TEXTURE_MAG_FILTER,
        //     glow::LINEAR as _,
        // );
        // gl.bind_texture(glow::TEXTURE_2D , None);

        // gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(*tex), 0);

        // let rbo = gl.create_renderbuffer().unwrap();
        // gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rbo));
        // gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, WIDTH as _, HEIGHT as _);
        // gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_STENCIL_ATTACHMENT, glow::RENDERBUFFER, Some(rbo));
        // gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH_COMPONENT, WIDTH as _, HEIGHT as _);
        // gl.framebuffer_renderbuffer(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::RENDERBUFFER, Some(rbo));

       // gl.draw_buffer(glow::COLOR_ATTACHMENT0);


        // gl.tex_image_2d(
        //     glow::TEXTURE_2D,
        //     0,
        //     glow::R8 as _, // When generating a texture like this, you're probably working in linear color space
        //     WIDTH as _,
        //     HEIGHT as _,
        //     0,
        //     glow::RED,
        //     glow::UNSIGNED_BYTE,
        //     Some(
        // std::slice::from_raw_parts(movie_frame.data[0].offset(0), (WIDTH * HEIGHT * (2)) as _)
        //     )
        //     // Some(movie_frame.data.as_slice() as &[u8])
        // );
        // gl.generate_mipmap(glow::TEXTURE_2D);


        let tex2 = gl.create_texture().unwrap();
        gl.active_texture(glow::TEXTURE3);
        gl.bind_texture(glow::TEXTURE_2D, Some(tex2));
        let  i = gl.get_uniform_location(program, "background").unwrap();
        // gl.uniform_1_i32(Some(&i), glow::TEXTURE3 as _);
        gl.uniform_1_i32(Some(&i), 3);
        // error!("{}", gl.get_shader_info_log(shader_program));
        // error!("{}", gl.get_program_link_status(program));

        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as _,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as _,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::R8 as _, // When generating a texture like this, you're probably working in linear color space
            WIDTH as _,
            HEIGHT as _,
            0,
            glow::RED,
            glow::UNSIGNED_BYTE,
            Some (
                std::slice::from_raw_parts(movie_frame.data[0], (WIDTH * HEIGHT * (2)) as _)
            )
        );

        // gl.framebuffer_texture(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, Some(*tex), 0);
        // gl.draw_buffers(&[glow::COLOR_ATTACHMENT0]);

        // gl.bind_framebuffer(glow::FRAMEBUFFER, Some(frame_buff));
        // gl.viewport(0, 0, WIDTH as _, HEIGHT as _);

        // gl.blit_framebuffer(0, 0, WIDTH as _, HEIGHT as _, 0, 0, WIDTH as _, HEIGHT as _, glow::COLOR_BUFFER_BIT, glow::LINEAR);
 
        // gl.bind_renderbuffer(glow::RENDERBUFFER, None);
        // gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        // gl.viewport(0, 0, WIDTH as _, HEIGHT as _);

        // gl.tex_image_2d(
        //     glow::TEXTURE_2D,
        //     0,
        //     glow::R3_G3_B2 as _, // When generating a texture like this, you're probably working in linear color space
        //     WIDTH as _,
        //     HEIGHT as _,
        //     0,
        //     glow::R3_G3_B2,
        //     glow::UNSIGNED_BYTE,
        //     Some(
        //         std::slice::from_raw_parts(movie_frame.data[0], (1280 * 720 * 1) as _)
        //     )
        // )
        // SDL_UpdateYUVTexture(
        //     texture.raw(),
        //     ::std::ptr::null(),
        //     movie_frame.data[0],
        //     movie_frame.linesize[0],
        //     movie_frame.data[1],
        //     movie_frame.linesize[1],
        //     movie_frame.data[2],
        //     movie_frame.linesize[2],
        // );

        unsafe {
            let rgb_frame = ffi::av_frame_alloc()
                .as_mut()
                .expect("failed to allocated memory for AVFrame during rgb conversion");

            rgb_frame.width = WIDTH as _;
            rgb_frame.height = HEIGHT as _;
            rgb_frame.format = ffi::AVPixelFormat_AV_PIX_FMT_RGB24;
            rgb_frame.time_base = ffi::AVRational { num: 1, den: 60 };
            let ret = ffi::av_frame_get_buffer(rgb_frame, 0);
            let rgb_convert_ctx = ffi::sws_getContext(
                WIDTH as _, HEIGHT as _, ffi::AVPixelFormat_AV_PIX_FMT_YUV420P,
                WIDTH as _, HEIGHT as _, ffi::AVPixelFormat_AV_PIX_FMT_RGB24,
                0, ptr::null_mut(), ptr::null_mut(), ptr::null());
            ffi::sws_scale(rgb_convert_ctx,
                movie_frame.data.as_slice().as_ptr() as _, movie_frame.linesize.as_slice().as_ptr() as _,
                0, HEIGHT as _,
                rgb_frame.data.as_slice().as_ptr() as _, rgb_frame.linesize.as_slice().as_ptr() as _);


            // gl.active_texture(glow::TEXTURE2);
            // // gl.bind_texture(glow::TEXTURE_2D, Some(*tex));
            // gl.tex_image_2d(
            //     glow::TEXTURE_2D,
            //     0,
            //     glow::RGB as _, // When generating a texture like this, you're probably working in linear color space
            //     WIDTH as _,
            //     HEIGHT as _,
            //     0,
            //     glow::RGB,
            //     glow::UNSIGNED_BYTE,
            //     Some (
            //         std::slice::from_raw_parts(rgb_frame.data[0] as _, (WIDTH * HEIGHT * (3)) as _)
            //     )
            // );
        sdl2::sys::SDL_UpdateYUVTexture(
            sdl2::sys::SDL_Texture{};
            ::std::ptr::null(),
            movie_frame.data[0],
            movie_frame.linesize[0],
            movie_frame.data[1],
            movie_frame.linesize[1],
            movie_frame.data[2],
            movie_frame.linesize[2],
        );


            ffi::av_frame_free(rgb_frame.opaque as *mut _);
        }
    };
    Ok(())
}

fn vertex_shader_source() -> String {
    return "
    uniform mat4 uVPMatrix;
attribute vec4 a_Position;
attribute vec2 a_TexCoord;
varying vec2 vTextureCoord;

void main(void)
{
    vTextureCoord = a_TexCoord;
    vTextureCoord = vec2(a_TexCoord.x, (1.0 - (a_TexCoord.y)));
    gl_Position =  a_Position;
}
    ".to_owned();
}
fn yuv_rgb_shader_source() -> String {
    return "
#version 330 core
#ifdef GL_ES
precision mediump float;
#endif
uniform sampler2D background;
in vec2 vTextureCoord;
layout(location = 0) out vec4 diffuseColor;
void main() {
    vec4 col = texture2D(background, vTextureCoord);
    diffuseColor = vec4(0.0, col.r, 0.0, 1.0);
}".to_owned()
}

fn yuv_rgb_shader_source3() -> String {
    return "
    #ifdef GL_ES
precision mediump float;
#endif
uniform sampler2DRect sTextureY;
void main(void) {
  float nx,ny,r,g,b,y,u,v;
  vec4 txl,ux,vx;
  nx=gl_TexCoord[0].x;
  ny=576.0-gl_TexCoord[0].y;
  y=texture2DRect(sTextureY,vec2(nx,ny)).r;
  u = 1.0;
  v = 1.0;

  y=1.1643*(y-0.0625);
  u=u-0.5;
  v=v-0.5;

  r=y+1.5958*v;
  g=y-0.39173*u-0.81290*v;
  b=y+2.017*u;
  gl_FragColor=vec4(g,r,b,1.0);
}".to_owned()

	// # gl_FragColor = vec4(st.x,st.y,0.0,1.0);
	// # diffuseColor = vec4(st.x,st.y,0.0,1.0);
}
fn yuv_rgb_shader_source2() -> String {
    return "
/** A fragment shader to convert YUV420P to RGB.
  * Input textures Y - is a block of size w*h. 
  * texture U is of size w/2*h/2.
  * texture V is of size w/2*h/2. 
  * In this case, the layout looks like the following :
  * __________
  * |        |
  * |   Y    | size = w*h
  * |        |
  * |________|
  * |____U___|size = w*(h/4)
  * |____V___|size = w*(h/4)
  */
precision highp float;
varying vec2 vTextureCoord;
uniform sampler2D sTextureY;
uniform sampler2D sTextureU;
uniform sampler2D sTextureV;
void main (void) {
    float r, g, b, y, u, v;
    y = texture2D(sTextureY, vTextureCoord).r;
    u = texture2D(sTextureU, vTextureCoord).r;
    v = texture2D(sTextureV, vTextureCoord).r;

    y = 1.1643*(y-0.0625);
    u = u-0.5;
    v = v-0.5;

    r = y+1.5958*v;
    g = y-0.39173*u-0.81290*v;
    b = y+2.017*u;
    gl_FragColor = vec4(g, 1.0, b, 1.0);
}
    ".to_owned();
}
