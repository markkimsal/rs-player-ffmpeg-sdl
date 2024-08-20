#![allow(unused_variables, dead_code, unused)]
use ::std::slice::Iter;

use log::debug;
use ::log::info;
use rusty_ffmpeg::ffi;
use ::rusty_ffmpeg::ffi::{AVDurationEstimationMethod_AVFMT_DURATION_FROM_BITRATE, AV_NOPTS_VALUE};

use crate::movie_state::{self, FrameWrapper, MovieState};

#[derive(Default, Debug)]
pub struct Clock {
    pts: i64,
    pts_drift: i64,     /* clock base minus time at which we updated the clock */
    last_updated: i64,
    speed: f32,
    paused: bool,
}


pub struct AnalyzerContext {
    pub movie_list: Vec<MovieState>,
    pub paused: std::sync::atomic::AtomicBool,
    pub clock: Clock,
    pub force_render: bool,
}

impl AnalyzerContext {
    pub fn new() -> AnalyzerContext {
        AnalyzerContext {
            movie_list: vec![],
            paused: std::sync::atomic::AtomicBool::new(true),
            clock: Clock{ paused: false, pts: AV_NOPTS_VALUE, speed: 1.0, ..Default::default() },
            force_render: true,
        }
    }
}

impl AnalyzerContext {
    pub fn add_movie_state(&mut self, mut movie: MovieState) {
        movie.pause();
        self.movie_list.push(movie);
    }

    pub fn movie_count(&self) -> u8 {
        self.movie_list.len() as u8
    }

    pub fn dequeue_frame(&mut self, movie_index: u8) -> (u8, Option<*mut ffi::AVFrame>) {
        if self.movie_list.len() == 0 {
            return (0, None);
        }
        if let (index, Some(pts)) = self.peek_movie_state_packet(movie_index as _) {
            if pts != 0 && self.is_paused() {
                return (index, None);
            }
        } else {
            return (0, None)
        }
        let dest_frame = unsafe {
            ffi::av_frame_alloc()
            .as_mut()
            .expect("failed to allocated memory for AVFrame")
        };

        let movie_state: &MovieState = self.movie_list.get(movie_index as usize).unwrap();
        unsafe {
        if let Some(frame) = movie_state.dequeue_frame() {
                
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
            return (movie_index as _, Some(dest_frame));
        }
        }
        (0, None)

    }

    fn peek_movie_state_packet(&mut self, movie_index: usize) -> (u8, Option<i64>) {
        let movie_state = self.movie_list.get_mut(movie_index).unwrap();
        // for (index, movie_state) in self.movie_list.iter_mut().enumerate() {
            if movie_state.step {
                movie_state.step = false;
                return (movie_index as _, Some(0));
            }
            let mut current_clock = unsafe {ffi::av_gettime_relative()};
            'retry: loop {
            if let Some(pts) = movie_state.peek_frame_pts() {
                // if last_pts == ffi::AV_NOPTS_VALUE {
                //     let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
                //     last_pts = pts * time_base.num as i64 / time_base.den as i64;
                // }
                let frame_rate = movie_state.video_frame_rate;

                // let mut delay: f64 = (frame_rate.num as f64) / (frame_rate.den as f64);
                let mut delay: f64 = 0.;
                unsafe {
                // let mut last_clock = movie_state.last_pts_time;
                let mut last_clock = self.clock.last_updated;
                // if movie_state.last_pts == ffi::AV_NOPTS_VALUE {
                if self.clock.pts == ffi::AV_NOPTS_VALUE {
                    last_clock = 0;
                }
                let delta = current_clock - last_clock;


                // debug!("last_clock: {}  delta: {}", last_clock, delta );
                // debug!("       pts: {}", pts * (time_base.den as i64 / time_base.num as i64) );
                // debug!("       pts: {}", pts );

                // we looped, this pts is less than last displayed pts
                if pts < movie_state.last_pts && movie_state.last_pts != ffi::AV_NOPTS_VALUE {
                    movie_state.last_pts = ffi::AV_NOPTS_VALUE;
                    movie_state.last_pts = 0;
                    // reset the master clock down
                    // self.clock.pts = movie_state.last_pts;
                }

                if movie_state.last_pts != ffi::AV_NOPTS_VALUE {
                    let time_base = (*(movie_state.video_stream.lock().unwrap()).ptr).time_base;
                    let pts_time = pts as f64  * time_base.num as f64 / time_base.den as f64;
                    let movie_delta_time = ((current_clock as f64) / 1_000_000.) - ((self.clock.last_updated as f64) / 1_000_000.);
                    if pts_time - movie_state.last_pts_time >  movie_delta_time {
                        delay = pts_time - movie_state.last_pts_time - movie_delta_time;
                        // info!("{}, {}, {}", movie_delta_time, pts_time - movie_state.last_pts_time, delay);
                    }

                    movie_state.last_pts_time = pts_time;
                }
                }
                if delay > 0.0001 {
                    // return None;
                    // TODO: check other movie_states to see if any other frame is ready for display
                    ::std::thread::sleep(std::time::Duration::from_secs_f64((delay - 0.0001).max(0.)));
                    return (0, None);
                    // continue;
                }
                let time_base = unsafe {(*(movie_state.video_stream.lock().unwrap()).ptr).time_base};
                let pts_time = pts as f64  * time_base.num as f64 / time_base.den as f64;

                // we need to negative delay, let's drop this frame
                // if delay < -0.21 && ! self.paused.load(::std::sync::atomic::Ordering::Relaxed) {
                // if (movie_state.last_display_time + movie_state.last_pts_time as f64) < (current_clock as f64 / 1_000_000.)
                if (movie_state.last_display_time + pts_time as f64 - 0.001) < (current_clock as f64 / 1_000_000.)
                    && ! self.paused.load(::std::sync::atomic::Ordering::Relaxed)
                    && movie_state.last_pts != ffi::AV_NOPTS_VALUE {
                // if (movie_state.last_pts_time < delay * 2.) && ! self.paused.load(::std::sync::atomic::Ordering::Relaxed) {
                    // frame drop
                    // info!("frame drop {}", delay);
                    // movie_state.last_pts      = pts; //ffi::AV_NOPTS_VALUE;
                    // movie_state.last_pts_time = pts_time;
                    // TODO: don't update the last clock somehow.  otherwise the movies can
                    // TODO: get out of sync with eacher but remain relatively correct with the deltas
                    let _ = movie_state.dequeue_frame().unwrap();
                    continue 'retry;
                    // return (0, None);
               }

               movie_state.last_pts      = pts;
               movie_state.last_display_time = current_clock as f64 / 1_000_000.;
               // movie_state.last_pts_time = unsafe { ffi::av_gettime_relative() };
               // movie_state.last_pts_time = current_clock as f64 / 1_000_000.;
               // next frame's delay is based on only the latest pts
               if self.clock.pts < movie_state.last_pts {
                   self.clock.pts = movie_state.last_pts;
               }
               self.clock.last_updated = current_clock;
               return (movie_index as _, Some(pts));
               }
            }
        (0, None)
    }

    pub fn step(&mut self) {
        if ! self.paused.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        self.movie_list.iter_mut().for_each(|movie| {
            movie.step_force();
        });
        self.force_render = true;
    }

    pub fn pause(&mut self) {
        let state = self.paused.load(std::sync::atomic::Ordering::Relaxed);
        self.paused.store(!state, std::sync::atomic::Ordering::Relaxed);

        self.movie_list.iter_mut().for_each(|movie| {
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

    pub fn movie_list_iter(&self) -> Iter<MovieState> {
        return self.movie_list.iter();
    }
}
