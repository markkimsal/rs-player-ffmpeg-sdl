#![allow(unused_variables, dead_code)]
use std::{collections::VecDeque, ops::Deref, sync::{mpsc::Sender, Arc, Mutex}, thread::JoinHandle};

use rusty_ffmpeg::ffi::{self};
pub struct RecordState {
    pub format_context: Arc<Mutex<FormatContextWrapper>>,
    pub audio_stream: Arc<Mutex<StreamWrapper>>,
    pub audio_ctx: Arc<Mutex<CodecContextWrapper>>,
    pub audio_buf: [u8; 1024 * 1024],
    // pub audio_pkt: *const ffi::AVPacket,
    pub videoqueue: Arc<Mutex<VecDeque<PacketWrapper>>>,
    pub video_stream: Arc<Mutex<StreamWrapper>>,
    pub video_ctx: Arc<Mutex<CodecContextWrapper>>,
    pub picq: Arc<Mutex<VecDeque<FrameWrapper>>>,
    pub paused: std::sync::atomic::AtomicBool,
    pub join_handle: Option<JoinHandle<()>>,
}

impl Drop for RecordState {
    fn drop(&mut self) {
        // claim lock to drain other threads
        {
            let video_ctx = self.video_ctx.lock().unwrap();
            unsafe {ffi::av_free(video_ctx.ptr as *mut _);}
            drop(video_ctx);
        }
        println!("dropping record state");
    }
}

impl RecordState {
    pub fn new () -> RecordState {
        RecordState {
            format_context: Arc::new(Mutex::new(FormatContextWrapper{ptr:std::ptr::null_mut()})),
            audio_stream: Arc::new(Mutex::new(StreamWrapper{ptr:std::ptr::null_mut()})),
            audio_ctx: Arc::new(Mutex::new(CodecContextWrapper{ptr:std::ptr::null_mut()})),
            audio_buf: [0; 1024 * 1024],
            // audio_pkt: std::ptr::null_mut(),
            videoqueue: Arc::new(Mutex::new(VecDeque::with_capacity(10))),
            // audio_pkt: std::ptr::null_mut(),
            video_stream: Arc::new(Mutex::new(StreamWrapper{ptr:std::ptr::null_mut()})),
            video_ctx: Arc::new(Mutex::new(CodecContextWrapper{ptr:std::ptr::null_mut()})),
            picq: Arc::new(Mutex::new(VecDeque::with_capacity(3))),
            paused: std::sync::atomic::AtomicBool::new(false),
            join_handle: None,
        }
    }

    pub fn start_recording_thread(&mut self) -> Option<Sender<FrameWrapper>> {
        let (tx, rx) = std::sync::mpsc::channel::<FrameWrapper>();

        unsafe {
            let codec = ffi::avcodec_find_encoder_by_name("libx264".as_ptr() as _);
            let pkt = ffi::av_packet_alloc();
        
            let c = ffi::avcodec_alloc_context3(codec);
            /* put sample parameters */
            c.as_mut().unwrap().bit_rate = 400000;
            /* resolution must be a multiple of two */
            c.as_mut().unwrap().width = 352;
            c.as_mut().unwrap().height = 288;
            /* frames per second */
            c.as_mut().unwrap().time_base = ffi::AVRational{ num: 1, den: 25};
            c.as_mut().unwrap().framerate = ffi::AVRational{ num: 25, den: 1};
        
            self.video_ctx = Arc::new(Mutex::new(CodecContextWrapper{
                ptr:  c
            }));

        }
        self.join_handle = Some(std::thread::spawn(move|| {
            while let Ok(msg) = rx.recv() {
                unsafe {
                    println!("📽📽  received frame: {}", msg.ptr.as_mut().unwrap().best_effort_timestamp);
                }
            }
            eprintln!("🦀🦀 stopping record thread: ");
        }));
        Some(tx)
    }
}

pub fn start_recording_thread() -> (Option<Sender<String>>, JoinHandle<()>) {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let handle = std::thread::spawn(move|| {
        while let Ok(msg) = rx.recv() {
            println!("🦀🦀 received message: {}", msg);
        }
        eprintln!("🦀🦀 stopping record thread: ");
    });
    (Some(tx), handle)
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
