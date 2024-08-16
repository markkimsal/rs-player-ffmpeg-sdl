#![allow(unused_variables, dead_code, unused_imports, unused_mut)]
use std::{
    io::Write,
    ops::Deref,
    ptr::{slice_from_raw_parts, slice_from_raw_parts_mut},
    sync::mpsc::{Sender, SyncSender},
    thread::JoinHandle,
    time::Duration,
};

use log::{debug, info};
use rusty_ffmpeg::ffi::{self, av_frame_unref};

use sdl2::{
    event::Event,
    keyboard::Keycode,
    pixels::{Color, PixelFormatEnum},
    render::{Canvas, Texture, TextureAccess},
    sys::{SDL_LockTexture, SDL_UnlockTexture, SDL_UpdateYUVTexture},
    video::Window,
    Error, Sdl,
};

use rsplayer::{
    analyzer_state::AnalyzerContext,
    app::{open_movie, play_movie},
    movie_state::{self, FormatContextWrapper, FrameWrapper, MovieState},
    record_state::{FrameWrapper as RecordFrameWrapper, RecordState},
};

// static CANVAS: Option<Canvas<Window>> = None;
pub struct SdlSubsystemCtx {
    sdl_ctx: Sdl,
    canvas: Canvas<Window>,
    is_recording: bool,
}

pub unsafe fn init_subsystem<'sdl>(
    default_width: u32,
    default_height: u32,
) -> Result<SdlSubsystemCtx, Error> {
    let sdl_ctx = sdl2::init().unwrap();
    let video_subsystem = match sdl_ctx.video() {
        Ok(video_subsystem) => video_subsystem,
        Err(err) => {
            eprintln!("Error: {}", err);
            return Err(Error::UnsupportedError);
        }
    };

    #[allow(unused_mut)]
    let mut window_flags: u32 = 0;
    // window_flags |= sdl2::sys::SDL_WindowFlags::SDL_WINDOW_BORDERLESS as u32;
    let window = video_subsystem
        .window("rs-player-ffmpeg-sdl2", default_width, default_height)
        .resizable()
        .position_centered()
        .set_window_flags(window_flags)
        .borderless()
        .build()
        .unwrap();

    // let canvas = sdl2::sys::SDL_CreateRenderer(window.raw(), -1, sdl2::sys::SDL_RendererFlags::SDL_RENDERER_ACCELERATED);
    // let canvas = Canvas{ target: window.raw(), context: 1, default_pixel_format: todo!()  }
    let canvas = window
        .into_canvas()
        // .software()
        .accelerated()
        .build()
        .unwrap();

    Ok(SdlSubsystemCtx {
        sdl_ctx,
        canvas,
        is_recording: false,
    })
}

