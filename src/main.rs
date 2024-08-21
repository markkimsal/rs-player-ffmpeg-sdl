#![allow(unused_mut)]
use std::time::Duration;

use ::log::logger;
use log::{debug, info};
use ::rsplayer::app::start_analyzer;
#[allow(unused_imports)]
use rsplayer::{analyzer_state::AnalyzerContext, app::{open_movie, play_movie}};
use rusty_ffmpeg::ffi;

fn main() {

    let mut clog = colog::default_builder();
    clog.filter(None, log::LevelFilter::Info);
    clog.init();

    unsafe {
        dump_video_codecs();
    }
    let args: Vec<String> = std::env::args().collect();
    // you can't pass cli arguments to debug with rust-analyzer
    let default_file = String::from("test_vid.mp4");
    // let mut video_state = MovieState::new();
    let mut analyzer_ctx = AnalyzerContext::new();
    unsafe {
        let filepath: std::ffi::CString = std::ffi::CString::new(args.get(1).unwrap_or(&default_file).as_str()).unwrap();
        open_movie(&mut analyzer_ctx, filepath.as_ptr());
        open_movie(&mut analyzer_ctx, filepath.as_ptr());
    }
    analyzer_ctx.set_loop(false);

    unsafe {
        let tx = start_analyzer(&mut analyzer_ctx);
        // let tx = play_movie(analyzer_ctx.movie_list.get_mut(0).unwrap());
        let mut keep_playing = true;
        while keep_playing == true {

            let _ = tx.send("pause".to_owned());
            // analyzer_ctx.pause();
            play_movies(&mut analyzer_ctx);
            let _ = tx.send("unpause".to_owned());
            let _ = tx.send("quit".to_owned());
            keep_playing = false;
        }
        drop(tx);
        ::std::thread::sleep(Duration::from_secs(1));
        drop(analyzer_ctx);
    }
}

unsafe fn dump_video_codecs() {
    let i: *mut u64 = ::std::ptr::null_mut();
    let iptr: *mut *mut ::std::ffi::c_void =  &mut (i as *mut ::std::ffi::c_void);
    let mut codec = ffi::av_codec_iterate(iptr);
    debug!("Video Codecs:");
    while !codec.is_null() {
        let name = std::ffi::CStr::from_ptr((*codec).name).to_str().unwrap();
        if !(*codec).pix_fmts.is_null() {
            let long_name = match (*codec).long_name.is_null() {
                false => std::ffi::CStr::from_ptr((*codec).long_name).to_str().unwrap(),
                _ => "",
            };
            debug!("V: {:<12} - {}", name, long_name);
        }
        codec = ffi::av_codec_iterate(iptr as *mut *mut std::ffi::c_void);
    }
}

unsafe fn play_movies(analyzer_ctx: &mut AnalyzerContext) {
    // let refresh_rate = 1./60.;
    let mut nearest_frame = -0.;
    'all_frames: loop {
    for index in 0..analyzer_ctx.movie_count() {
        if let (remaining, Some(mut dest_frame)) = analyzer_ctx.dequeue_frame(index as _) {
            if nearest_frame < remaining {
                nearest_frame = remaining;
            }
            info!("got frame {} for movie index {}", (*dest_frame).coded_picture_number, index);
            ffi::av_frame_unref(dest_frame as *mut _);
            ffi::av_frame_free(&mut dest_frame as *mut *mut _);
            ::std::thread::yield_now();
            // ::std::thread::sleep(Duration::from_secs_f64(refresh_rate*1.5));
            // ::std::thread::yield_now();
            analyzer_ctx.step();
        } else {
            break 'all_frames;
        }
    };
    }
}
