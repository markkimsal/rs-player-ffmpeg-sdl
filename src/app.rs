#![allow(unused_imports, unused_variables, unused_mut, dead_code)]
use std::borrow::Borrow;
use std::borrow::BorrowMut;
use std::ffi::CStr;
use std::ffi::CString;
use std::ops::Deref;
use std::ops::DerefMut;
use std::panic::panic_any;
use std::ptr;
use std::ptr::NonNull;
use std::rc::Rc;
use std::slice;
use std::sync::Mutex;
use std::time::Duration;
use rusty_ffmpeg::ffi;

use libc::{size_t, c_int};
use rusty_ffmpeg::ffi::AVFrame;
use rusty_ffmpeg::ffi::AVPictureType_AV_PICTURE_TYPE_P;
use rusty_ffmpeg::ffi::AVPixelFormat_AV_PIX_FMT_ARGB;
use rusty_ffmpeg::ffi::AVPixelFormat_AV_PIX_FMT_YUV410P;
use rusty_ffmpeg::ffi::AVPixelFormat_AV_PIX_FMT_YUV420P;
use rusty_ffmpeg::ffi::AVPixelFormat_AV_PIX_FMT_YUVJ420P;
use rusty_ffmpeg::ffi::AV_TIME_BASE_Q;
use rusty_ffmpeg::ffi::SWS_BILINEAR;
use rusty_ffmpeg::ffi::SwsContext;
use rusty_ffmpeg::ffi::av_frame_free;
use rusty_ffmpeg::ffi::av_freep;
use rusty_ffmpeg::ffi::sws_getCachedContext;
use rusty_ffmpeg::ffi::sws_getContext;
use rusty_ffmpeg::ffi::sws_freeContext;
use rusty_ffmpeg::ffi::sws_scale;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{Canvas, Texture, TextureAccess};
use sdl2::sys::SDL_PixelFormat;
use sdl2::sys::SDL_PixelFormatEnum;
use sdl2::sys::SDL_RenderCopy;
use sdl2::sys::SDL_Texture;
use sdl2::sys::SDL_TextureAccess;
use sdl2::sys::SDL_UpdateTexture;
use sdl2::sys::SDL_UpdateYUVTexture;
use sdl2::video::Window;

use crate::filter::RotateFilter;
use crate::movie_state;
use crate::movie_state::CodecContextWrapper;
use crate::movie_state::MovieState;

