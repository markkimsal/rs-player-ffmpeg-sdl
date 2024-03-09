#![allow(unused_variables, dead_code)]
use rusty_ffmpeg::ffi;

pub struct MovieState {
    pub format_context: *mut ffi::AVFormatContext,
    pub video_stream_idx: i64,
    pub audio_stream_idx: i64,
    pub audio_stream: *mut ffi::AVStream,
    pub audio_ctx: *mut ffi::AVCodecContext,
    pub audio_buf: [u8; 1024 * 1024],
    pub audio_pkt: *const ffi::AVPacket,
    pub video_stream: *mut ffi::AVStream,
    pub video_ctx: *mut ffi::AVCodecContext,
}

impl MovieState {
    pub fn new () -> MovieState {
        MovieState {
            format_context: std::ptr::null_mut(),
            video_stream_idx: -1,
            audio_stream_idx: -1,
            audio_stream: std::ptr::null_mut(),
            audio_ctx: std::ptr::null_mut(),
            audio_buf: [0; 1024 * 1024],
            audio_pkt: std::ptr::null_mut(),
            video_stream: std::ptr::null_mut(),
            video_ctx: std::ptr::null_mut(),
        }
    }
}