fn main() {
    let mut clog = colog::default_builder();
    clog.filter(None, log::LevelFilter::Info);
    clog.init();

    let args: Vec<String> = std::env::args().collect();
    let default_file = String::from("test_vid.mp4");
    let mut analyzer_ctx = AnalyzerContext::new();

    let window_width:  u32 = 1280;
    let window_height: u32 = 720;
 
    unsafe {
        let mut subsystem = match init_subsystem(window_width, window_height) {
            Ok(s) => s,
            Err(e) => {
                return;
            }
        };

        let filepath: std::ffi::CString =
            std::ffi::CString::new(args.get(1).unwrap_or(&default_file).as_str()).unwrap();
        open_movie(&mut analyzer_ctx, filepath.as_ptr()); //, &mut video_state);
        let tx = play_movie(&mut analyzer_ctx);
        event_loop(&mut analyzer_ctx, &mut subsystem, tx)
    }
}
pub unsafe fn event_loop(
    analyzer_ctx: &mut AnalyzerContext,
    subsystem: &mut SdlSubsystemCtx,
    tx: std::sync::mpsc::Sender<String>,
) {
    subsystem.canvas.set_draw_color(Color::RGB(0, 255, 255));
    subsystem.canvas.clear();
    subsystem.canvas.present();
    let texture_creator = subsystem.canvas.texture_creator();

    let textw: u32 = subsystem.canvas.viewport().width() as u32;
    let texth: u32 = subsystem.canvas.viewport().height() as u32;
    let mut texture: Texture = texture_creator
        .create_texture(
            Some(PixelFormatEnum::IYUV),
            TextureAccess::Target,
            textw,
            texth,
        )
        .unwrap();

    let mut movie_texture: Texture = texture_creator
        .create_texture(
            Some(PixelFormatEnum::IYUV),
            TextureAccess::Target,
            textw,
            texth,
        )
        .unwrap();

    let mut draw_texture: Texture = texture_creator
        .create_texture(
            Some(PixelFormatEnum::IYUV),
            TextureAccess::Target,
            textw,
            texth,
        )
        .unwrap();

    let mut ui_texture: Texture = texture_creator
        .create_texture(
            Some(PixelFormatEnum::IYUV),
            TextureAccess::Target,
            textw,
            texth,
        )
        .unwrap();

    let _ = sdl2::video::drivers().map(|d: &'static str| {
        eprintln!("driver {}", d);
    });

    info!("available video drivers: ");
    let mut iter = sdl2::video::drivers();
    iter.for_each(|i| info!(" * {}", iter.next().unwrap()));

    info!("available audio drivers: ");
    let _ = sdl2::audio::drivers().for_each(|d: &'static str| info!(" * {}", d));
    // .map(|d: &'static str| info!(" * audio driver: {}", d));

    let info = subsystem.canvas.deref().info();
    dbg!(&info);

    // let info = texture.query();
    // dbg!(&info);
    // let info = subsystem.canvas.info();
    // dbg!(&info);

    // let foo = texture.raw();
    // let mut renderer = sdl2::sys::SDL_CreateRenderer(subsystem.canvas.window().raw(), 1, sdl2::sys::SDL_RendererFlags::SDL_RENDERER_SOFTWARE as _);
    // if (renderer).is_null() {
    //     eprintln!("Failed to create renderer");
    //     let foo = sdl2::sys::SDL_GetError();
    //     dbg!(&sdl2::sys::SDL_GetError());
    //     return;
    // }
    // let ui_texture = texture_creator.create_texture(Some(PixelFormatEnum::IYUV), TextureAccess::Target, textw, texth).unwrap();
    // sdl2::sys::SDL_SetRenderTarget(renderer, ui_texture.raw());

    let mut event_pump = subsystem.sdl_ctx.event_pump().unwrap();
    let mut i = 0;
    let mut last_pts = 0;
    let mut last_clock = ffi::av_gettime_relative();
    let clock = ffi::av_gettime();
    let mut record_tx: Option<SyncSender<RecordFrameWrapper>> = None;

    let mut the_record_state = RecordState::new();

    // let fmt_ctx = movie_state.format_context.lock().unwrap().ptr;
    // let dest_frame = ffi::av_frame_alloc()
    //     .as_mut()
    //     .expect("failed to allocated memory for AVFrame");

    let mut record_handle: Option<JoinHandle<()>> = None;
    'running: loop {
        // i = (i + 1) % 255;
        i = i + 1;
        for event in event_pump.poll_iter() {
            // println!("event: {:?}", event);
            match event {
                Event::Quit { .. } => {
                    // keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
                    tx.send("quit".to_string()).unwrap();
                    break 'running;
                },
                Event::KeyDown{ .. } => {
                    if let Event::KeyDown{keycode, ..} = event {
                        match keycode {
                            Some(Keycode::R) => {
                                match subsystem.is_recording {
                                    true => {
                                        tx.send("Stop recording".to_string()).unwrap();
                                        record_tx = None;
                                    }
                                    false => {
                                        tx.send("Start recording".to_string()).unwrap();
                                        // record_tx = the_record_state.start_recording_thread();
                                        let result = the_record_state.start_recording_thread();
                                        record_tx = Some(result.0);
                                        record_handle = result.1;
                                    }
                                }
                                subsystem.is_recording = !subsystem.is_recording;
                            }
                            Some(Keycode::Space) => {
                                tx.send("pause".to_string()).unwrap();
                                analyzer_ctx.pause();
                            }
                            Some(Keycode::Period) => {
                                tx.send("step".to_string()).unwrap();
                                analyzer_ctx.step();
                            }
                            Some(Keycode::Q) | Some(Keycode::Escape) => {
                                record_tx = None;
                                // the_record_state.stop_recording_thread();
                                if let Some(record_handle) = record_handle {
                                    record_handle.join().unwrap();
                                }
                                tx.send("quit".to_string()).unwrap();
                                break 'running;
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        // The rest of the game loop goes here...
        draw_ui(
            &mut subsystem.canvas,
            &mut ui_texture,
            subsystem.is_recording,
        );

        // if analyzer_ctx.is_paused() == true {
        //     std::thread::yield_now();
        //     screen_cap(subsystem, &mut record_tx, i);
        //     continue;
        // }


        if let Some(dest_frame) = analyzer_ctx.dequeue_frame() {
            // let dest_frame = dest_frame.ptr;

            frame_to_texture(dest_frame.as_mut().unwrap(), &mut movie_texture).unwrap_or_default();

            // texture_to_texture(
            //     &mut movie_texture,
            //     &mut subsystem.canvas,
            //     &mut texture,
            // ).unwrap_or_default();
            ffi::av_frame_unref(dest_frame as *mut _);
        };

        composite(&mut subsystem.canvas, &mut texture, &mut movie_texture);
        composite(&mut subsystem.canvas, &mut texture, &mut ui_texture);
        blit_texture(&mut subsystem.canvas, &mut texture).unwrap_or_default();
        // blit_texture(&mut subsystem.canvas, &mut ui_texture).unwrap_or_default();

        // last_clock = ffi::av_gettime_relative();
        subsystem.canvas.present();

        screen_cap(subsystem, &mut record_tx, i);
        std::thread::yield_now();
    }
    drop(tx);
    drop(record_tx);
    // ffi::av_free(dest_frame.opaque);
}

fn record_frame(
    frame: &mut ffi::AVFrame,
    tx: &Option<std::sync::mpsc::SyncSender<RecordFrameWrapper>>,
) -> Result<(), String> {
    let wrapped_frame = RecordFrameWrapper { ptr: frame as _ };
    if tx.is_some() {
        _ = tx.as_ref().unwrap().send(wrapped_frame);
    }
    Ok(())
}

fn frame_to_texture(movie_frame: &mut ffi::AVFrame, texture: &mut Texture) -> Result<(), String> {
    unsafe {
        set_sdl_yuv_conversion_mode(movie_frame);
        SDL_UpdateYUVTexture(
            texture.raw(),
            ::std::ptr::null(),
            movie_frame.data[0],
            movie_frame.linesize[0],
            movie_frame.data[1],
            movie_frame.linesize[1],
            movie_frame.data[2],
            movie_frame.linesize[2],
        );
    };
    Ok(())
}

fn texture_to_texture(
    src_texture: &mut Texture,
    canvas: &mut Canvas<Window>,
    dest_texture: &mut Texture,
) -> Result<(), String> {
    let info = src_texture.query();
    let n_units = info.width * info.height * 3 / 2;
    let mut pixels: Vec<u8> = Vec::with_capacity(n_units as _);
    let pixels = pixels.as_mut_slice();

    let offset0: isize = info.width as isize * info.height as isize;
    let uv_plane_size: isize = (info.width / 2) as isize * (info.height as isize / 2);
    let offset1: isize = offset0 + uv_plane_size;

    unsafe {
        SDL_LockTexture(
            (*src_texture).raw() as *mut _,
            std::ptr::null(),
            pixels.as_mut_ptr() as _,
            info.width as _,
        );
        dest_texture
            .update_yuv(
                None,
                std::slice::from_raw_parts(pixels.as_ptr().offset(0), offset0 as _),
                info.width as _,
                std::slice::from_raw_parts(pixels.as_ptr().offset(offset0), uv_plane_size as _),
                (info.width / 2) as _,
                std::slice::from_raw_parts(pixels.as_ptr().offset(offset1), uv_plane_size as _),
                (info.width / 2) as _,
            )
            .expect("failed to update yuv texture");
        // let debug = ::std::slice::from_raw_parts(pixels.as_ptr().offset(0), 1024);
        // dbg!(&debug);
        SDL_UnlockTexture((*src_texture).raw() as *mut _);
    };
    Ok(())
}

fn blit_texture(canvas: &mut Canvas<Window>, texture: &mut Texture) -> Result<(), String> {
    canvas.copy(texture, None, None).unwrap();
    Ok(())
}

unsafe fn set_sdl_yuv_conversion_mode(frame: *const ffi::AVFrame) {
    let mut mode = sdl2::sys::SDL_YUV_CONVERSION_MODE::SDL_YUV_CONVERSION_AUTOMATIC;
    if !frame.is_null()
        && ((*frame).format == ffi::AVPixelFormat_AV_PIX_FMT_YUV420P
            || (*frame).format == ffi::AVPixelFormat_AV_PIX_FMT_YUV422P
            || (*frame).format == ffi::AVPixelFormat_AV_PIX_FMT_UYVY422)
    {
        if (*frame).color_range == ffi::AVColorRange_AVCOL_RANGE_JPEG {
            mode = sdl2::sys::SDL_YUV_CONVERSION_MODE::SDL_YUV_CONVERSION_JPEG;
        } else if (*frame).colorspace == ffi::AVColorPrimaries_AVCOL_PRI_BT709 {
            mode = sdl2::sys::SDL_YUV_CONVERSION_MODE::SDL_YUV_CONVERSION_BT709;
        } else if (*frame).colorspace == ffi::AVColorPrimaries_AVCOL_PRI_BT470BG
            || (*frame).colorspace == ffi::AVColorPrimaries_AVCOL_PRI_SMPTE170M
        {
            mode = sdl2::sys::SDL_YUV_CONVERSION_MODE::SDL_YUV_CONVERSION_BT601;
        }
    }
    sdl2::sys::SDL_SetYUVConversionMode(mode); /* FIXME: no support for linear transfer */
}

fn fill_frame_with_memcpy(frame: &mut ffi::AVFrame, buffer: *const u8, len: usize, i: i64) {
    unsafe {
        let bfslice: &[u8] = &*slice_from_raw_parts(buffer, len);
        let frameslice: &mut [u8] = &mut *slice_from_raw_parts_mut((*frame.buf[0]).data, len);
        let cyslice: &mut [u8] = &mut *slice_from_raw_parts_mut(frame.data[2], 1024);
        frameslice.copy_from_slice(bfslice);

        // frame data is 32 bit aligned
        // sdl buffers are un-aligned (packed)
        let offset0: usize = frame.linesize[0] as usize * frame.height as usize;
        let offset1: usize = offset0 + (frame.linesize[1] as usize * (frame.height as usize / 2));
        frame.data[1] = frame.data[0].offset(offset0 as isize);
        frame.data[2] = frame.data[0].offset(offset1 as isize);
    }
}
fn fill_frame_with_buffer(frame: &mut ffi::AVFrame, buffer: *const u8, len: usize, i: i64) {
    unsafe {
        let bfslice: &[u8] = &*slice_from_raw_parts(buffer, len);
        let offset0: usize = frame.linesize[0] as usize * frame.height as usize;
        let offset1: usize = offset0 + (frame.linesize[1] as usize * (frame.height as usize / 2));

        let dslice: &mut [u8] = &mut *slice_from_raw_parts_mut((*frame.buf[0]).data as _, len);
        // for interactive debugging ...
        // let crslice: &mut [u8] = &mut *slice_from_raw_parts_mut((*frame.buf[0]).data.add(offset1), 1024);
        // let cyslice: &mut [u8] = &mut *slice_from_raw_parts_mut(frame.data[2], len / 2);
        for y in 0..frame.height as usize {
            for x in 0..frame.width as usize {
                dslice[(y * (frame.linesize[0] as usize) + x) as usize] =
                    (bfslice[(y * (frame.linesize[0] as usize) + x) as usize]) as u8;
            }
        }

        for y in 0..(frame.height / 2) as usize {
            let y1 = offset0 + (y * frame.linesize[1] as usize);
            let y2 = offset1 + (y * frame.linesize[2] as usize);
            for x in 0..(frame.width / 2) as usize {
                dslice[y1 + x as usize] = bfslice[y1 + x as usize] as u8;
                dslice[y2 + x as usize] = bfslice[y2 + x as usize] as u8;
            }
        }
    }
}

#[allow(dead_code)]
fn write_out_buffer(buffer: *const u8, len: usize, filename: &str) {
    unsafe {
        // buffer.iter().for_each(|b| println!("{:02x}", b));
        let mut file_out = std::fs::File::options()
            .write(true)
            .create(true)
            .append(true)
            .open(filename)
            .expect("cannot open output.mp4");
        let bfslice: &[u8] = &*slice_from_raw_parts(buffer, len);
        file_out.write_all(bfslice as _).unwrap();
        // file_out.write_all(buffer.into()).unwrap();
        let _ = file_out.flush();
    }
}

fn fill_frame_with_pattern(dest_frame: &mut ffi::AVFrame, i: i64) {
    let mut ybuff = vec![0u8; (dest_frame.linesize[0] as i32 * dest_frame.height) as usize];
    for y in 0..dest_frame.height as usize {
        for x in 0..dest_frame.width as usize {
            ybuff[(y * (dest_frame.linesize[0] as usize) + x) as usize] =
                ((x + y + i as usize) * 3) as u8;
        }
    }
    // dest_frame.data[0] = ::std::ptr::addr_of_mut!(ybuff) as _;
    dest_frame.data[0] = (ybuff).as_mut_ptr() as *mut _;

    /* Cb and Cr */
    let mut cbbuff = vec![0; (dest_frame.linesize[1] as i32 * dest_frame.height / 2) as usize];
    let mut crbuff = vec![0; (dest_frame.linesize[2] as i32 * dest_frame.height / 2) as usize];
    for y in 0..(dest_frame.height / 2) as usize {
        for x in 0..(dest_frame.width / 2) as usize {
            cbbuff[(y * dest_frame.linesize[1] as usize + x) as usize] =
                ((128 + y + i as usize) * 2) as u8;
            crbuff[(y * dest_frame.linesize[2] as usize + x) as usize] = (64 + x) as u8;
        }
    }
    dest_frame.data[1] = cbbuff.as_mut_ptr() as *mut _;
    dest_frame.data[2] = ::std::ptr::addr_of_mut!(*crbuff) as *mut _;
    dest_frame.colorspace = ffi::AVColorSpace_AVCOL_SPC_BT709;
}

unsafe fn texture_cap(
    subsystem: &mut SdlSubsystemCtx,
    record_tx: &mut Option<std::sync::mpsc::SyncSender<RecordFrameWrapper>>,
    i: i64,
    event_pump: &sdl2::EventPump,
) {
    if !subsystem.is_recording {
        return;
    }
    // let mut size = SDL_Point { x: 0, y: 0 };
    // let mut format:u32 = 0;
    // let rect = sdl2::sys::SDL_QueryTexture(
    //     src_texture.raw(),
    //     // sdl2::sys::SDL_PixelFormatEnum::SDL_PIXELFORMAT_IYUV as _,
    //     &mut format as _,
    //     std::ptr::null_mut(),
    //     &mut size.x as _,
    //     &mut size.y as _,
    // );
    // dest_frame.width = size.x;
    // dest_frame.height = size.y;
}

unsafe fn screen_cap(
    subsystem: &mut SdlSubsystemCtx,
    record_tx: &mut Option<std::sync::mpsc::SyncSender<RecordFrameWrapper>>,
    i: i64,
) {
    if !subsystem.is_recording {
        return;
    }

    let screen_size = subsystem.canvas.window().size();
    let dest_frame = ffi::av_frame_alloc()
        .as_mut()
        .expect("failed to allocated memory for AVFrame");

    dest_frame.width = screen_size.0 as _;
    dest_frame.height = screen_size.1 as _;
    dest_frame.format = ffi::AVPixelFormat_AV_PIX_FMT_YUV420P;
    dest_frame.time_base = ffi::AVRational { num: 1, den: 25 };
    let ret = ffi::av_frame_get_buffer(dest_frame, 0);

    dest_frame.pts = i;

    let n_units = dest_frame.width * dest_frame.height * 3 / 2;
    let mut aligned: Vec<u8> = Vec::with_capacity(n_units as _);
    let aligned = aligned.as_mut_slice();

    // let pitch = (1280 * sdl2::pixels::SDL_BYTESPERPIXEL(ffi::AVPixelFormat_AV_PIX_FMT_YUV420P) as _);

    let ret = sdl2::sys::SDL_RenderReadPixels(
        subsystem.canvas.raw() as *mut _,
        std::ptr::null(),
        // 0,
        sdl2::sys::SDL_PixelFormatEnum::SDL_PIXELFORMAT_IYUV as _,
        // (dest_frame.buf[0].as_mut().unwrap().buffer) as _,
        aligned.as_mut_ptr() as *mut _,
        // dest_frame.data[0] as *mut _,
        // pitch as _,
        dest_frame.width,
        // 1280 *  3 / 2,
    );

    // fill_frame_with_pattern(dest_frame, i);
    fill_frame_with_memcpy(dest_frame, aligned.as_ptr(), n_units as usize, i);
    // write_out_buffer(dest_frame.data[0], n_units as _, "dest_frame.yuv");
    // write_out_buffer(aligned.as_ptr(), n_units as _, "dest_frame.yuv");

    let _ = record_frame(dest_frame, &record_tx);
    // ::std::thread::sleep(Duration::from_secs_f64(0.15));
    // av_frame_unref(dest_frame as *mut _);
}

unsafe fn draw_ui(renderer: &mut Canvas<Window>, tex2: &mut Texture, is_recording: bool) {
    sdl2::sys::SDL_SetRenderTarget(renderer.raw(), tex2.raw());
    sdl2::sys::SDL_SetRenderDrawBlendMode(
        renderer.raw(),
        sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_NONE,
    );
    sdl2::sys::SDL_SetRenderDrawColor(renderer.raw(), 0, 0, 0, 0);
    sdl2::sys::SDL_SetTextureBlendMode(tex2.raw(), sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_NONE);
    sdl2::sys::SDL_RenderClear(renderer.raw());
    sdl2::sys::SDL_SetTextureBlendMode(tex2.raw(), sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_BLEND);


    // sdl2::sys::SDL_RenderClear(renderer.raw());
    if !is_recording {
        sdl2::sys::SDL_SetRenderTarget(renderer.raw(), std::ptr::null_mut());
        return;
    }

    let dest_rect = sdl2::sys::SDL_Rect {
        x: 10,
        y: 10,
        w: 40,
        h: 40,
    };

    sdl2::sys::SDL_RenderClear(renderer.raw());
    sdl2::sys::SDL_SetTextureBlendMode(tex2.raw(), sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_BLEND);
    sdl2::sys::SDL_SetTextureAlphaMod(tex2.raw(), 170 as u8);
    sdl2::sys::SDL_SetRenderDrawColor(renderer.raw(), 202, 22, 22, 255);
    sdl2::sys::SDL_RenderFillRect(renderer.raw(), std::ptr::addr_of!(dest_rect));

    sdl2::sys::SDL_RenderCopy(
        renderer.raw(),
        tex2.raw(),
        std::ptr::null(),
        std::ptr::null(),
    );
    sdl2::sys::SDL_SetRenderTarget(renderer.raw(), std::ptr::null_mut());
}

unsafe fn composite(renderer: &mut Canvas<Window>, tex: &mut Texture, tex2: &mut Texture) {
    sdl2::sys::SDL_SetRenderTarget(renderer.raw(), tex.raw());
    let dest_rect = sdl2::sys::SDL_Rect {
        x: 30,
        y: 30,
        w: 60,
        h: 60,
    };
    sdl2::sys::SDL_SetRenderDrawBlendMode(
        renderer.raw(),
        sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_BLEND,
    );
    // sdl2::sys::SDL_SetTextureBlendMode(tex.raw(), sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_BLEND);
    // sdl2::sys::SDL_SetTextureAlphaMod(tex.raw(), 25 as u8);
    // sdl2::sys::SDL_SetTextureAlphaMod(tex.raw(), 70 as u8);
    // sdl2::sys::SDL_RenderCopy(renderer.raw(), tex2.raw(), std::ptr::null(), &dest_rect);
    sdl2::sys::SDL_RenderCopy(renderer.raw(), tex2.raw(), std::ptr::null(), std::ptr::null());

    sdl2::sys::SDL_SetRenderTarget(renderer.raw(), std::ptr::null_mut());

    // sdl2::sys::SDL_SetTextureAlphaMod(tex.raw(), 255 as u8);
    // sdl2::sys::SDL_SetTextureBlendMode(tex.raw(), sdl2::sys::SDL_BlendMode::SDL_BLENDMODE_NONE);
}
