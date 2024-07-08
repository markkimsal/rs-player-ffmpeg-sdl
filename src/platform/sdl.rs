#![allow(unused_variables, dead_code, unused_imports)]
use std::{io::Write, ops::Deref, ptr::{slice_from_raw_parts, slice_from_raw_parts_mut}, sync::mpsc::{Sender, SyncSender}, thread::JoinHandle, time::Duration};

use log::debug;
use rusty_ffmpeg::ffi::{self, av_frame_unref};

use sdl2::{
    event::Event,
    Error,
    keyboard::Keycode,
    pixels::{Color, PixelFormatEnum},
    render::{Canvas, Texture, TextureAccess},
    sys::SDL_UpdateYUVTexture,
    video::Window,
    Sdl
};

use crate::{movie_state::{self, FormatContextWrapper, FrameWrapper}, record_state::{FrameWrapper as RecordFrameWrapper, RecordState}};

// static CANVAS: Option<Canvas<Window>> = None;
pub struct SdlSubsystemCtx {
    sdl_ctx: Sdl,
    canvas: Canvas<Window>,
    is_recording: bool,
}

struct AlignedBytes([u8; 3]);
pub fn init_subsystem<'sdl>(default_width: u32, default_height: u32) ->Result<SdlSubsystemCtx, Error> {
    let sdl_ctx = sdl2::init().unwrap();
    let video_subsystem = match sdl_ctx.video() {
        Ok(video_subsystem) => video_subsystem,
        Err(err) => {
            eprintln!("Error: {}", err);
            return Err(Error::UnsupportedError);
        }
    };

    let window = video_subsystem.window("rs-player-ffmpeg-sdl2", default_width, default_height)
        .resizable()
        .position_centered()
        .set_window_flags(sdl2::sys::SDL_SWSURFACE)
        .build()
        .unwrap();
    let canvas = window
        .into_canvas()
        .software()
        .build()
        .unwrap();

    Ok(SdlSubsystemCtx{
        sdl_ctx,
        canvas,
        is_recording: false
    })
}

