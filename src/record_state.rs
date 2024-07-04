#![allow(unused_variables, dead_code)]
use std::{borrow::BorrowMut, collections::VecDeque, ops::Deref, sync::{mpsc::SyncSender, Arc, Mutex}, thread::JoinHandle};

use rusty_ffmpeg::ffi::{self};
pub struct RecordState {
    pub format_context: Arc<Mutex<FormatContextWrapper>>,
    pub audio_stream: Arc<Mutex<StreamWrapper>>,
    pub audio_ctx: Arc<Mutex<CodecContextWrapper>>,
    pub audio_buf: [u8; 1024 * 1024],
    // pub audio_pkt: *const ffi::AVPacket,
    pub videoqueue: Arc<Mutex<VecDeque<PacketWrapper>>>,
    pub video_stream: Arc<Mutex<StreamWrapper>>,
    // pub video_ctx: Arc<Mutex<CodecContextWrapper>>,
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
            // video_ctx: Arc::new(Mutex::new(CodecContextWrapper{ptr:std::ptr::null_mut()})),
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
        dbg!(&fctx);
        #[allow(unused_mut)]
        let mut out_fmt = fctx.as_ref().unwrap().oformat;
        self.format_context = Arc::new(Mutex::new(FormatContextWrapper{ptr: fctx}));
        if out_fmt.as_ref().unwrap().video_codec != ffi::AVCodecID_AV_CODEC_ID_NONE {
            add_stream(&mut video_st, &mut fctx, &mut video_codec, out_fmt.as_ref().unwrap().video_codec);
            ffi::av_dump_format(fctx, 0, file_name.as_ptr() as _, 1);
        }
        open_video(fctx, &mut video_codec, &mut video_st);
        ffi::av_dump_format(fctx, 0, file_name.as_ptr() as _, 1);

        /* Some formats want stream headers to be separate. */
        // if (fctx.as_mut().unwrap().oformat.as_ref().unwrap().flags & ffi::AVFMT_GLOBALHEADER as i32) != 0 {
        //     self.video_ctx.lock().unwrap().ptr.as_mut().unwrap().flags |= ffi::AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        // }
        // let locked_video_ctx = self.video_ctx.clone(); // expect("someone else is using the encode context");

