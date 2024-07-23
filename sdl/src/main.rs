#![allow(unused_variables, dead_code, unused_imports)]
use std::{io::Write, ops::Deref, ptr::{slice_from_raw_parts, slice_from_raw_parts_mut}, sync::mpsc::{Sender, SyncSender}, thread::JoinHandle, time::Duration};

use log::{
    info,
    debug
};
use rusty_ffmpeg::ffi::{self, av_frame_unref};

use sdl2::{
    event::Event, keyboard::Keycode, pixels::{Color, PixelFormatEnum}, render::{Canvas, Texture, TextureAccess},
    sys::{
        SDL_LockTexture,
        SDL_UnlockTexture,
        SDL_UpdateYUVTexture
    },
    video::Window,
    Error,
    Sdl
};

use rsplayer::{analyzer_state::AnalyzerContext, app::{open_movie, play_movie}, movie_state::{self, FormatContextWrapper, FrameWrapper, MovieState}, record_state::{FrameWrapper as RecordFrameWrapper, RecordState}};

// static CANVAS: Option<Canvas<Window>> = None;
pub struct SdlSubsystemCtx {
    sdl_ctx: Sdl,
    canvas: Canvas<Window>,
    is_recording: bool,
}

struct AlignedBytes([u8; 3]);
pub unsafe fn init_subsystem<'sdl>(default_width: u32, default_height: u32) ->Result<SdlSubsystemCtx, Error> {
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
    let window = video_subsystem.window("rs-player-ffmpeg-sdl2", default_width, default_height)
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

    Ok(SdlSubsystemCtx{
        sdl_ctx,
        canvas,
        is_recording: false
})
}

fn main() {
    println!("Hello, world!");
    let mut clog = colog::default_builder();
    clog.filter(None, log::LevelFilter::Info);
    clog.init();

    let args: Vec<String> = std::env::args().collect();
    let default_file = String::from("foo.mp4");
    let mut analyzer_ctx = AnalyzerContext::new();
    unsafe {
        let filepath: std::ffi::CString = std::ffi::CString::new(args.get(1).unwrap_or(&default_file).as_str()).unwrap();
        open_movie(&mut analyzer_ctx, filepath.as_ptr()); //, &mut video_state);
    }
    // open_window(format_context, codec_context);
    unsafe {play_movie(&mut analyzer_ctx); }
}