pub unsafe fn event_loop(movie_state: std::sync::Arc<&mut movie_state::MovieState>, subsystem: &mut SdlSubsystemCtx, tx: std::sync::mpsc::Sender<String>) {

    subsystem.canvas.set_draw_color(Color::RGB(0, 255, 255));
    subsystem.canvas.clear();
    subsystem.canvas.present();
    let texture_creator = subsystem.canvas.texture_creator();
 
    let textw: u32;
    let texth: u32;
    {
        let codec_context = unsafe {movie_state.video_ctx.lock().unwrap().ptr.as_ref().unwrap()};
        textw = codec_context.width as u32;
        texth = codec_context.height as u32;
    }
    // let mut lock = unsafe{movie_state.video_ctx.try_lock()};
    // if let (textw: u32, texth: u32) = unsafe{movie_state.video_ctx.lock().unwrap().ptr.as_ref().unwrap()} {
    //     (codec_context.width as u32, codec_context.height as u32)
    // }
    let mut texture: Texture = texture_creator.create_texture(
        Some(PixelFormatEnum::IYUV),
        TextureAccess::Streaming,
        textw,
        texth
    ).unwrap();
    // let mut texture = subsystem.canvas.su;
 
    // let mut texture: SDL_SW_YUVTexture = sdl2::sys::SDL_SW_CreateYUVTexture(PixelFormatEnum::IYUV, textw, texth).unwrap();
    let _ = sdl2::video::drivers()
        .map(|d: &'static str| {eprintln!("driver {}", d);});
    dbg!(sdl2::video::drivers().len());
    let mut iter = sdl2::video::drivers();
    eprintln!("iter: {:?}", iter.next());
    eprintln!("iter: {:?}", iter.next());
    eprintln!("iter: {:?}", iter.next());
    eprintln!("iter: {:?}", iter.next());
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

    let fmt_ctx = movie_state.format_context.lock().unwrap().ptr;
    let frame_rate: ffi::AVRational = ffi::av_guess_frame_rate(
        fmt_ctx,
        movie_state.video_stream.lock().unwrap().ptr,
        ::std::ptr::null_mut(),
    );
    let dest_frame =
        ffi::av_frame_alloc().as_mut()
        .expect("failed to allocated memory for AVFrame");

    'running: loop {
        // i = (i + 1) % 255;
        i = i + 1;
        for event in event_pump.poll_iter() {
            // println!("event: {:?}", event);
            match event {
                Event::Quit {..} |
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    // keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
                    break 'running;
                },
                Event::KeyDown { keycode: Some(Keycode::Space), .. } => {
                    tx.send("pause".to_string()).unwrap();
                    movie_state.pause();
                    // pause_packets.store(!pause_packets.load(std::sync::atomic::Ordering::Relaxed), std::sync::atomic::Ordering::Relaxed);
                    // packet_thread.thread().unpark();
                },
                Event::KeyDown { keycode: Some(Keycode::R), .. } => {
                    match subsystem.is_recording {
                        true => {
                            tx.send("Stop recording".to_string()).unwrap();
                            record_tx = None;
                        },
                        false => {
                            tx.send("Start recording".to_string()).unwrap();
                            record_tx = the_record_state.start_recording_thread();
                        }
                    }
                    subsystem.is_recording = !subsystem.is_recording;
                },
                _ => {}
            }
        }
        // The rest of the game loop goes here...

        if movie_state.is_paused() == true {
            std::thread::yield_now();
            screen_cap(subsystem, &mut record_tx, i, &event_pump);
            continue;
        }

        unsafe {
            if let Some(pts) = movie_state.peek_frame_pts() {
                // if last_pts == ffi::AV_NOPTS_VALUE {
                //     let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
                //     last_pts = pts * time_base.num as i64 / time_base.den as i64;
                // }

                // let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
                let mut delay:f64 = (frame_rate.num as f64) / (frame_rate.den as f64);

                if pts > last_pts && last_pts != ffi::AV_NOPTS_VALUE {
                    // println!("av_gettime_relative: {}", (ffi::av_gettime_relative() - last_clock ) );
                    // let mut delay:f64 = ( pts - last_pts ) as f64;
                    // let mut delay:f64 = ( pts ) as f64;
                    delay *= ( pts - last_pts ) as f64;
                    debug!("pts: {}", delay );
                    {
                    // delay *= (time_base.num as f64) / (time_base.den as f64);
                    }
                    debug!("delay 1: {}", delay );
                    delay -= (ffi::av_gettime_relative() - last_clock) as f64 / 100_000.;
                }
                    // println!("av_gettime_relative: {}", (ffi::av_gettime_relative() - last_clock ) as f64 / 100_000. );
                if delay > 0. {
                    debug!("pts: {}", delay );

                    // println!("delay 2: {}", delay );
                    ::std::thread::sleep(Duration::from_secs_f64(1. / delay));
                }
            }

            if let Some(frame) = movie_state.dequeue_frame() {
                let mut in_vfilter = movie_state.in_vfilter.lock().unwrap();
                let mut out_vfilter = movie_state.out_vfilter.lock().unwrap();
                let mut vgraph = movie_state.vgraph.lock().unwrap();
           
                if in_vfilter.is_null() || out_vfilter.is_null() {
                    let format_context = &movie_state.format_context;
                    let rotation = super::get_orientation_metadata_value((*format_context).lock().unwrap().ptr);
                    crate::filter::init_filter(
                        rotation,
                        &mut vgraph.ptr,
                        &mut out_vfilter.ptr,
                        &mut in_vfilter.ptr,
                        (frame.ptr.as_ref().unwrap().width, frame.ptr.as_ref().unwrap().height),
                        frame.ptr.as_ref().unwrap().format,
                    );
                }
                ffi::av_buffersrc_add_frame(in_vfilter.ptr, frame.ptr);
                ffi::av_buffersink_get_frame_flags(out_vfilter.ptr, dest_frame, 0);


                blit_frame(
                    dest_frame,
                    &mut subsystem.canvas,
                    &mut texture,
                ).unwrap_or_default();


                // let codec_context = unsafe{codec_context.as_ref().unwrap()};
                // last_pts = ffi::av_rescale_q(frame.ptr.as_ref().unwrap().pts, time_base, ffi::AVRational { num: 1, den: 1 });
                last_pts = frame.ptr.as_ref().unwrap().best_effort_timestamp;
                ffi::av_frame_unref(dest_frame as *mut _);
            };
        }
        last_clock = ffi::av_gettime_relative();
        subsystem.canvas.present();

        screen_cap(subsystem, &mut record_tx, i, &event_pump);
        std::thread::yield_now();
    }
    ffi::av_free(dest_frame.opaque);
}

