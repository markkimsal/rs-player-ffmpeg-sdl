use std::ffi::CStr;
use std::ffi::CString;
use ::std::ops::DerefMut;
use std::ptr;
use std::ptr::NonNull;
use ::std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::sync::Mutex;
use ::std::thread::JoinHandle;
use log::debug;
use log::info;
use rusty_ffmpeg::ffi;


use crate::analyzer_state::AnalyzerContext;
use crate::decode_thread::decode_thread;
use crate::movie_state::movie_state_enqueue_packet;
use crate::movie_state::CodecContextWrapper;
use crate::movie_state::MovieState;

static mut DECODE_THREADS: Vec<Box<JoinHandle<()>>> = vec![];
static mut PACKET_THREADS: Vec<Box<JoinHandle<()>>> = vec![];

// #[cfg_attr(target_os="linux", path="platform/sdl.rs")]
// mod platform;

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
pub unsafe extern "C" fn open_movie(analyzer_context: &mut AnalyzerContext, filepath: *const libc::c_char) {
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
    let mut video_state = MovieState::new();
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
                    video_state.video_ctx = Mutex::new(
                        CodecContextWrapper{ptr: ffi::avcodec_alloc_context3(local_codec)}
                    ); //.as_mut().unwrap();
                    codec_ptr = local_codec;
                    codec_parameters_ptr = local_codec_params;
                    time_base_den = (*stream).time_base.den;
                    time_base_num = (*stream).time_base.num;
                    video_state.video_frame_rate = ffi::av_guess_frame_rate(
                        format_ctx.as_mut().unwrap(),
                        video_state.video_stream.lock().unwrap().ptr,
                        ::std::ptr::null_mut(),
                    );
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
    analyzer_context.add_movie_state(video_state);

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

#[repr(C)]
struct Storage<'m> {
    // ptr: *mut ffi::AVFormatContext,
    ptr: &'m MovieState
}
unsafe impl Send for Storage<'_>{}

#[no_mangle]
#[allow(improper_ctypes_definitions)]
/// play all movies attached to the analyzer
pub unsafe extern "C" fn start_analyzer(analyzer_ctx: *mut AnalyzerContext) -> Sender<String> {
    let analyzer_ctx = analyzer_ctx.as_mut().unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let keep_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));

    for movie_state in analyzer_ctx.movie_list.iter_mut() {
        // let movie_state = analyzer_ctx.movie_list.get_mut(0).unwrap();
        let movie_state_arc  = std::sync::Arc::new(movie_state);
        let movie_state1     = std::sync::Arc::clone(&movie_state_arc);

        let keep_running2  = std::sync::Arc::clone(&keep_running);
        PACKET_THREADS.push(
            Box::new(std::thread::spawn(move || packet_thread_spawner(
                std::sync::Arc::clone(&keep_running2),
                movie_state1.video_stream_idx,
                movie_state1,
            )))
        );

        let keep_running3  = std::sync::Arc::clone(&keep_running);
        let movie_state2   = std::sync::Arc::clone(&movie_state_arc);
        DECODE_THREADS.push(
            Box::new(std::thread::spawn(move || {
                decode_thread(movie_state2, keep_running3)
            }))
        );
    }

    std::thread::spawn(move || {
        // when all tx refs are dropped, this rx will close
        for msg in rx {
            debug!("🦀🦀 received message: {}", msg);
            if msg == "quit" {
               break;
            }
        }
        info!("🦀🦀 done");
        keep_running.store(false, std::sync::atomic::Ordering::Relaxed);

        for (index, _) in DECODE_THREADS.iter().enumerate() {
            let cur_thread = DECODE_THREADS.remove(index);
            let _ = cur_thread.join();
        }
        for (index, _) in PACKET_THREADS.iter().enumerate() {
            let cur_thread = PACKET_THREADS.remove(index);
            cur_thread.join().unwrap();
        }
    });
    tx
}


