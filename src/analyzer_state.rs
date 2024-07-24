#![allow(unused_variables, dead_code, unused)]
use log::debug;
use rusty_ffmpeg::ffi;

use crate::movie_state::{self, FrameWrapper, MovieState};

pub struct AnalyzerContext {
    pub movie_list: Vec<MovieState>,
    pub paused: std::sync::atomic::AtomicBool,
}

impl AnalyzerContext {
    pub fn new() -> AnalyzerContext {
        AnalyzerContext {
            movie_list: vec![],
            paused: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl AnalyzerContext {
    pub fn add_movie_state(&mut self, movie: MovieState) {
        self.movie_list.push(movie);
    }

    pub fn dequeue_frame(&mut self) -> Option<*mut ffi::AVFrame> {
        if self.movie_list.len() == 0 {
            return None;
        }
        if let None = self.peek_movie_state_packet() {
            return None;
        }
        let dest_frame = unsafe {
            ffi::av_frame_alloc()
            .as_mut()
            .expect("failed to allocated memory for AVFrame")
        };

        let movie_state = self.movie_list.get(0).unwrap();
        unsafe {
        if let Some(frame) = self.movie_list[0].dequeue_frame() {
                
            let mut in_vfilter = movie_state.in_vfilter.lock().unwrap();
            let mut out_vfilter = movie_state.out_vfilter.lock().unwrap();
            let mut vgraph = movie_state.vgraph.lock().unwrap();

            if in_vfilter.is_null() || out_vfilter.is_null() {
                let format_context = &movie_state.format_context;
                let rotation = 0;
                // let rotation = get_orientation_metadata_value(
                //     (*format_context).lock().unwrap().ptr,
                // );
                crate::filter::init_filter(
                    rotation,
                    &mut vgraph.ptr,
                    &mut out_vfilter.ptr,
                    &mut in_vfilter.ptr,
                    (
                        frame.ptr.as_ref().unwrap().width,
                        frame.ptr.as_ref().unwrap().height,
                    ),
                    frame.ptr.as_ref().unwrap().format,
                );
            }
            ffi::av_buffersrc_add_frame(in_vfilter.ptr, frame.ptr);
            ffi::av_buffersink_get_frame_flags(out_vfilter.ptr, dest_frame, 0);
            return Some(dest_frame);
        }
        }
        None

    }

    fn peek_movie_state_packet(&mut self) -> Option<i64> {
        let movie_state = self.movie_list.get(0).unwrap();
        if let Some(pts) = movie_state.peek_frame_pts() {
            // if last_pts == ffi::AV_NOPTS_VALUE {
            //     let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
            //     last_pts = pts * time_base.num as i64 / time_base.den as i64;
            // }
            let frame_rate = movie_state.video_frame_rate;

            // let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
            let mut delay: f64 = (frame_rate.num as f64) / (frame_rate.den as f64);

            if pts > movie_state.last_pts && movie_state.last_pts != ffi::AV_NOPTS_VALUE {
                // println!("av_gettime_relative: {}", (ffi::av_gettime_relative() - last_clock ) );
                // let mut delay:f64 = ( pts - last_pts ) as f64;
                // let mut delay:f64 = ( pts ) as f64;
                delay *= (pts - movie_state.last_pts) as f64;
                debug!("pts: {}", delay);
                {
                    // delay *= (time_base.num as f64) / (time_base.den as f64);
                }
                // debug!("delay 1: {}", delay);
                // unsafe {
                // delay -= (ffi::av_gettime_relative() - last_clock) as f64 / 100_000.;
                // }
            }
            // println!("av_gettime_relative: {}", (ffi::av_gettime_relative() - last_clock ) as f64 / 100_000. );
            if delay > 0. {
                debug!("pts: {}", delay);

                // println!("delay 2: {}", delay );
                ::std::thread::sleep(std::time::Duration::from_secs_f64(1. / delay));
            }
            return Some(pts);
        }
        None
    }

    pub fn pause(&self) {
        let state = self.paused.load(std::sync::atomic::Ordering::Relaxed);
        self.paused.store(!state, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool{
        self.paused.load(std::sync::atomic::Ordering::Relaxed)
    }
}