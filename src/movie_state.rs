#![allow(unused_variables, dead_code, unused)]
use std::{ops::Deref, sync::Mutex, collections::VecDeque};

use log::{error, info};
use rusty_ffmpeg::ffi::{self};

use crate::filter::init_filter;

#[repr(C)]
pub struct MovieState {
    pub format_context: Mutex<FormatContextWrapper>,
    pub video_stream_idx: i64,
    pub audio_stream_idx: i64,
    pub audio_stream: Mutex<StreamWrapper>,
    pub audio_ctx: Mutex<CodecContextWrapper>,
    pub audio_buf: [u8; 1024 * 1024],
    // pub audio_pkt: *const ffi::AVPacket,
    pub videoqueue: Mutex<VecDeque<PacketWrapper>>,
    pub video_stream: Mutex<StreamWrapper>,
    pub video_ctx: Mutex<CodecContextWrapper>,
    pub picq: Mutex<VecDeque<FrameWrapper>>,
    pub paused: std::sync::atomic::AtomicBool,
    pub in_vfilter: Mutex<FilterContextWrapper>,   // the first filter in the video chain
    pub out_vfilter: Mutex<FilterContextWrapper>,   // the last filter in the video chain
    pub vgraph: Mutex<FilterGraphWrapper>,
    pub video_frame_rate: ffi::AVRational,
    pub last_pts: i64,
    pub last_pts_time: f64,
    pub last_display_time: f64,
    pub step: bool,
}
impl Drop for MovieState {
    fn drop(&mut self) {
        // claim lock to drain other threads
        {
            let video_ctx = self.video_ctx.lock().unwrap();
            unsafe {ffi::av_free(video_ctx.ptr as *mut _);}
            drop(video_ctx);
        }
        {
            self.clear_packet_queue().unwrap();
        }
        {
            let mut format_ctx = self.format_context.lock().unwrap();
            unsafe {ffi::avformat_close_input(&mut format_ctx.ptr);}
        }

        // make sure its empty after giving up the lock
        assert!(self.videoqueue.lock().unwrap().is_empty());
        info!("dropping movie state");
    }
}
impl MovieState {
    pub fn new () -> MovieState {
        let vgraph = unsafe {ffi::avfilter_graph_alloc()};
        MovieState {
            format_context: Mutex::new(FormatContextWrapper{ptr:std::ptr::null_mut()}),
            video_stream_idx: -1,
            audio_stream_idx: -1,
            audio_stream: Mutex::new(StreamWrapper{ptr:std::ptr::null_mut()}),
            audio_ctx: Mutex::new(CodecContextWrapper{ptr:std::ptr::null_mut()}),
            audio_buf: [0; 1024 * 1024],
            videoqueue: Mutex::new(VecDeque::with_capacity(10)),
            // audio_pkt: std::ptr::null_mut(),
            video_stream: Mutex::new(StreamWrapper{ptr:std::ptr::null_mut()}),
            video_ctx: Mutex::new(CodecContextWrapper{ptr:std::ptr::null_mut()}),
            picq: Mutex::new(VecDeque::with_capacity(3)),
            paused: std::sync::atomic::AtomicBool::new(false),
            in_vfilter: Mutex::new(FilterContextWrapper{ ptr:std::ptr::null_mut() }),
            out_vfilter: Mutex::new(FilterContextWrapper { ptr: std::ptr::null_mut() }),
            vgraph: Mutex::new(FilterGraphWrapper { ptr: vgraph }),
            video_frame_rate: ffi::AVRational { num: 1, den: 60 },
            last_pts: ffi::AV_NOPTS_VALUE,
            last_pts_time: 0.,
            last_display_time: 0.,
            step: false,
        }
    }
}
unsafe impl Send for MovieState{}
impl MovieState {
    pub fn set_format_context(&mut self, format_context: *mut ffi::AVFormatContext) {
        self.format_context = Mutex::new(FormatContextWrapper{ptr:format_context});
    }
    pub fn enqueue_packet(&self, packet: *mut ffi::AVPacket) -> Result<(), ()> {
        let mut vq = self.videoqueue.lock().unwrap();
        if vq.len() >= 10 {
            return Err(());
        }
        vq.push_back(PacketWrapper{ptr:packet});
        return Ok(());
    }

    pub fn clear_packet_queue(&mut self) -> Result<(), ()> {
        let mut vq = self.videoqueue.lock().unwrap();

        vq.iter_mut().for_each(|p| unsafe {
            ffi::av_packet_free(&mut p.ptr);
        });
        vq.clear();
        return Ok(());
    }

    pub fn enqueue_frame(&self, frame: *mut ffi::AVFrame) -> Result<(), ()> {
        let mut pq = self.picq.lock().unwrap();
        if pq.len() >= 4 {
            // eprintln!("dropping frame");
            return Err(());
        }
        pq.push_back(FrameWrapper{ptr:frame});
        return Ok(());
    }

