#![allow(unused_variables, dead_code)]
use std::ops::Deref;

use rusty_ffmpeg::ffi;

pub struct MovieState {
    pub format_context: FormatContextWrapper,
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
            format_context: FormatContextWrapper{ptr:std::ptr::null_mut()},
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
unsafe impl Send for MovieState{}
impl MovieState {
    pub fn set_format_context(&mut self, format_context: *mut ffi::AVFormatContext) {
        self.format_context = FormatContextWrapper{ptr:format_context};
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
