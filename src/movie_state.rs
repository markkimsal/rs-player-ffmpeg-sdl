#![allow(unused_variables, dead_code)]
use std::{ops::Deref, sync::{Arc, Mutex}, collections::VecDeque};

use rusty_ffmpeg::ffi::{self};
use sdl2::video;
#[repr(C)]
pub struct MovieState {
    pub format_context: Arc<Mutex<FormatContextWrapper>>,
    pub video_stream_idx: i64,
    pub audio_stream_idx: i64,
    pub audio_stream: Arc<Mutex<StreamWrapper>>,
    pub audio_ctx: Arc<Mutex<CodecContextWrapper>>,
    pub audio_buf: [u8; 1024 * 1024],
    // pub audio_pkt: *const ffi::AVPacket,
    pub videoqueue: Arc<Mutex<VecDeque<PacketWrapper>>>,
    pub video_stream: Arc<Mutex<StreamWrapper>>,
    pub video_ctx: Arc<Mutex<CodecContextWrapper>>,
    pub picq: Arc<Mutex<VecDeque<FrameWrapper>>>,
    pub paused: std::sync::atomic::AtomicBool,
    pub in_vfilter: *const ffi::AVFilterContext,   // the first filter in the video chain
    pub out_vfilter: *const ffi::AVFilterContext,   // the last filter in the video chain
    pub vgraph: *const ffi::AVFilterGraph,
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
        println!("dropping movie state");
    }
}
impl MovieState {
    pub fn new () -> MovieState {
        MovieState {
            format_context: Arc::new(Mutex::new(FormatContextWrapper{ptr:std::ptr::null_mut()})),
            video_stream_idx: -1,
            audio_stream_idx: -1,
            audio_stream: Arc::new(Mutex::new(StreamWrapper{ptr:std::ptr::null_mut()})),
            audio_ctx: Arc::new(Mutex::new(CodecContextWrapper{ptr:std::ptr::null_mut()})),
            audio_buf: [0; 1024 * 1024],
            videoqueue: Arc::new(Mutex::new(VecDeque::with_capacity(10))),
            // audio_pkt: std::ptr::null_mut(),
            video_stream: Arc::new(Mutex::new(StreamWrapper{ptr:std::ptr::null_mut()})),
            video_ctx: Arc::new(Mutex::new(CodecContextWrapper{ptr:std::ptr::null_mut()})),
            picq: Arc::new(Mutex::new(VecDeque::with_capacity(3))),
            paused: std::sync::atomic::AtomicBool::new(false),
            in_vfilter: std::ptr::null(),
            out_vfilter: std::ptr::null(),
            vgraph: std::ptr::null(),
        }
    }
}
unsafe impl Send for MovieState{}
impl MovieState {
    pub fn set_format_context(&mut self, format_context: *mut ffi::AVFormatContext) {
        self.format_context = Arc::new(Mutex::new(FormatContextWrapper{ptr:format_context}));
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
        return pq.pop_front();
    }
    pub fn peek_frame_pts(&self) -> Option<i64> {
        let pq = self.picq.lock().unwrap();
        if pq.len() <= 0 {
            return None
        }
        let front = pq.front().unwrap();
        unsafe {Some(front.ptr.as_ref().unwrap().pts)}
    }

    pub fn pause(&self) {
        let state = self.paused.load(std::sync::atomic::Ordering::Relaxed);
        self.paused.store(!state, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_paused(&self) -> bool{
        self.paused.load(std::sync::atomic::Ordering::Relaxed)
    }

}
pub fn movie_state_enqueue_packet(videoqueue: &Arc<Mutex<VecDeque<PacketWrapper>>>, packet: *mut ffi::AVPacket) -> Result<(), ()> {
    let mut vq = videoqueue.lock().unwrap();
    if vq.len() >= 10 {
        return Err(());
    }
    vq.push_back(PacketWrapper{ptr:packet});
    return Ok(());
}
pub fn movie_state_enqueue_frame(picq: &Arc<Mutex<VecDeque<FrameWrapper>>>, frame: *mut ffi::AVFrame) -> Result<(), ()> {
    let mut pq = picq.lock().unwrap();
    if pq.len() >= 4 {
        // eprintln!("dropping frame");
        return Err(());
    }
    pq.push_back(FrameWrapper{ptr:frame});
    return Ok(());
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