// #[path="filter.rs"]
// mod filter;
fn rotation_filter_init() -> crate::filter::RotateFilter {
    unsafe {
        crate::filter::RotateFilter {
            filter_graph: ffi::avfilter_graph_alloc(),
            buffersink_ctx: std::ptr::null_mut(),
            buffersrc_ctx:  std::ptr::null_mut(),
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn new_movie_state() -> *mut MovieState {
    Box::into_raw(Box::new(MovieState::new())) as *mut MovieState
}
#[no_mangle]
pub unsafe extern "C" fn drop_movie_state(movie_state: *mut MovieState) {
    drop(Box::<MovieState>::from_raw(movie_state));
}

#[no_mangle]
pub unsafe extern "C" fn a_function_from_rust() -> i32 {
    42
}
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn open_movie(filepath: *const libc::c_char, video_state: &mut MovieState) {
    // let filepath: CString = CString::new(src).unwrap();
    let mut format_ctx = ffi::avformat_alloc_context();

    let format     = ptr::null_mut();
    let dict       = ptr::null_mut();
    if {
        ffi::avformat_open_input(&mut format_ctx, filepath, format, dict)
    } != 0 {
        panic!("🚩 cannot open file")
    }

    if ffi::avformat_find_stream_info(format_ctx, ptr::null_mut()) < 0 {
        panic!("ERROR could not get the stream info");
    }
    video_state.set_format_context(format_ctx.as_mut().unwrap());

    ffi::av_dump_format(video_state.format_context.lock().unwrap().ptr, 0, filepath, 0);

    let streams = {
        let format_ctx = video_state.format_context.lock().unwrap();
        std::slice::from_raw_parts(format_ctx.as_ref().unwrap().streams, format_ctx.as_ref().unwrap().nb_streams as usize)
    };
    let mut codec_ptr: *const ffi::AVCodec = ptr::null_mut();
    let mut codec_parameters_ptr: *const ffi::AVCodecParameters = ptr::null_mut();
    let mut video_stream_index = None;
    let mut time_base_den:i32 = 10000;
    let mut time_base_num:i32 = 10000;

    for s in streams
        .iter()
        .map(|stream| *stream)
        .enumerate()
    {
        let (i, stream): (usize, *mut ffi::AVStream) = s;
        println!(
            "AVStream->time_base before open codec {}/{}",
            (*stream).time_base.num, (*stream).time_base.den
        );

        let local_codec_params = (*stream).codecpar.as_ref()
            .expect("ERROR: unable to dereference codec parameters");
        let local_codec = ffi::avcodec_find_decoder(local_codec_params.codec_id).as_ref()
            .expect("ERROR unsupported codec!");

        match local_codec_params.codec_type {
            ffi::AVMediaType_AVMEDIA_TYPE_VIDEO => {

                if video_stream_index.is_none() {

                    video_stream_index = Some(i);
                    video_state.video_stream.lock().unwrap().ptr = stream;
                    video_state.video_stream_idx = i as i64;
                    video_state.video_ctx = std::sync::Arc::new(Mutex::new(
                        CodecContextWrapper{ptr: ffi::avcodec_alloc_context3(local_codec)}
                    )); //.as_mut().unwrap();
                    codec_ptr = local_codec;
                    codec_parameters_ptr = local_codec_params;
                    time_base_den = (*stream).time_base.den;
                    time_base_num = (*stream).time_base.num;
                }

                println!(
                    "Video Codec: resolution {} x {}",
                    local_codec_params.width, local_codec_params.height
                );
                println!(
                    "Video Codec: {} {:?}",
                    local_codec_params.codec_id,
                    unsafe {
                        match (*local_codec).long_name.is_null()  {
                            true => CStr::from_ptr((*local_codec).name),
                            false => CStr::from_ptr((*local_codec).long_name),
                        }
                    },
                );
            },
            _ => {}
        }
    }
//    let codec_context = unsafe { ffi::avcodec_alloc_context3(codec_ptr).as_mut() }.unwrap();

    if unsafe { ffi::avcodec_parameters_to_context((video_state.video_ctx.lock().unwrap()).ptr, codec_parameters_ptr) } < 0 {
        panic!("failed to copy codec params to codec context");
    }
    if ffi::avcodec_open2((video_state.video_ctx.lock().unwrap()).ptr, codec_ptr, ptr::null_mut()) < 0 {
        panic!("failed to open codec through avcodec_open2");
    }

    // let format_ctx = video_state.format_context.lock().unwrap();
    let mut dur_s = format_ctx.as_ref().unwrap().duration / 10000;
    let dur_min = dur_s  / 6000; // (60 * time_base_den as i64);
    // let dur_min = dur_s  /  (60 / time_base_den as i64);
    dur_s -= dur_min * 6000; // (60 * time_base_den as i64);

    let format_name = unsafe { CStr::from_ptr((*(*format_ctx).iformat).name) }
        .to_str()
        .unwrap();
    println!(
        "format {}, duration {:0>3}:{:0>2}, time_base {}/{}",
        format_name, dur_min, dur_s / 100 , time_base_num, time_base_den
    );
}

#[allow(improper_ctypes_definitions)]
pub extern "C" fn open_input(src: &str) -> (*const ffi::AVCodec, &mut ffi::AVFormatContext, &mut ffi::AVCodecContext) {
// unsafe {ffi::av_log_set_level(ffi::AV_LOG_DEBUG as i32)};
    let filepath: CString = CString::new(src).unwrap();
    let mut format_ctx = unsafe { ffi::avformat_alloc_context() };

    let format     = ptr::null_mut();
    let dict       = ptr::null_mut();
    if unsafe {
        ffi::avformat_open_input(&mut format_ctx, filepath.as_ptr(), format, dict)
    } != 0 {
        panic!("🚩 cannot open file")
    }
    let format_context = unsafe { format_ctx.as_mut() }.unwrap();
    let format_name = unsafe { CStr::from_ptr((*(*format_ctx).iformat).name) }
        .to_str()
        .unwrap();

    if unsafe { ffi::avformat_find_stream_info(format_context, ptr::null_mut()) } < 0 {
        panic!("ERROR could not get the stream info");
    }
    unsafe { ffi::av_dump_format(format_context, 0, filepath.as_ptr(), 0) };

    let streams = unsafe {
        std::slice::from_raw_parts(format_context.streams, format_context.nb_streams as usize)
    };
    let mut codec_ptr: *const ffi::AVCodec = ptr::null_mut();
    let mut codec_parameters_ptr: *const ffi::AVCodecParameters = ptr::null_mut();
    let mut video_stream_index = None;
    let mut time_base_den:i32 = 10000;
    let mut time_base_num:i32 = 10000;

    for s in streams
        .iter()
        .map(|stream| unsafe { stream.as_ref() }.unwrap())
        .enumerate()
    {
        let (i, &stream): (usize, &ffi::AVStream) = s;
        println!(
            "AVStream->time_base before open codec {}/{}",
            stream.time_base.num, stream.time_base.den
        );

        let local_codec_params = unsafe { stream.codecpar.as_ref() }
            .expect("ERROR: unable to dereference codec parameters");
        let local_codec = unsafe { ffi::avcodec_find_decoder(local_codec_params.codec_id).as_ref() }
            .expect("ERROR unsupported codec!");

        match local_codec_params.codec_type {
            ffi::AVMediaType_AVMEDIA_TYPE_VIDEO => {
                if video_stream_index.is_none() {
                    video_stream_index = Some(i);
                    codec_ptr = local_codec;
                    codec_parameters_ptr = local_codec_params;
                    time_base_den = stream.time_base.den;
                    time_base_num = stream.time_base.num;
                }

                println!(
                    "Video Codec: resolution {} x {}",
                    local_codec_params.width, local_codec_params.height
                );
                println!(
                    "Video Codec: {} {:?}",
                    local_codec_params.codec_id,
                    unsafe {
                        match (*local_codec).long_name.is_null()  {
                            true => CStr::from_ptr((*local_codec).name),
                            false => CStr::from_ptr((*local_codec).long_name),
                        }
                    },
                );
            },
            _ => {}
        }
    }
    let codec_context = unsafe { ffi::avcodec_alloc_context3(codec_ptr).as_mut() }.unwrap();

    if unsafe { ffi::avcodec_parameters_to_context(codec_context, codec_parameters_ptr) } < 0 {
        panic!("failed to copy codec params to codec context");
    }

    if unsafe { ffi::avcodec_open2(codec_context, codec_ptr, ptr::null_mut()) } < 0 {
        panic!("failed to open codec through avcodec_open2");
    }
    let mut dur_s = format_context.duration / time_base_den as i64;
    let dur_min = dur_s  / 6000; // (60 * time_base_den as i64);
    dur_s -= dur_min * 6000; // (60 * time_base_den as i64);
    println!(
        "format {}, duration {:0>3}:{:0>2}, time_base {} /{}",
        format_name, dur_min, dur_s / 100 , time_base_num, time_base_den
    );
    (codec_ptr, format_context, codec_context)
}
#[repr(C)]
struct Storage<'m> {
    // ptr: *mut ffi::AVFormatContext,
    ptr: &'m MovieState
}
unsafe impl Send for Storage<'_>{}

#[no_mangle]
#[allow(improper_ctypes_definitions)]
 pub unsafe extern "C" fn play_movie(movie_state: *const MovieState) {

    let movie_state = movie_state.as_ref().unwrap();
    let format_context = std::sync::Arc::clone(&movie_state.format_context);
    // let codec_context = unsafe {movie_state.video_ctx.as_mut().unwrap()};
    let codec_context = unsafe {movie_state.video_ctx.lock().unwrap().ptr.as_ref().unwrap()};
    let rotation = unsafe { get_orientation_metadata_value((*format_context).lock().unwrap().ptr) };
    let mut rotate_filter = rotation_filter_init();
    crate::filter::init_filter(
        rotation,
        &mut rotate_filter.filter_graph,
        &mut rotate_filter.buffersink_ctx,
        &mut rotate_filter.buffersrc_ctx,
        (codec_context.width, codec_context.height),
        codec_context.pix_fmt
    );

    // let (window_width, window_height): (u32, u32) = match rotation {
    //     90 => (codec_context.height as u32 / 2, codec_context.width as u32 / 2),
    //     _  => (codec_context.width as u32 / 2, codec_context.height as u32 / 2)
    // };
    let (window_width, window_height): (u32, u32) = match rotation {
         90 => (450, 800),
        -90 => (450, 800),
        _  => (800, 450)
    };
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();

    let window = video_subsystem.window("rs-player-ffmpeg-sdl2", window_width, window_height)
        .position_centered()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().build().unwrap();

    canvas.set_draw_color(Color::RGB(0, 255, 255));
    canvas.clear();
    canvas.present();
    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator.create_texture(
        Some(PixelFormatEnum::ARGB32),
        TextureAccess::Streaming,
        // 800,450
        // 450,800
        window_width,
        window_height
    ).unwrap();
    // let frame = unsafe { ffi::av_frame_alloc().as_mut() }
    //     .expect("failed to allocated memory for AVFrame");
    // let packet = unsafe { ffi::av_packet_alloc().as_mut() }
    //     .expect("failed to allocated memory for AVPacket");
    let dest_frame =
        unsafe { ffi::av_frame_alloc().as_mut() }
        .expect("failed to allocated memory for AVFrame");

    // we are going to rotate before scale, so input w/h needs to be flipped depending
    // on rotation flags
    let sws_ctx = unsafe { sws_getContext(
        match rotation { 90 => codec_context.height, _ => codec_context.width},
        match rotation { 90 => codec_context.width, _ => codec_context.height},
        AVPixelFormat_AV_PIX_FMT_YUV420P,
        window_width as i32,
        window_height as i32,
        AVPixelFormat_AV_PIX_FMT_ARGB,
        SWS_BILINEAR as i32,
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
    ) };

    let frame = unsafe {ffi::av_frame_alloc().as_mut()}
        .expect("failed to allocated memory for AVFrame");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    // let rc_movie_state = Rc::clone(&movie_state);
    // let format_context = std::sync::Arc::new(Mutex::new(movie_state.format_context));
    let arc_format_context = std::sync::Arc::clone(&movie_state.format_context);
    // let video_ctx = std::sync::Arc::new(Mutex::new(movie_state.video_ctx));
    let arc_video_ctx = std::sync::Arc::clone(&movie_state.video_ctx);
    let keep_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let keep_running2 = std::sync::Arc::clone(&keep_running);
    let keep_running3 = std::sync::Arc::clone(&keep_running);
    let pause_packets = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let pause_packets2 = std::sync::Arc::clone(&pause_packets);
    let pause_packets3 = std::sync::Arc::clone(&pause_packets);
    let movie_state = std::sync::Arc::new(movie_state);
    let arc_movie_state = std::sync::Arc::clone(&movie_state);

    // std::thread::spawn(move|| {
    //     for msg in rx {
    //         println!("received message: {}", msg);
    //     }
    // });
    let packet_thread = std::thread::spawn(move|| {
        loop {
            if !keep_running2.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
        unsafe {
            let packet = ffi::av_packet_alloc().as_mut()
                .expect("failed to allocated memory for AVPacket");
            let response = ffi::av_read_frame((*(arc_format_context.lock().unwrap())).ptr, packet);
            // if response == ffi::AVERROR(ffi::EAGAIN) || response == ffi::AVERROR_EOF {
            if response == ffi::AVERROR_EOF {
                println!("{}", String::from(
                    "EOF",
                ));
                // *keep_running2.get_mut() = false;
                keep_running2.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
                // break 'running;
            }

            if response < 0 {
                println!("{}", String::from(
                    "ERROR",
                ));
                // *keep_running2.get_mut() = false;
                keep_running2.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
                // break 'running;
            }
            {
                if arc_movie_state.video_stream_idx == packet.stream_index as i64 {
                    while let Err(_) = arc_movie_state.enqueue_packet(packet) {
                        // ::std::thread::sleep(Duration::from_millis(4));
                        ::std::thread::yield_now();
                        if !keep_running2.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                    }
                    // ::std::thread::sleep(Duration::from_millis(33));
                } else {
                    ffi::av_packet_unref(packet);
                }
            }
            if pause_packets2.load(std::sync::atomic::Ordering::Relaxed) {
                ::std::thread::park();
            }
       }
        };
    });
    let mut event_pump = sdl_context.event_pump().unwrap();
    let mut i = 0;
    let mut last_pts = 0;
    let mut last_clock = ffi::av_gettime_relative();
    'running: loop {
        i = (i + 1) % 255;
        // canvas.set_draw_color(Color::RGB(i, 64, 255 - i));
        // canvas.clear();
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit {..} |
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
                    break 'running
                },
                Event::KeyDown { keycode: Some(Keycode::Space), .. } => {
                    pause_packets.store(!pause_packets.load(std::sync::atomic::Ordering::Relaxed), std::sync::atomic::Ordering::Relaxed);
                    packet_thread.thread().unpark();
                },

                _ => {}
            }
        }
        // The rest of the game loop goes here...

        if pause_packets3.load(std::sync::atomic::Ordering::Relaxed) {
            continue;
        }

        if keep_running.load(std::sync::atomic::Ordering::Relaxed) == false {
            break 'running;
        }

        unsafe {
            let mut locked_videoqueue = movie_state.videoqueue.lock().unwrap();
            if let Some(packet) = locked_videoqueue.front_mut() {
                // !Note that AVPacket.pts is in AVStream.time_base units, not AVCodecContext.time_base units.
                let mut delay:f64 = packet.ptr.as_ref().unwrap().pts as f64 - last_pts as f64;
                last_pts = packet.ptr.as_ref().unwrap().pts;
                if let Ok(_) = decode_packet(packet.ptr, movie_state.video_ctx.lock().unwrap().ptr.as_mut().unwrap(), frame) {

                    {
                        let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
                        delay *= (time_base.num as f64) / (time_base.den as f64);
                    }
                    blit_frame(frame, dest_frame, &mut canvas, &mut texture, sws_ctx, &rotate_filter).unwrap_or_default();
                }
                ffi::av_packet_free(&mut (packet.ptr));
                locked_videoqueue.pop_front();

                // println!("av_gettime_relative: {}", (ffi::av_gettime_relative() - last_clock ) );
                delay -= (ffi::av_gettime_relative() - last_clock ) as f64 / 1_000_000.0;
                // TODO: less than 2 * FPS
                if delay > 0.0 && delay < 1.0 {
                    ::std::thread::sleep(Duration::from_secs_f64(delay));
                }
                last_clock = ffi::av_gettime_relative();
            }
        }

        canvas.present();
    }
    unsafe { av_frame_free(&mut (dest_frame as *mut _)) };
    unsafe { sws_freeContext(sws_ctx as *mut _) };

    packet_thread.join().unwrap();
}

