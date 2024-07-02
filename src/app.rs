#![allow(unused_variables)]
use std::collections::VecDeque;
use std::ffi::CStr;
use std::ffi::CString;
use std::ptr;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use rusty_ffmpeg::ffi;

use rusty_ffmpeg::ffi::AVFrame;
use rusty_ffmpeg::ffi::av_frame_free;

use crate::movie_state::movie_state_enqueue_frame;
use crate::movie_state::movie_state_enqueue_packet;
use crate::movie_state::CodecContextWrapper;
use crate::movie_state::FrameWrapper;
use crate::movie_state::MovieState;

#[cfg_attr(target_os="linux", path="platform/sdl.rs")]
mod platform;

// #[path="filter.rs"]
// mod filter;
// fn rotation_filter_init() -> crate::filter::RotateFilter {
//     unsafe {
//         crate::filter::RotateFilter {
//             filter_graph: ffi::avfilter_graph_alloc(),
//             buffersink_ctx: std::ptr::null_mut(),
//             buffersrc_ctx:  std::ptr::null_mut(),
//         }
//     }
// }

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
                unsafe {
                    println!(
                        "Video Codec: {} {:?}",
                        local_codec_params.codec_id,
                        match (*local_codec).long_name.is_null()  {
                            true => CStr::from_ptr((*local_codec).name),
                            false => CStr::from_ptr((*local_codec).long_name),
                        }
                    );
                }
            },
            _ => {}
        }
    }
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
pub unsafe extern "C" fn open_input(src: &str) -> (*const ffi::AVCodec, &mut ffi::AVFormatContext, &mut ffi::AVCodecContext) {
// unsafe {ffi::av_log_set_level(ffi::AV_LOG_DEBUG as i32)};
    let filepath: CString = CString::new(src).unwrap();
    let mut format_ctx = unsafe { ffi::avformat_alloc_context() };

    let format     = ptr::null_mut();
    let dict       = ptr::null_mut();
    if ffi::avformat_open_input(&mut format_ctx, filepath.as_ptr(), format, dict) != 0 {
        panic!("🚩 cannot open file")
    }
    let format_context = format_ctx.as_mut().unwrap();
    let format_name = CStr::from_ptr((*(*format_ctx).iformat).name)
        .to_str()
        .unwrap();

    if ffi::avformat_find_stream_info(format_context, ptr::null_mut()) < 0 {
        panic!("ERROR could not get the stream info");
    }
    ffi::av_dump_format(format_context, 0, filepath.as_ptr(), 0);

    let streams = std::slice::from_raw_parts(format_context.streams, format_context.nb_streams as usize);
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
                    match (*local_codec).long_name.is_null()  {
                        true => CStr::from_ptr((*local_codec).name),
                        false => CStr::from_ptr((*local_codec).long_name),
                    }
                );
            },
            _ => {}
        }
    }
    let codec_context = ffi::avcodec_alloc_context3(codec_ptr).as_mut().unwrap();

    if ffi::avcodec_parameters_to_context(codec_context, codec_parameters_ptr) < 0 {
        panic!("failed to copy codec params to codec context");
    }

    if ffi::avcodec_open2(codec_context, codec_ptr, ptr::null_mut()) < 0 {
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
 pub unsafe extern "C" fn play_movie(movie_state: *mut MovieState) {

    let mut movie_state = movie_state.as_mut().unwrap();
    let format_context = std::sync::Arc::clone(&movie_state.format_context);
    let codec_context = movie_state.video_ctx.lock().unwrap().ptr.as_ref().unwrap();
    let rotation = get_orientation_metadata_value((*format_context).lock().unwrap().ptr);
    // let mut rotate_filter = rotation_filter_init();
    // crate::filter::init_filter(
    //     rotation,
    //     &mut rotate_filter.filter_graph,
    //     &mut rotate_filter.buffersink_ctx,
    //     &mut rotate_filter.buffersrc_ctx,
    //     (codec_context.width, codec_context.height),
    //     codec_context.pix_fmt
    // );

    let (window_width, window_height): (u32, u32) = match rotation {
        90 => (codec_context.height as u32 , codec_context.width as u32 ),
        _  => (codec_context.width as u32 , codec_context.height as u32 )
    };
    // let (window_width, window_height): (u32, u32) = match rotation {
    //      90 => (450, 800),
    //     -90 => (450, 800),
    //     _  => (800, 450)
    // };
    // let frame = unsafe { ffi::av_frame_alloc().as_mut() }
    //     .expect("failed to allocated memory for AVFrame");
    // let packet = unsafe { ffi::av_packet_alloc().as_mut() }
    //     .expect("failed to allocated memory for AVPacket");
    let dest_frame =
        unsafe { ffi::av_frame_alloc().as_mut() }
        .expect("failed to allocated memory for AVFrame");

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let arc_format_context = std::sync::Arc::clone(&movie_state.format_context);
    let arc_video_ctx = std::sync::Arc::clone(&movie_state.video_ctx);
    let keep_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let pause_packets = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    std::thread::spawn(move|| {
        for msg in rx {
            println!("🦀🦀 received message: {}", msg);
        }
    });
    let packet_thread = packet_thread_spawner(
        arc_format_context,
        std::sync::Arc::clone(&pause_packets),
        std::sync::Arc::clone(&keep_running),
        movie_state.video_stream_idx,
        std::sync::Arc::clone(&movie_state.videoqueue),
    );

    let i = 0;
    let last_pts = 0;
    let last_clock = ffi::av_gettime_relative();
    // let videoqueue =  movie_state.videoqueue.clone();
    // let video_ctx =  movie_state.video_ctx.clone();
    // let picq =  movie_state.picq.clone();
    let videoqueue     = std::sync::Arc::clone(&movie_state.videoqueue);
    let picq           = std::sync::Arc::clone(&movie_state.picq);
    let video_ctx      = std::sync::Arc::clone(&movie_state.video_ctx);
    let pause_packets3 = std::sync::Arc::clone(&pause_packets);
    let keep_running3  = std::sync::Arc::clone(&keep_running);
    let decode_thread  = std::thread::spawn(move || {
    let frame = ffi::av_frame_alloc().as_mut()
        .expect("failed to allocated memory for AVFrame");
        loop {
        unsafe {
            let mut locked_videoqueue = videoqueue.lock().unwrap();
            if let Some(packet) = locked_videoqueue.front_mut() {
                // !Note that AVPacket.pts is in AVStream.time_base units, not AVCodecContext.time_base units.
                // let mut delay:f64 = packet.ptr.as_ref().unwrap().pts as f64 - last_pts as f64;
                // last_pts = packet.ptr.as_ref().unwrap().pts;
                if let Ok(_) = decode_packet(packet.ptr, video_ctx.clone(), frame) {
                    {
                        // let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
                        // delay *= (time_base.num as f64) / (time_base.den as f64);
                    }

                    while let Err(_) = movie_state_enqueue_frame(&picq, frame) {
                        ::std::thread::yield_now();
                        ::std::thread::sleep(Duration::from_millis(4));
                        if ! keep_running3.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                    }

                } else {
                    ffi::av_freep(frame as *mut _ as *mut _);
                }
                ffi::av_packet_unref(packet.ptr);
                locked_videoqueue.pop_front();
            }
            ::std::thread::yield_now();
            if ! keep_running3.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            if pause_packets3.load(std::sync::atomic::Ordering::Relaxed) {
                ::std::thread::park();
            }
        }
        };
    });


    let mut subsystem = match platform::init_subsystem(window_width, window_height) {
        Ok(s) => s,
        Err(e) => {
            unsafe { av_frame_free(&mut (dest_frame as *mut _)) };
            keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
            decode_thread.join().unwrap();
            packet_thread.join().unwrap();
            return;
        }
    };
    platform::event_loop(&mut movie_state, &mut subsystem, tx);

    unsafe { av_frame_free(&mut (dest_frame as *mut _)) };
    keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
    decode_thread.join().unwrap();
    packet_thread.join().unwrap();
}

fn decode_packet(
    packet: *mut ffi::AVPacket,
    arc_codec_context: Arc<Mutex<CodecContextWrapper>>,
    frame: &mut ffi::AVFrame,
) -> Result<(), String> {
    let lock = arc_codec_context.try_lock();
    if let Err(e) = lock {
        return Err(String::from("Error while locking the codec context."));
    }
    let codec_context = unsafe {lock.unwrap().ptr.as_mut().unwrap()};
    let mut response = unsafe { ffi::avcodec_send_packet(codec_context, packet) };

    if response < 0 {
        eprintln!("Error while sending a packet to the decoder. {:?}", ffi::av_err2str(response));
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
        // let codec_context = unsafe{codec_context.as_ref().unwrap()};
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
        frame.format = codec_context.pix_fmt;
        return Ok(());
    }
    Ok(())
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

fn packet_thread_spawner(
    arc_format_context: std::sync::Arc<std::sync::Mutex<crate::movie_state::FormatContextWrapper>>,
    pause_packets: std::sync::Arc<std::sync::atomic::AtomicBool>,
    keep_running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    video_stream_idx: i64,
    videoqueue: std::sync::Arc<std::sync::Mutex<VecDeque<crate::movie_state::PacketWrapper>>>
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move|| {
        loop {
            if !keep_running.load(std::sync::atomic::Ordering::Relaxed) {
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
                keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
                // break 'running;
            }

            if response < 0 {
                println!("{}", String::from(
                    "ERROR",
                ));
                // *keep_running2.get_mut() = false;
                keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
                return;
                // break 'running;
            }
            {
                if video_stream_idx == packet.stream_index as i64 {
                    while let Err(_) = movie_state_enqueue_packet(&videoqueue, packet) {
                        // ::std::thread::sleep(Duration::from_millis(4));
                        ::std::thread::yield_now();
                        if !keep_running.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                    }
                    // ::std::thread::sleep(Duration::from_millis(33));
                } else {
                    ffi::av_packet_unref(packet);
                }
            }
            if pause_packets.load(std::sync::atomic::Ordering::Relaxed) {
                ::std::thread::park();
            }
            ::std::thread::yield_now();
        }
        };
    })
}
