#![allow(unused_variables, dead_code)]
use std::{ops::Deref, sync::{Arc, Mutex}, collections::VecDeque};

use rusty_ffmpeg::ffi::{self};
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
