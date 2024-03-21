
use std::{time::Duration, thread::ThreadId, borrow::{Borrow, BorrowMut}, ops::DerefMut, sync::Mutex};

use libc::IFLA_NEW_IFINDEX;
use rusty_ffmpeg::ffi::{self, SwsContext, AVPixelFormat_AV_PIX_FMT_ARGB};

use sdl2::{video::{Window, WindowContext}, render::{TextureAccess, Canvas, Texture, TextureCreator}, pixels::{PixelFormatEnum, Color}, event::Event, keyboard::Keycode, Sdl, sys::{SDL_UpdateTexture, SDL_Texture, SDL_RenderCopy, SDL_blit, SDL_Renderer}};

use crate::movie_state;

// static CANVAS: Option<Canvas<Window>> = None;
pub struct SdlSubsystemCtx {
    sdl_ctx: Sdl,
    canvas: Canvas<Window>,
}

pub fn init_subsystem<'sdl>(default_width: u32, default_height: u32) ->Result<SdlSubsystemCtx, ()> {
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();

    let window = video_subsystem.window("rs-player-ffmpeg-sdl2", default_width, default_height)
        .position_centered()
        .build()
        .unwrap();
    let canvas = window
        .into_canvas()
        .build()
        .unwrap();

        let sdl_ctx = SdlSubsystemCtx{sdl_ctx: sdl_context, canvas: canvas};
        return Ok(sdl_ctx);

    // unsafe {
    // }
    // return Err(());
}

pub unsafe fn event_loop(movie_state: &movie_state::MovieState, subsystem: &mut SdlSubsystemCtx, tx: std::sync::mpsc::Sender<String>) {

    subsystem.canvas.set_draw_color(Color::RGB(0, 255, 255));
    subsystem.canvas.clear();
    subsystem.canvas.present();
    // unsafe {let mut renderer = sdl2::sys::SDL_CreateRenderer(window.raw(), -1, 0); }
    let texture_creator = subsystem.canvas.texture_creator();
    let mut texture: Texture = texture_creator.create_texture(
        Some(PixelFormatEnum::ARGB32),
        TextureAccess::Streaming,
        800,
        450
    ).unwrap();

    let mut event_pump = subsystem.sdl_ctx.event_pump().unwrap();
    let mut i = 0;
    let mut last_pts = 0;
    let mut last_clock = ffi::av_gettime_relative();
    let mut clock = ffi::av_gettime();

    'running: loop {
        i = (i + 1) % 255;
        for event in event_pump.poll_iter() {
            println!("event: {:?}", event);
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
                if pts > last_pts {
                    // println!("av_gettime_relative: {}", (ffi::av_gettime_relative() - last_clock ) );
                    let mut delay:f64 = ( pts - last_pts ) as f64;
                    // let mut delay:f64 = ( pts ) as f64;
                    println!("pts: {}", delay );
                    {
                    let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
                    delay *= (time_base.num as f64) / (time_base.den as f64);
                    }
                    // println!("delay 1: {}", delay );
                    delay -= (ffi::av_gettime_relative() - last_clock) as f64 / 100_000.;
                    // println!("av_gettime_relative: {}", (ffi::av_gettime_relative() - last_clock ) as f64 / 100_000. );
                    if delay > 0. {
                        // println!("delay 2: {}", delay );
                        ::std::thread::sleep(Duration::from_secs_f64(delay));
                    }
                }
            }
            if let Some(frame) = movie_state.dequeue_frame() {
                let dest_frame =
                    ffi::av_frame_alloc().as_mut()
                    .expect("failed to allocated memory for AVFrame");

                blit_frame(
                    frame.ptr.as_mut().unwrap(),
                    dest_frame,
                    &mut subsystem.canvas,
                    &mut texture,
                    ).unwrap_or_default();
                last_pts = frame.ptr.as_ref().unwrap().pts;
            };
        }
        last_clock = ffi::av_gettime_relative();
        subsystem.canvas.present();
        std::thread::yield_now();
    }
}
fn blit_frame(
    src_frame: &mut ffi::AVFrame,
    dest_frame: &mut ffi::AVFrame,
    canvas: &mut Canvas<Window>,
    texture: &mut Texture,
) -> Result<(), String> {

        // let  new_frame = frame_thru_filter(filter, src_frame);
        let new_frame = src_frame;

        // dest_frame.width  = new_frame.width;
        // dest_frame.height = new_frame.height;
        dest_frame.width  = canvas.window().size().0 as i32;
        dest_frame.height = canvas.window().size().1 as i32;
        dest_frame.format = AVPixelFormat_AV_PIX_FMT_ARGB;

        unsafe {
            let mut sws_ctx = ffi::sws_getCachedContext(
                ::std::ptr::null_mut(),
                1280,
                720,
                // match 0 { 90 => codec_context.height, _ => codec_context.width},
                // match 0 { 90 => codec_context.width, _ => codec_context.height},
                ffi::AVPixelFormat_AV_PIX_FMT_YUV420P,
                800,
                450,
                AVPixelFormat_AV_PIX_FMT_ARGB,
                ffi::SWS_BILINEAR as i32,
                ::std::ptr::null_mut(),
                ::std::ptr::null_mut(),
                ::std::ptr::null_mut(),
            );


            ffi::av_frame_get_buffer(dest_frame, 0);
             ffi::sws_scale(
                sws_ctx,
                new_frame.data.as_ptr() as _,
                new_frame.linesize.as_ptr(),
                0,
                new_frame.height,
                // codec_context.height,
                dest_frame.data.as_mut_ptr(),
                dest_frame.linesize.as_mut_ptr()
            )
        };

    let new_frame = dest_frame;
    // unsafe { SDL_UpdateTexture(
    //     texture.raw(), ptr::null(),
    //     (*dest_frame).data[0] as _, (*dest_frame).linesize[0] as _
    // ) };
    unsafe { SDL_UpdateTexture(
        texture.raw(), std::ptr::null(),
        (new_frame).data[0] as _, (new_frame).linesize[0] as _
    ) };

    // SDL cannot handle YUV(J)420P
    // unsafe { SDL_UpdateYUVTexture(
    //     texture.raw(), ptr::null(),
    //     dest_frame.data[0], dest_frame.linesize[0],
    //     dest_frame.data[1], dest_frame.linesize[1],
    //     dest_frame.data[2], dest_frame.linesize[2],
    // ) };
    canvas.copy(texture, None, None).unwrap();
    Ok(())
}