    pub fn dequeue_frame(&self) -> Option<FrameWrapper> {
        let mut pq = self.picq.lock().unwrap();
        if pq.len() <= 0 {
            return None
        }
        pq.pop_front()
    }

    pub fn dequeue_frame_raw(&mut self) -> Option<*mut ffi::AVFrame> {
        if (self.step == false && self.is_paused()) {
            return None;
        }
        if (self.step) {
            self.step = false;
        }
        let mut pq = self.picq.lock().unwrap();
        if pq.len() <= 0 {
            return None
        }
        unsafe {
            let dest_frame = 
                ffi::av_frame_alloc()
                .as_mut()
                .expect("failed to allocated memory for AVFrame");

            let frame = pq.pop_front().unwrap();
            let mut in_vfilter = self.in_vfilter.lock().unwrap();
            let mut out_vfilter = self.out_vfilter.lock().unwrap();
            let mut vgraph = self.vgraph.lock().unwrap();

            if in_vfilter.is_null() || out_vfilter.is_null() {
                let rotation = 0;
                init_filter(
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

    pub fn peek_frame_pts(&self) -> Option<i64> {
        let pq = self.picq.lock().unwrap();
        if pq.len() <= 0 {
            return None
        }
        let front = pq.front().unwrap();
        unsafe {Some(front.ptr.as_ref().unwrap().pts)}
    }

    pub fn pause(&mut self) {
        let state = self.paused.load(std::sync::atomic::Ordering::Relaxed);
        self.paused.store(!state, std::sync::atomic::Ordering::Relaxed);
        self.last_pts = ffi::AV_NOPTS_VALUE;
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn step(&mut self) {
        if ! self.paused.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        self.step = true;
    }

    pub fn step_force(&mut self) {
        self.step = true;
    }

    pub fn update_last_pts_time(&mut self, pts: i64) {
        let time_base = unsafe {(*(self.video_stream.lock().unwrap()).ptr).time_base};
        self.last_pts_time = pts as f64  * time_base.num as f64 / time_base.den as f64;
    }

    pub fn update_last_time(&mut self, t: i64) {
        self.last_pts_time = (t as f64) / 1_000_000.;
    }

}
pub fn movie_state_enqueue_packet(videoqueue: &Mutex<VecDeque<PacketWrapper>>, packet: *mut ffi::AVPacket) -> Result<(), ()> {
    let mut vq = videoqueue.lock().unwrap();
    if vq.len() >= 10 {
        return Err(());
    }
    vq.push_back(PacketWrapper{ptr:packet});
    return Ok(());
}
pub fn movie_state_enqueue_frame(picq: &Mutex<VecDeque<FrameWrapper>>, frame: *mut ffi::AVFrame) -> Result<(), ()> {

    let mut pq = picq.lock().unwrap();
    if pq.len() >= 4 {
        // eprintln!("dropping frame");
        return Err(());
    }
    unsafe {
        let clone_frame = ffi::av_frame_clone(frame);
        pq.push_back(FrameWrapper{ptr:clone_frame});
    }
    return Ok(());
}

pub struct FilterGraphWrapper {
    pub ptr: *mut ffi::AVFilterGraph,
}
unsafe impl Send for FilterGraphWrapper{}
impl Deref for FilterGraphWrapper {
    type Target = *mut ffi::AVFilterGraph;
    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

pub struct FilterContextWrapper {
    pub ptr: *mut ffi::AVFilterContext,
}
unsafe impl Send for FilterContextWrapper{}
impl Deref for FilterContextWrapper {
    type Target = *mut ffi::AVFilterContext;
    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

pub struct FormatContextWrapper {
    pub ptr: *mut ffi::AVFormatContext,
}
unsafe impl Send for FormatContextWrapper{}
impl Deref for FormatContextWrapper {
    type Target = *mut ffi::AVFormatContext;
    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}

pub struct CodecContextWrapper {
    pub ptr: *mut ffi::AVCodecContext,
}
unsafe impl Send for CodecContextWrapper{}
impl Deref for CodecContextWrapper {
    type Target = *mut ffi::AVCodecContext;
    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}
pub struct StreamWrapper {
    pub ptr: *mut ffi::AVStream,
}
unsafe impl Send for StreamWrapper{}
impl Deref for StreamWrapper {
    type Target = *mut ffi::AVStream;
    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}
pub struct PacketWrapper {
    pub ptr: *mut ffi::AVPacket,
}
unsafe impl Send for PacketWrapper{}
impl Deref for PacketWrapper {
    type Target = *mut ffi::AVPacket;
    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}
pub struct FrameWrapper {
    pub ptr: *mut ffi::AVFrame,
}
unsafe impl Send for FrameWrapper{}
impl Deref for FrameWrapper {
    type Target = *mut ffi::AVFrame;
    fn deref(&self) -> &Self::Target {
        &self.ptr
    }
}