#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn play_movie(analyzer_ctx: *mut AnalyzerContext) -> Sender<String> {

    let analyzer_ctx = analyzer_ctx.as_mut().unwrap();
    let movie_state = analyzer_ctx.movie_list.get_mut(0).unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let keep_running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let movie_state_arc    = std::sync::Arc::new(movie_state);
    let movie_state1   = std::sync::Arc::clone(&movie_state_arc);

    let keep_running2  = std::sync::Arc::clone(&keep_running);

    PACKET_THREADS.push(
        Box::new(std::thread::spawn(move || packet_thread_spawner(
            std::sync::Arc::clone(&keep_running2),
            movie_state1.video_stream_idx,
            movie_state1,
        )))
    );


    let keep_running3  = std::sync::Arc::clone(&keep_running);
    let movie_state2   = std::sync::Arc::clone(&movie_state_arc);
    DECODE_THREADS.push(
        Box::new(std::thread::spawn(move || {
            decode_thread(movie_state2, keep_running3)
        }))
    );

    std::thread::spawn(move || {
        // when all tx refs are dropped, this rx will close
        for msg in rx {
            debug!("🦀🦀 received message: {}", msg);
            if msg == "quit" {
               break;
            }
        }
        info!("🦀🦀 done");
        keep_running.store(false, std::sync::atomic::Ordering::Relaxed);

        for (index, _) in DECODE_THREADS.iter().enumerate() {
            let cur_thread = DECODE_THREADS.remove(index);
            let _ = cur_thread.join();
        }
        for (index, _) in PACKET_THREADS.iter().enumerate() {
            let cur_thread = PACKET_THREADS.remove(index);
            cur_thread.join().unwrap();
        }
    });
    tx
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
    info!(" 🔄 got no rotation tag.");
    // let streams = NonNull::<ffi::AVStream>::new((*format_ctx).streams as *mut _).unwrap();
    info!(" 🔄 nb_streams ptr is {:?}", (*format_ctx).nb_streams);
    let mut rotation = 0.;
    for i in 0..(*format_ctx).nb_streams as usize {
        unsafe {
            let mut _ptr = NonNull::new((*format_ctx).streams as *mut _).unwrap();
            let stream_ptr = ((*format_ctx).streams as *mut *mut ffi::AVStream).add(i);
            // let s = Box::<ffi::AVStream>::from_raw(*_ptr.as_ptr());
            let s = Box::<ffi::AVStream>::from_raw(*stream_ptr);
            info!(" 🔄 streams nb_side_data is {:?}",s.nb_side_data);
            if !s.side_data.is_null() {
                let _display_matrix = ffi::av_stream_get_side_data(
                    Box::into_raw(s) as *const _,
                    ffi::AVPacketSideDataType_AV_PKT_DATA_DISPLAYMATRIX,
                    std::ptr::null_mut()
                );
                info!(" 🔄 displaymatrix is {:?}", _display_matrix);
                rotation = -ffi::av_display_rotation_get(_display_matrix as *const i32);
                info!(" 🔄 rotation is {:?}", rotation);
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
    keep_running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    video_stream_idx: i64,
    movie_state: Arc<&mut MovieState>
) {
    loop {
        if !keep_running.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        unsafe {
            let packet = ffi::av_packet_alloc().as_mut()
                .expect("failed to allocated memory for AVPacket");
            let response = ffi::av_read_frame(movie_state.format_context.lock().unwrap().ptr, packet);
            // if response == ffi::AVERROR(ffi::EAGAIN) || response == ffi::AVERROR_EOF {
            if response == ffi::AVERROR_EOF {
                println!("{}", String::from(
                    "EOF",
                ));
                let seek_ret = ffi::av_seek_frame(movie_state.format_context.lock().unwrap().ptr, movie_state.video_stream_idx as i32, 0, ffi::AVSEEK_FLAG_BACKWARD as i32);
                if seek_ret < 0 {
                    eprintln!("📽📽  failed to seek backwards: ");
                    keep_running.store(false, std::sync::atomic::Ordering::Relaxed);
                    return;
                }
                eprintln!("rewind to {}",
                    seek_ret
                );
                continue;
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
                    while let Err(_) = movie_state_enqueue_packet(&movie_state.videoqueue, packet) {
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
            ::std::thread::yield_now();
        }
    };
}

#[cfg(test)]
mod tests {
    use ::std::{thread::sleep, time::Duration};

    use super::*;

    #[test]
    fn test_add_two_movies_to_analyzer() {
        let default_file = String::from("test_vid.mp4");
        let mut analyzer_ctx = AnalyzerContext::new();
        let filepath: std::ffi::CString = std::ffi::CString::new(default_file).unwrap();
        unsafe {
            open_movie(&mut analyzer_ctx, filepath.as_ptr());
            open_movie(&mut analyzer_ctx, filepath.as_ptr());
        }

        assert_eq!(analyzer_ctx.movie_count(), 2);
        analyzer_ctx.close();
        sleep(Duration::from_millis(200));

        assert_eq!(analyzer_ctx.movie_count(), 0)
    }

    #[test]
    fn test_pause_analyzer_pauses_all_movies() {
        let default_file = String::from("test_vid.mp4");
        let mut analyzer_ctx = AnalyzerContext::new();
        let filepath: std::ffi::CString = std::ffi::CString::new(default_file).unwrap();
        unsafe {
            open_movie(&mut analyzer_ctx, filepath.as_ptr());
        }

        assert_eq!(analyzer_ctx.movie_count(), 1);
        analyzer_ctx.step();
        for movie in analyzer_ctx.movie_list.iter() {
            println!("movie 1");
            assert_eq!(movie.step, true)
        }
        analyzer_ctx.close();
        sleep(Duration::from_millis(200));

        assert_eq!(analyzer_ctx.movie_count(), 0)
    }

}