fn decode_packet(
    packet: *mut ffi::AVPacket,
    codec_context: *mut ffi::AVCodecContext,
    frame: &mut ffi::AVFrame,
) -> Result<(), String> {
    let mut response = unsafe { ffi::avcodec_send_packet(codec_context, packet) };

    if response < 0 {
        return Err(String::from("Error while sending a packet to the decoder."));
    }
    while response >= 0 {
        response = unsafe { ffi::avcodec_receive_frame(codec_context, frame) };
        if response == ffi::AVERROR(ffi::EAGAIN) || response == ffi::AVERROR_EOF {
            return Err(String::from(
                "EAGAIN",
            ));
            // break;
        } else if response < 0 {
            return Err(String::from(
                "Error while receiving a frame from the decoder.",
            ));
        }
        let codec_context = unsafe{codec_context.as_ref().unwrap()};
        println!(
            "Frame {} (type={}, size={} bytes) pts {} key_frame {} [DTS {}]",
            codec_context.frame_number,
            unsafe { ffi::av_get_picture_type_char(frame.pict_type) },
            frame.pkt_size,
            // frame.pts * codec_context.time_base.num as i64 / codec_context.time_base.den as i64,
            unsafe {ffi::av_rescale_q(frame.pts, codec_context.time_base, ffi::AVRational { num: 1, den: 1 })},
            frame.key_frame,
            frame.pkt_dts
        );
        return Ok(());
    }
    Ok(())
}


