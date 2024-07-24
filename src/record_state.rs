#![allow(unused_variables, dead_code)]
use std::{
    collections::VecDeque,
    fs::File, ops::Deref, sync::{mpsc::SyncSender, Arc, Mutex}, thread::JoinHandle
};
use std::io::Write;
use log::{debug, error, info};
use rusty_ffmpeg::ffi;

pub struct RecordState {
    pub format_context: Arc<Mutex<FormatContextWrapper>>,
    pub audio_stream: Arc<Mutex<StreamWrapper>>,
    pub audio_ctx: Arc<Mutex<CodecContextWrapper>>,
    pub audio_buf: [u8; 1024 * 1024],
    // pub audio_pkt: *const ffi::AVPacket,
    pub videoqueue: Arc<Mutex<VecDeque<PacketWrapper>>>,
    pub video_stream: Arc<Mutex<StreamWrapper>>,
    pub picq: Arc<Mutex<VecDeque<FrameWrapper>>>,
    pub paused: std::sync::atomic::AtomicBool,
    pub join_handle: Option<JoinHandle<()>>,
}

impl Drop for RecordState {
    fn drop(&mut self) {
        // claim lock to drain other threads
        // unsafe {
            // let video_ctx = self.video_ctx.lock().unwrap();
            // ffi::av_free(video_ctx.ptr as *mut _);
            // drop(video_ctx);
        // };
        unsafe {
            let format_ctx = self.format_context.lock().unwrap();
            ffi::avformat_free_context(format_ctx.ptr);
            drop(format_ctx);
        };
        info!("dropping record state");
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
            picq: Arc::new(Mutex::new(VecDeque::with_capacity(3))),
            paused: std::sync::atomic::AtomicBool::new(false),
            join_handle: None,
        }
    }

    pub unsafe fn start_recording_thread(&mut self) -> Option<SyncSender<FrameWrapper>> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<FrameWrapper>(3);

        let file_ext: std::ffi::CString = std::ffi::CString::new("mp4").unwrap();
        let file_name: std::ffi::CString = std::ffi::CString::new("output.mp4").unwrap();
        let mut video_codec: *const ffi::AVCodec = std::ptr::null();
        let mut video_st = OutputStream::new();
        let mut fctx = ffi::avformat_alloc_context();
        // let f = ffi::avformat_alloc_output_context2(&mut fctx, std::ptr::null(), file_ext.as_ptr() as _, file_name.as_ptr() as _);
        // let f = ffi::avformat_alloc_output_context2(&mut fctx, out_fmt, std::ptr::null(), file_name.as_ptr() as _);
        let _ = ffi::avformat_alloc_output_context2(&mut fctx, std::ptr::null(), file_ext.as_ptr() as _, file_name.as_ptr() as _);
        let out_fmt = (*fctx).oformat;
        self.format_context = Arc::new(Mutex::new(FormatContextWrapper{ptr: fctx}));
        if (*out_fmt).video_codec != ffi::AVCodecID_AV_CODEC_ID_NONE {
            add_stream(&mut video_st, &mut fctx, &mut video_codec, (*out_fmt).video_codec, 1280, 720);
        }
        open_video(fctx, &mut video_codec, &mut video_st);
        ffi::av_dump_format(fctx, 0, file_name.as_ptr() as _, 1);

        // let locked_video_ctx = self.video_ctx.clone(); // expect("someone else is using the encode context");

        let locked_format_ctx = self.format_context.clone(); // expect("someone else is using the encode context");
        #[allow(unused_mut)]
        let mut pts = 0 as i64;
        self.join_handle = Some(std::thread::spawn(move|| {
            let pkt = ffi::av_packet_alloc().as_mut().unwrap();
            let locked_format_ctx = locked_format_ctx.lock().unwrap().ptr;
            let video_st = video_st;
            // let mut file_out = std::fs::File::create("output.mp4").expect("cannot open output.mp4");
            ffi::avio_open(&mut locked_format_ctx.as_mut().unwrap().pb, file_name.as_ptr() as _, ffi::AVIO_FLAG_WRITE as i32);
            ffi::avformat_write_header(locked_format_ctx, std::ptr::null_mut());
            info!("📽 📽  output file : output.mp4");
            while let Ok(msg) = rx.recv() {
                unsafe {
                    write_frame_interleaved(&video_st, locked_format_ctx, pkt, pts, &msg);
                    // write_out_buffer(
                    //     (*(*(*msg)).buf[0]).data,
                    //     (*(*(*msg)).buf[0]).size,
                    //     "after_write.yuv");
                }
                ffi::av_frame_unref(msg.ptr);
            }
            info!("📽 📽 stopping record thread");

            // if locked_video_ctx.lock().unwrap().ptr.as_ref().unwrap().codec_id == ffi::AVCodecID_AV_CODEC_ID_MPEG2VIDEO {
            //     // let endcode: [u8; 4 ] = [ 0, 0, 1, 0xb7 ];
            //     // let _ = file_out.write(&endcode);
            // } else {
            //     ffi::av_write_trailer(locked_format_ctx);
            // }
            ffi::av_write_trailer(locked_format_ctx);
            // let _ = file_out.flush();
        }));
        Some(tx)
    }
}