        let locked_video_ctx = video_st.enc_ctx.clone();
        let locked_format_ctx = self.format_context.clone(); // expect("someone else is using the encode context");
        #[allow(unused_mut)]
        let mut pts = 0 as i64;
        self.join_handle = Some(std::thread::spawn(move|| {
            let pkt = ffi::av_packet_alloc().as_mut().unwrap();
            let locked_format_ctx = locked_format_ctx.lock().unwrap().ptr;
            let locked_video_ctx = locked_video_ctx.lock().unwrap().ptr;
            // let mut file_out = std::fs::File::create("output.mp4").expect("cannot open output.mp4");
            ffi::avio_open(&mut locked_format_ctx.as_mut().unwrap().pb, file_name.as_ptr() as _, ffi::AVIO_FLAG_WRITE as i32);
            ffi::avformat_write_header(locked_format_ctx, std::ptr::null_mut());
            eprintln!("📽📽  output file : output.mp4");
            while let Ok(msg) = rx.recv() {
                unsafe {

                    let frame = msg.ptr.as_mut().unwrap();
                    println!("📽📽  received frame: wxh {}x{}", frame.width, frame.height);
                    println!("📽📽  received frame: pts {}", frame.pts);
                    // pts = pts + 1 as i64;
                    // frame.pts = pts;
                    let mut ret = ffi::avcodec_send_frame(locked_video_ctx, frame);
                    if ret < 0 {
                        eprintln!("📽📽  avcoded_send_frame: {}", ret);
                    }
                    while ret >= 0 {
                        ret = ffi::avcodec_receive_packet(locked_video_ctx, pkt);
                        if ret == ffi::AVERROR(ffi::EAGAIN) || ret == ffi::AVERROR_EOF {
                            break;
                        } else if ret < 0 {
                        }
                        pkt.stream_index = 0;
                        // pkt.time_base = ffi::AVRational{num: 1, den: 25};

                        // TODO: arc the output stream and read the stream's timebase
                        /* rescale output packet timestamp values from codec to stream timebase */
                        ffi::av_packet_rescale_ts(pkt, (*locked_video_ctx).time_base, ffi::AVRational{num: 1, den: 12800});
                        // pkt.as_mut().unwrap().pts = pts;

                        // ffi::av_interleaved_write_frame(locked_format_ctx.ptr, pkt);
                        ffi::av_write_frame(locked_format_ctx, pkt);
                        ffi::av_packet_unref(pkt);

                        // let buf = std::slice::from_raw_parts(pkt.as_ref().unwrap().data, pkt.as_ref().unwrap().size as _);
                        // eprintln!("📽📽  write packet: {} (size={})", pkt.as_ref().unwrap().pts, pkt.as_ref().unwrap().size);
                        // let _ =file_out.write(&buf);
                        // ffi::av_packet_unref(pkt);
                    }
                }
            }
            eprintln!("🦀🦀 stopping record thread");

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
    st: *mut ffi::AVStream,
    enc_ctx: Arc<Mutex<CodecContextWrapper>>,
    next_pts: i64,
    frame: *mut ffi::AVFrame,
}
impl OutputStream {
    fn new() -> OutputStream {
        OutputStream {
            st: std::ptr::null_mut(),
            enc_ctx: Arc::new(Mutex::new(CodecContextWrapper{ptr:std::ptr::null_mut()})),
            next_pts: 0,
            frame: std::ptr::null_mut(),
        }
    }
}
unsafe fn add_stream(
    ost: &mut OutputStream,
    oc: &mut *mut ffi::AVFormatContext,
    codec: &mut *const ffi::AVCodec,
    codec_id: ffi::AVCodecID,
) {
    *codec = ffi::avcodec_find_encoder(codec_id);
    let c = ffi::avcodec_alloc_context3(codec.as_ref().unwrap());
    let mut enc_ctx = ost.enc_ctx.lock().unwrap();
    ost.st = ffi::avformat_new_stream(*oc, std::ptr::null_mut());
    let c = c.as_mut().unwrap();
    enc_ctx.borrow_mut().ptr = c;
    match codec.as_ref().unwrap().type_ {
        ffi::AVMediaType_AVMEDIA_TYPE_VIDEO => {
            c.codec_id = codec.as_ref().unwrap().id;
            c.codec_type = ffi::AVMediaType_AVMEDIA_TYPE_VIDEO;
            /* put sample parameters */
            c.bit_rate = 40000;
            /* resolution must be a multiple of two */
            c.width = 1280;
            c.height = 720;
            /* frames per second */
            c.time_base = ffi::AVRational{ num: 1, den: 25};
            c.framerate = ffi::AVRational{ num: 25, den: 1};

            c.gop_size = 10;
            c.max_b_frames = 1;
            c.pix_fmt = ffi::AVPixelFormat_AV_PIX_FMT_YUV420P;
            // c.profile = ffi::FF_PROFILE_H264_CONSTRAINED_BASELINE as _;
            // c.profile = ffi::FF_PROFILE_H264_MAIN as _;
            ost.st.as_mut().unwrap().time_base = ffi::AVRational{num: 1, den: 25};
            // TODO: set as a reference to ost.st?
            c.time_base = ffi::AVRational{num: 1, den: 25};
        }
        _ => {
            eprintln!("📽📽  unknnown codec type: {:?}", codec.as_ref().unwrap().type_);
        }
    }
}

unsafe fn open_video(
    oc: *const ffi::AVFormatContext,
    codec: &mut *const ffi::AVCodec,
    ost: &mut OutputStream,
) {
    let _ = ffi::avcodec_open2(ost.enc_ctx.lock().unwrap().ptr, *codec, std::ptr::null_mut());
    eprintln!("📽📽  opened codec: {:?}", codec);

    ffi::avcodec_parameters_from_context(ost.st.as_mut().unwrap().codecpar, ost.enc_ctx.lock().unwrap().ptr);
    // self.video_ctx = Arc::new(Mutex::new(CodecContextWrapper{
    //     ptr:  c
    // }));
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