fn blit_frame(
    src_frame: &mut ffi::AVFrame,
    dest_frame: &mut ffi::AVFrame,
    canvas: &mut Canvas<Window>,
    texture: &mut Texture,
    sws_ctx: *mut SwsContext,
    filter: &crate::filter::RotateFilter,
) -> Result<(), String> {

        let  new_frame = frame_thru_filter(filter, src_frame);

        // dest_frame.width  = new_frame.width;
        // dest_frame.height = new_frame.height;
        dest_frame.width  = canvas.window().size().0 as i32;
        dest_frame.height = canvas.window().size().1 as i32;
        dest_frame.format = AVPixelFormat_AV_PIX_FMT_ARGB;

        unsafe {
            ffi::av_frame_get_buffer(dest_frame, 0);
             sws_scale(
                sws_ctx,
                new_frame.data.as_ptr() as _,
                new_frame.linesize.as_ptr(),
                0,
                new_frame.height,
                // codec_context.height,
                dest_frame.data.as_mut_ptr(),
                dest_frame.linesize.as_mut_ptr()
            )
        };

    let new_frame = dest_frame;
    // unsafe { SDL_UpdateTexture(
    //     texture.raw(), ptr::null(),
    //     (*dest_frame).data[0] as _, (*dest_frame).linesize[0] as _
    // ) };
    unsafe { SDL_UpdateTexture(
        texture.raw(), ptr::null(),
        (new_frame).data[0] as _, (new_frame).linesize[0] as _
    ) };

    // SDL cannot handle YUV(J)420P
    // unsafe { SDL_UpdateYUVTexture(
    //     texture.raw(), ptr::null(),
    //     dest_frame.data[0], dest_frame.linesize[0],
    //     dest_frame.data[1], dest_frame.linesize[1],
    //     dest_frame.data[2], dest_frame.linesize[2],
    // ) };
    canvas.copy(texture, None, None)
    // unsafe { SDL_RenderCopy(canvas, texture.raw(), ptr::null(), ptr::null()) }
}


