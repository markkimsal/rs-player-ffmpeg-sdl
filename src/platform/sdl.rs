
use std::{borrow::Borrow, sync::mpsc::Sender, thread::JoinHandle, time::Duration};

use rusty_ffmpeg::ffi::{self};

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

use crate::{movie_state::{self, FrameWrapper}, record_state::{FrameWrapper as RecordFrameWrapper, RecordState}};

// static CANVAS: Option<Canvas<Window>> = None;
pub struct SdlSubsystemCtx {
    sdl_ctx: Sdl,
    canvas: Canvas<Window>,
    is_recording: bool,
}

pub fn init_subsystem<'sdl>(default_width: u32, default_height: u32) ->Result<SdlSubsystemCtx, Error> {
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = match sdl_context.video() {
        Ok(video_subsystem) => video_subsystem,
        Err(err) => {
            eprintln!("Error: {}", err);
            return Err(Error::UnsupportedError);
        }
    };

    let window = video_subsystem.window("rs-player-ffmpeg-sdl2", default_width, default_height)
        .resizable()
        .position_centered()
        .build()
        .unwrap();
    let canvas = window
        .into_canvas()
        .build()
        .unwrap();

        let sdl_ctx = SdlSubsystemCtx{sdl_ctx: sdl_context, canvas: canvas, is_recording: false};
        return Ok(sdl_ctx);

    // unsafe {
    // }
    // return Err(());
}

pub unsafe fn event_loop(movie_state: &mut movie_state::MovieState, subsystem: &mut SdlSubsystemCtx, tx: std::sync::mpsc::Sender<String>) {

    subsystem.canvas.set_draw_color(Color::RGB(0, 255, 255));
    subsystem.canvas.clear();
    subsystem.canvas.present();
    // unsafe {let mut renderer = sdl2::sys::SDL_CreateRenderer(window.raw(), -1, 0); }
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

    let mut event_pump = subsystem.sdl_ctx.event_pump().unwrap();
    let mut i = 0;
    let mut last_pts = 0;
    let mut last_clock = ffi::av_gettime_relative();
    let clock = ffi::av_gettime();
    let mut record_tx: Option<Sender<RecordFrameWrapper>> = None;
    let mut record_thread: JoinHandle<()>;

    let mut the_record_state = RecordState::new();

    let fmt_ctx = movie_state.format_context.lock().unwrap().ptr;
    let frame_rate: ffi::AVRational = ffi::av_guess_frame_rate(
        fmt_ctx,
        movie_state.video_stream.lock().unwrap().ptr,
        ::std::ptr::null_mut(),
    );

    'running: loop {
        i = (i + 1) % 255;
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
                            // drop(record_tx)
                            // if let Some(inner_tx) = record_tx.borrow() {
                            // }
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
                    println!("pts: {}", delay );
                    {
                    // delay *= (time_base.num as f64) / (time_base.den as f64);
                    }
                    println!("delay 1: {}", delay );
                    delay -= (ffi::av_gettime_relative() - last_clock) as f64 / 100_000.;
                }
                    // println!("av_gettime_relative: {}", (ffi::av_gettime_relative() - last_clock ) as f64 / 100_000. );
                if delay > 0. {
                    println!("pts: {}", delay );

                    // println!("delay 2: {}", delay );
                    ::std::thread::sleep(Duration::from_secs_f64(1. / delay));
                }
            }

            if let Some(frame) = movie_state.dequeue_frame() {
                let dest_frame =
                    ffi::av_frame_alloc().as_mut()
                    .expect("failed to allocated memory for AVFrame");

           
                if movie_state.in_vfilter.is_null() || movie_state.out_vfilter.is_null() {
                    let format_context = std::sync::Arc::clone(&movie_state.format_context);
                    let rotation = super::get_orientation_metadata_value((*format_context).lock().unwrap().ptr);
                    crate::filter::init_filter(
                        rotation,
                        &mut movie_state.vgraph,
                        &mut movie_state.out_vfilter,
                        &mut movie_state.in_vfilter,
                        (frame.ptr.as_ref().unwrap().width, frame.ptr.as_ref().unwrap().height),
                        frame.ptr.as_ref().unwrap().format,
                    );
                }
                ffi::av_buffersrc_add_frame(movie_state.in_vfilter, frame.ptr);
                ffi::av_buffersink_get_frame_flags(movie_state.out_vfilter, dest_frame, 0);


                blit_frame(
                    dest_frame,
                    &mut subsystem.canvas,
                    &mut texture,
                    ).unwrap_or_default();


        let _ = record_frame(dest_frame, &record_tx);
                // let codec_context = unsafe{codec_context.as_ref().unwrap()};
                // last_pts = ffi::av_rescale_q(frame.ptr.as_ref().unwrap().pts, time_base, ffi::AVRational { num: 1, den: 1 });
                last_pts = frame.ptr.as_ref().unwrap().best_effort_timestamp;
            };
        }
        last_clock = ffi::av_gettime_relative();
        subsystem.canvas.present();
        std::thread::yield_now();
    }
}

fn record_frame(
    frame: &mut ffi::AVFrame,
    tx: &Option<std::sync::mpsc::Sender<RecordFrameWrapper>>,
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