struct OutputStream {
    st: StreamWrapper,
    enc_ctx: CodecContextWrapper,
    next_pts: i64,
    frame: FrameWrapper,
}
unsafe impl Send for OutputStream{}
unsafe impl Sync for OutputStream{}
impl OutputStream {
    fn new() -> OutputStream {
        OutputStream {
            st: StreamWrapper{ ptr: std::ptr::null_mut() },
            enc_ctx: CodecContextWrapper{ptr:std::ptr::null_mut()},
            next_pts: 0,
            frame: FrameWrapper{ ptr: std::ptr::null_mut() },
        }
    }
}
unsafe fn add_stream(
    ost: &mut OutputStream,
    oc: &mut *mut ffi::AVFormatContext,
    codec: &mut *const ffi::AVCodec,
    codec_id: ffi::AVCodecID,
    width: usize,
    height: usize,
) {
    ost.st = StreamWrapper{ ptr: ffi::avformat_new_stream(*oc, std::ptr::null_mut()) };

    let desired_encoder: std::ffi::CString = std::ffi::CString::new("libopenh264").unwrap();
    *codec = ffi::avcodec_find_encoder_by_name(desired_encoder.as_ptr() as _);
    if codec.is_null() {
        *codec = ffi::avcodec_find_encoder(codec_id);
    }
    let c = ffi::avcodec_alloc_context3(*codec);
    let c = c.as_mut().unwrap();
    match codec.as_ref().unwrap().type_ {
        ffi::AVMediaType_AVMEDIA_TYPE_VIDEO => {
            c.codec_type = ffi::AVMediaType_AVMEDIA_TYPE_VIDEO;
            /* put sample parameters */
            c.bit_rate = 400000;
            /* resolution must be a multiple of two */
            c.width = width as _;
            c.height = height as _;

            c.gop_size = 10;
            c.max_b_frames = 1;
            c.pix_fmt = ffi::AVPixelFormat_AV_PIX_FMT_YUV420P;
            // c.profile = ffi::FF_PROFILE_H264_CONSTRAINED_BASELINE as _;
            // c.profile = ffi::FF_PROFILE_H264_MAIN as _;
            ost.st.as_mut().unwrap().time_base = ffi::AVRational{num: 1, den: 25};
            // TODO: set as a reference to ost.st?
            /* frames per second */
            c.time_base = ffi::AVRational{ num: 1, den: 25};
            c.framerate = ffi::AVRational{ num: 25, den: 1};
        }
        _ => {
            error!("📽 📽  unknnown codec type: {:?}", (*(*codec)).type_);
        }
    }
    ost.enc_ctx.ptr = c;
}

unsafe fn open_video(
    oc: *const ffi::AVFormatContext,
    codec: &mut *const ffi::AVCodec,
    ost: &mut OutputStream,
) {
    let _ = ffi::avcodec_open2(ost.enc_ctx.ptr, *codec, std::ptr::null_mut());
    info!("📽 📽  opened codec: {:?}", codec);

    /* Some formats want stream headers to be separate. */
    // if ((*oc).oformat.as_ref().unwrap().flags & ffi::AVFMT_GLOBALHEADER as i32) != 0 {
    //     ost.enc_ctx.ptr.as_mut().unwrap().flags |= ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
    // }
    ffi::avcodec_parameters_from_context((*ost.st.ptr).codecpar, ost.enc_ctx.ptr);
}

unsafe fn write_frame_interleaved(
    video_st: &OutputStream,
    locked_format_ctx: *mut ffi::AVFormatContext,
    pkt: *mut ffi::AVPacket,
    pts: i64,
    msg: &FrameWrapper,
) {
    let frame = msg.ptr.as_mut().unwrap();
    debug!("📽 📽  received frame: wxh {}x{}  pts: {}", frame.width, frame.height, frame.pts);
    // pts = pts + 1 as i64;
    // frame.pts = pts;
    let mut ret = ffi::avcodec_send_frame(*video_st.enc_ctx, frame);
    if ret < 0 {
        error!("📽 📽  avcodec_send_frame: {}", ret);
        error!("📽 📽  avcodec_send_frame: {}", ffi::av_err2str(ret));
    }
    while ret >= 0 {
        ret = ffi::avcodec_receive_packet(*video_st.enc_ctx, pkt);
        if ret == ffi::AVERROR(ffi::EAGAIN) || ret == ffi::AVERROR_EOF {
            break;
        } else if ret < 0 {
        }
        (*pkt).stream_index = 0;

        /* rescale output packet timestamp values from codec to stream timebase */
        ffi::av_packet_rescale_ts(pkt, (*video_st.enc_ctx.ptr).time_base, (*video_st.st.ptr).time_base);

        ffi::av_interleaved_write_frame(locked_format_ctx, pkt);

        // let buf = std::slice::from_raw_parts(pkt.as_ref().unwrap().data, pkt.as_ref().unwrap().size as _);
        // eprintln!("📽 📽  write packet: {} (size={})", pkt.as_ref().unwrap().pts, pkt.as_ref().unwrap().size);
        // let _ =file_out.write(&buf);
        ffi::av_packet_unref(pkt);
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

#[allow(dead_code)]
fn write_out_buffer(buffer: *const u8, len: usize, filename: &str) {
    unsafe {
        // buffer.iter().for_each(|b| println!("{:02x}", b));
        let mut file_out = File::create(filename).expect("cannot open output.mp4");
        let bfslice: &[u8] = &*std::ptr::slice_from_raw_parts(buffer, len);
        file_out.write_all(bfslice as _).unwrap();
        // file_out.write_all(buffer.into()).unwrap();
        let _ = file_out.flush();
    }
}