fn frame_thru_filter(filter: &crate::filter::RotateFilter, frame: &mut AVFrame) -> AVFrame
{
    let filt_frame =
        unsafe { ffi::av_frame_alloc().as_mut() }
        .expect("failed to allocated memory for AVFrame");

    filt_frame.width  = frame.width;
    filt_frame.height = frame.height;
    filt_frame.format = frame.format;
    unsafe { ffi::av_frame_get_buffer(filt_frame, 0) };

	let result = unsafe { ffi::av_buffersrc_add_frame(filter.buffersrc_ctx, frame) };
    if result < 0 {
        if result == ffi::AVERROR_INVALIDDATA {
            eprintln!("Invalid data while feeding the filtergraph.");
        }
        eprintln!("{}", ffi::av_err2str(result));
        return *filt_frame;
    }

    loop {
        unsafe {
            let result =  ffi::av_buffersink_get_frame(filter.buffersink_ctx, filt_frame);
            // if result == ffi::AVERROR(ffi::EOF)  { break; }
            if result != ffi::AVERROR(ffi::EAGAIN)  { break; }
        }
    }

	return *filt_frame;
}



unsafe fn get_orientation_metadata_value(format_ctx: *mut ffi::AVFormatContext) -> i32 {
    let key_name = CString::new("rotate").unwrap();
	let tag: *mut ffi::AVDictionaryEntry = ffi::av_dict_get(
        (*format_ctx).metadata,
        key_name.as_ptr() as *const _,
        std::ptr::null(),
        0
    );
	if !tag.is_null() {
		return libc::atoi((*tag).value);
	}
    eprintln!(" 🔄 got no rotation tag.");
    // let streams = NonNull::<ffi::AVStream>::new((*format_ctx).streams as *mut _).unwrap();
    eprintln!(" 🔄 nb_streams ptr is {:?}", (*format_ctx).nb_streams);
    let mut rotation = 0.;
    for i in 0..(*format_ctx).nb_streams as usize {
        unsafe {
            let mut _ptr = NonNull::new((*format_ctx).streams as *mut _).unwrap();
            let stream_ptr = ((*format_ctx).streams as *mut *mut ffi::AVStream).add(i);
            // let s = Box::<ffi::AVStream>::from_raw(*_ptr.as_ptr());
            let s = Box::<ffi::AVStream>::from_raw(*stream_ptr);
            eprintln!(" 🔄 streams nb_side_data is {:?}",s.nb_side_data);
            if !s.side_data.is_null() {
                let _display_matrix = ffi::av_stream_get_side_data(
                    Box::into_raw(s) as *const _,
                    ffi::AVPacketSideDataType_AV_PKT_DATA_DISPLAYMATRIX,
                    std::ptr::null_mut()
                );
                eprintln!(" 🔄 displaymatrix is {:?}", _display_matrix);
                rotation = -ffi::av_display_rotation_get(_display_matrix as *const i32);
                eprintln!(" 🔄 rotation is {:?}", rotation);
            } else {
                // consume the box
                let unptr = Box::into_raw(s);
                std::ptr::drop_in_place(unptr);
            }
            return rotation as i32;
        }
    }
    0
}
