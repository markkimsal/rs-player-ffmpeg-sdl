#![allow(unused_imports)]
use std::ffi::CStr;
use std::ffi::CString;
use std::ops::Deref;
use std::panic::panic_any;
use std::ptr;
use std::ptr::NonNull;
use std::slice;
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
use rusty_ffmpeg::ffi::sws_getCachedContext;
use rusty_ffmpeg::ffi::sws_getContext;
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

pub fn open_input(src: &str) -> (*const ffi::AVCodec, &mut ffi::AVFormatContext, &mut ffi::AVCodecContext) {

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
    let mut _time_base_num:i32 = 10000;

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
                    _time_base_num = stream.time_base.num;
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
        "format {}, duration {:0>3}:{:0>2}, time_base {}",
        format_name, dur_min, dur_s / 100 , time_base_den
    );
    (codec_ptr, format_context, codec_context)
}

pub fn open_window(format_context: *mut ffi::AVFormatContext, codec_context: &mut ffi::AVCodecContext) {

    let rotation = unsafe { get_orientation_metadata_value(format_context) };
    let mut rotate_filter = rotation_filter_init();
    crate::filter::init_filter(
        rotation,
        &mut rotate_filter.filter_graph,
        &mut rotate_filter.buffersink_ctx,
        &mut rotate_filter.buffersrc_ctx
    );

    let (window_width, window_height): (u32, u32) = match rotation {
        90 => (codec_context.height as u32, codec_context.width as u32),
        _  => (codec_context.width as u32, codec_context.height as u32)
    };
    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();

    let window = video_subsystem.window("rs-player-ffmpeg-sdl2", window_width, window_height)
        .position_centered()
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().build().unwrap();
    let width = 800;
    let height = 600;

    canvas.set_draw_color(Color::RGB(0, 255, 255));
    canvas.clear();
    canvas.present();
    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator.create_texture(
        Some(PixelFormatEnum::ARGB32),
        TextureAccess::Streaming,
        height,
        width
    ).unwrap();
    let frame =
        unsafe { ffi::av_frame_alloc().as_mut() }
        .expect("failed to allocated memory for AVFrame");
    let packet = unsafe { ffi::av_packet_alloc().as_mut() }
        .expect("failed to allocated memory for AVPacket");
    let dest_frame =
        unsafe { ffi::av_frame_alloc().as_mut() }
        .expect("failed to allocated memory for AVFrame");

    let width:i32 = 800;
    let height:i32 = 600;
    let sws_ctx = unsafe { sws_getContext(
        codec_context.width,
        codec_context.height,
        AVPixelFormat_AV_PIX_FMT_YUV420P,
        width,
        height,
        AVPixelFormat_AV_PIX_FMT_ARGB,
        SWS_BILINEAR as i32,
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
    ) };


    let mut event_pump = sdl_context.event_pump().unwrap();
    let mut i = 0;
    'running: loop {
        i = (i + 1) % 255;
        // canvas.set_draw_color(Color::RGB(i, 64, 255 - i));
        // canvas.clear();
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit {..} |
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    break 'running
                },
                _ => {}
            }
        }
        // The rest of the game loop goes here...
        let video_stream_index = Some(0);

        unsafe { ffi::av_read_frame(format_context, packet) };
        {
            if video_stream_index == Some(packet.stream_index as usize) {
                if let Ok(_) = decode_packet(packet, codec_context, frame) {
                    blit_frame(frame, dest_frame, &mut canvas, &mut texture, sws_ctx, &rotate_filter).unwrap_or_default();
                }
            }
            unsafe { ffi::av_packet_unref(packet) };
        }

        canvas.present();
        // ::std::thread::sleep(Duration::new(0, 1_000_000_000u32 / 240000));
    }
    unsafe { av_frame_free(&mut (dest_frame as *mut _)) };
}

fn decode_packet(
    packet: &ffi::AVPacket,
    codec_context: &mut ffi::AVCodecContext,
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
        } else {
            println!(
                "Frame {} (type={}, size={} bytes) pts {} key_frame {} [DTS {}]",
                codec_context.frame_number,
                unsafe { ffi::av_get_picture_type_char(frame.pict_type) },
                frame.pkt_size,
                frame.pts,
                frame.key_frame,
                frame.pkt_dts
            );
            return Ok(());
        }
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
            dest_frame.height = 600;
            dest_frame.width  = 800;
            dest_frame.format = AVPixelFormat_AV_PIX_FMT_ARGB;
            unsafe { ffi::av_frame_get_buffer(dest_frame, 0) };

            unsafe { sws_scale(
                sws_ctx,
                src_frame.data.as_ptr() as _,
                src_frame.linesize.as_ptr(),
                0,
                1080,
                // codec_context.height,
                dest_frame.data.as_mut_ptr(),
                dest_frame.linesize.as_mut_ptr())
            };

    let new_frame = frame_thru_filter(filter, dest_frame);
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

    filt_frame.width  = 800;
    filt_frame.height = 600;
    filt_frame.format = AVPixelFormat_AV_PIX_FMT_ARGB;
    unsafe { ffi::av_frame_get_buffer(filt_frame, 0) };


	let mut result = unsafe { ffi::av_buffersrc_add_frame(filter.buffersrc_ctx, frame) };

    loop {
        unsafe {
            result =  ffi::av_buffersink_get_frame(filter.buffersink_ctx, filt_frame);
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
