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
            paused: std::sync::atomic::AtomicBool::new(true),
        }
    }
}

impl AnalyzerContext {
    pub fn add_movie_state(&mut self, movie: MovieState) {
        movie.pause();
        self.movie_list.push(movie);
    }

    pub fn movie_count(&self) -> u8 {
        self.movie_list.len() as u8
    }

    pub fn dequeue_frame(&mut self) -> Option<*mut ffi::AVFrame> {
        if self.movie_list.len() == 0 {
            return None;
        }
        if let Some(pts) = self.peek_movie_state_packet() {
            if pts != 0 && self.is_paused() {
                return None;
            }
        } else {
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
        let movie_state = self.movie_list.get_mut(0).unwrap();
        if movie_state.step {
            movie_state.step = false;
            return Some(0);
        }
        if let Some(pts) = movie_state.peek_frame_pts() {
            // if last_pts == ffi::AV_NOPTS_VALUE {
            //     let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
            //     last_pts = pts * time_base.num as i64 / time_base.den as i64;
            // }
            let frame_rate = movie_state.video_frame_rate;

            // let mut delay: f64 = (frame_rate.num as f64) / (frame_rate.den as f64);
            let mut delay: f64 = 0.;
            unsafe {
            let mut current_clock = unsafe {ffi::av_gettime_relative()};
            let mut last_clock = movie_state.last_pts_time;
            if movie_state.last_pts == ffi::AV_NOPTS_VALUE {
                last_clock = 0;
            }
            let delta = current_clock - last_clock;


            let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
            // debug!("last_clock: {}  delta: {}", last_clock, delta );
            // debug!("       pts: {}", pts * (time_base.den as i64 / time_base.num as i64) );
            // debug!("       pts: {}", pts );

            if pts > movie_state.last_pts && movie_state.last_pts != ffi::AV_NOPTS_VALUE {
                // println!("av_gettime_relative: {}", (ffi::av_gettime_relative() - last_clock ) );
                // let mut delay:f64 = ( pts - last_pts ) as f64;
                // let mut delay:f64 = ( pts ) as f64;
                delay = (pts - movie_state.last_pts) as f64;
                // delay = ffi::av_rescale_q(abs(delay), bq, cq)
                delay *= time_base.num as f64 / time_base.den as f64;
                delay -=  delta as f64 / 1_000_000.;
            }
            }
            if delay > 0. {
                // return None;
                // TODO: check other movie_states to see if any other frame is ready for display
                debug!("delay: {}", delay);
                ::std::thread::sleep(std::time::Duration::from_secs_f64(delay));
            }

            movie_state.last_pts_time = unsafe { ffi::av_gettime_relative() };
            movie_state.last_pts      = pts;
            return Some(pts);
        }
        None
    }

    pub fn step(&mut self) {
        if ! self.paused.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        self.movie_list.get_mut(0).unwrap().step_force();
    }

    pub fn pause(&self) {
        let state = self.paused.load(std::sync::atomic::Ordering::Relaxed);
        self.paused.store(!state, std::sync::atomic::Ordering::Relaxed);

        self.movie_list.iter().for_each(|movie| {
            movie.pause();
        });
    }

    pub fn is_paused(&self) -> bool{
        self.paused.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn close(&mut self) {
        while let Some(movie) = self.movie_list.pop() {
            eprint!("dropping movie \n");
            drop(movie);
        }
    }
}