fn record_frame(
    frame: &mut ffi::AVFrame,
    tx: &Option<std::sync::mpsc::SyncSender<RecordFrameWrapper>>,
) -> Result<(), String> {

    let wrapped_frame = RecordFrameWrapper { ptr: frame as _ };
    if tx.is_some()  {
        _ = tx.as_ref().unwrap().send(wrapped_frame);
    }
    Ok(())
}

fn blit_frame(
    dest_frame: &mut ffi::AVFrame,
    canvas: &mut Canvas<Window>,
    texture: &mut Texture,
) -> Result<(), String> {

    // let new_frame = dest_frame;
    // unsafe { SDL_UpdateTexture(
    //     texture.raw(), std::ptr::null(),
    //     (*dest_frame).data[0] as _, (*dest_frame).linesize[0] as _
    // ) };
    // unsafe { SDL_UpdateTexture(
    //     texture.raw(), std::ptr::null(),
    //     (new_frame).data[0] as _, (new_frame).linesize[0] as _
    // ) };

    // SDL cannot handle YUV(J)420P
    unsafe { SDL_UpdateYUVTexture(
        texture.raw(), ::std::ptr::null(),
        dest_frame.data[0], dest_frame.linesize[0],
        dest_frame.data[1], dest_frame.linesize[1],
        dest_frame.data[2], dest_frame.linesize[2],
    ) };
    canvas.copy(texture, None, None).unwrap();
    Ok(())
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
        for y in 0 .. frame.height as usize {
            for x in 0 .. frame.width as usize {
                dslice[(y * (frame.linesize[0] as usize) + x) as usize] = (bfslice[(y * (frame.linesize[0] as usize) + x) as usize]) as u8;
            }
        }
 
        for y in 0 .. (frame.height/2) as usize {
            let y1 = offset0 + (y * frame.linesize[1] as usize);
            let y2 = offset1 + (y * frame.linesize[2] as usize);
            for x in 0 .. (frame.width/2) as usize {
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
        for y in 0 .. dest_frame.height as usize {
            for x in 0 .. dest_frame.width as usize {
                ybuff[(y * (dest_frame.linesize[0] as usize) + x) as usize] = ((x + y + i as usize) * 3) as u8;
            }
        }
        // dest_frame.data[0] = ::std::ptr::addr_of_mut!(ybuff) as _;
        dest_frame.data[0] = (ybuff).as_mut_ptr() as *mut _;
 
        /* Cb and Cr */
        let mut cbbuff = vec![0; (dest_frame.linesize[1] as i32 * dest_frame.height/2) as usize];
        let mut crbuff = vec![0; (dest_frame.linesize[2] as i32 * dest_frame.height/2) as usize];
        for y in 0 .. (dest_frame.height/2) as usize {
            for x in 0 .. (dest_frame.width/2) as usize {
                cbbuff[(y * dest_frame.linesize[1] as usize + x) as usize] = ((128 + y + i as usize) * 2) as u8;
                crbuff[(y * dest_frame.linesize[2] as usize + x) as usize] = (64 + x) as u8;
            }
        }
        dest_frame.data[1] = cbbuff.as_mut_ptr() as *mut _;
        dest_frame.data[2] = ::std::ptr::addr_of_mut!(*crbuff) as *mut _;
        dest_frame.colorspace = ffi::AVColorSpace_AVCOL_SPC_BT709;
    }

unsafe fn screen_cap(subsystem: &mut SdlSubsystemCtx, record_tx: &mut Option<std::sync::mpsc::SyncSender<RecordFrameWrapper>>, i: i64, event_pump: &sdl2::EventPump) {
    if subsystem.is_recording {
    // let surface = subsystem.canvas.window().surface(&event_pump).unwrap();
    // sdl2::sys::SDL_LockSurface(surface.raw());
    // let mut pixels: *mut std::os::raw::c_void = std::ptr::null_mut();
    // let pixel_ptr: *mut *mut i32 = &mut pixels;
    // let pixel_ptr: *mut *mut std::os::raw::c_void = &mut pixels;
    // let pixels = std::ptr::null_mut();
    let dest_frame =
        ffi::av_frame_alloc().as_mut()
        .expect("failed to allocated memory for AVFrame");

    dest_frame.width = 1280;
    dest_frame.height = 720;
    dest_frame.format = ffi::AVPixelFormat_AV_PIX_FMT_YUV420P;
    dest_frame.time_base = ffi::AVRational { num: 1, den: 25 };
    // let ret = ffi::av_image_alloc(&mut dest_frame.data as *mut _, &mut dest_frame.linesize as *mut _, dest_frame.width, dest_frame.height, dest_frame.format, 16);
    // let ret = ffi::av_image_alloc(&mut dest_frame.data[1], &mut dest_frame.linesize[1], dest_frame.width, dest_frame.height, dest_frame.format, 16);
    // let ret = ffi::av_image_alloc(&mut dest_frame.data[2], &mut dest_frame.linesize[2], dest_frame.width, dest_frame.height, dest_frame.format, 16);
    let ret = ffi::av_frame_get_buffer(dest_frame, 0);

    dest_frame.pts = i;

    // we don't need alignment
    // let n_units = (*dest_frame.buf[0]).size;
    let n_units = dest_frame.width * dest_frame.height * 3 / 2;

    // let mut aligned: Vec<AlignedBytes> = Vec::with_capacity(n_units as _);
    let mut aligned: Vec<u8> = Vec::with_capacity(n_units as _);
    let aligned = aligned.as_mut_slice();

    // let srf = sdl2::sys::SDL_CreateRGBSurface(0, 1280, 720, 24, 0x00ff0000, 0x0000ff00, 0x000000ff, 0xff000000);
    // let srf = subsystem.canvas.window().surface(event_pump).unwrap();
    // let mut pitch: usize = srf.pixel_format().raw().as_mut().unwrap().BytesPerPixel as usize;
    // pitch *= srf.width() as usize;
    // dbg!(pitch);
    // let pitch = (1280 * sdl2::pixels::SDL_BYTESPERPIXEL(ffi::AVPixelFormat_AV_PIX_FMT_YUV420P) as _);

    let ret = sdl2::sys::SDL_RenderReadPixels(
        subsystem.canvas.raw() as *mut _,
        std::ptr::null(),
        // 0,
        sdl2::sys::SDL_PixelFormatEnum::SDL_PIXELFORMAT_IYUV as _,
        // (dest_frame.buf[0].as_mut().unwrap().buffer) as _,
        aligned.as_mut_ptr() as *mut _,
        // srf.as_mut().unwrap().pixels as *mut _,
        // srf.as_mut().unwrap().pitch as _,
        // dest_frame.data[0] as *mut _,
        // pitch as _,
        dest_frame.width,
        // subsystem.canvas.window().surface(&event_pump).unwrap().pitch() as _,
        // 1280 *  3 / 2,
    );

    // fill_frame_with_pattern(dest_frame, i);
    fill_frame_with_memcpy(dest_frame, aligned.as_ptr(), n_units as usize, i);
    // write_out_buffer(dest_frame.data[0], n_units as _, "dest_frame.yuv");
    // write_out_buffer(aligned.as_ptr(), n_units as _, "dest_frame.yuv");
    // dest_frame.data[0] = ::std::slice::from_raw_parts_mut(aligned.as_mut_ptr() as *mut u8, 1280 * 720 * 3 / 2);
    // dest_frame.data[0] = aligned.as_mut_ptr() as *mut _;
    // dest_frame.data[0] = (aligned).as_mut_ptr() as *mut _;
    // dest_frame.data[0] = ::std::ptr::addr_of_mut!(aligned) as *mut u8;


    // let dfbuf = dest_frame.buf[0].as_mut().unwrap();
    // dest_frame.data[0] = ::std::ptr::addr_of!(aligned) as *mut u8;
    // dest_frame.buf[0] = ::std::ptr::addr_of!(aligned) as *mut _;
    // let x = aligned[0..1280];
    // dest_frame.data[0] = ::std::slice::from_raw_parts(
    //     ::std::ptr::addr_of_mut!(aligned) as *mut u8,
    //     1280
    // ) as *mut _;

    // let buf = dest_frame.buf[0].as_mut().unwrap();
    // buf.buffer = ::std::ptr::addr_of_mut!(*pixels) as _;
    //  = (::std::ptr::addr_of_mut!(pixels) as *mut _);
    // dest_frame.data[1] = yuvdata.as_ref().unwrap()[1];
    // dest_frame.data[2] = yuvdata.as_ref().unwrap()[2];
    let _ = record_frame(dest_frame, &record_tx);
    // ::std::thread::sleep(Duration::from_secs_f64(0.15));
    // av_frame_unref(dest_frame as *mut _);
    }
}
