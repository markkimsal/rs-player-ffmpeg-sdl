use ::std::sync::{atomic::AtomicBool, Arc, Mutex};

#[allow(unused_imports)]
use ::log::{debug, error};
use ::rusty_ffmpeg::ffi;

use crate::movie_state::{movie_state_enqueue_frame, CodecContextWrapper, MovieState};


pub unsafe fn decode_thread(movie_state: Arc<&mut MovieState>, keep_running: Arc<AtomicBool>) {
    let frame = ffi::av_frame_alloc()
        .as_mut()
        .expect("failed to allocated memory for AVFrame");
    loop {
        let mut locked_videoqueue = movie_state.videoqueue.lock().unwrap();
        if let Some(packet) = locked_videoqueue.front_mut() {
            // !Note that AVPacket.pts is in AVStream.time_base units, not AVCodecContext.time_base units.
            if let Ok(_) = decode_packet(packet.ptr, &movie_state.video_ctx, frame) {
                {
                    // let time_base = movie_state.video_stream.lock().unwrap().ptr.as_ref().unwrap().time_base;
                    // delay *= (time_base.num as f64) / (time_base.den as f64);
                }

                // this returns an error when the queue is full.
                while let Err(_) = movie_state_enqueue_frame(&movie_state.picq, frame) {
                    // ::std::thread::yield_now();
                    ::std::thread::sleep(::std::time::Duration::from_millis(70));
                    if ! keep_running.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                }
            }
            ffi::av_frame_unref(frame as *mut _);
            ffi::av_packet_unref(packet.ptr);
            ffi::av_packet_free(&mut packet.ptr as *mut *mut _);
            locked_videoqueue.pop_front();
        } else {
            ::std::thread::sleep(::std::time::Duration::from_micros(10));
        }
        if ! keep_running.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
    };

    let _ = decode_packet(std::ptr::null_mut(), &movie_state.video_ctx, frame);
    ffi::av_frame_free(&mut (frame as *mut _) as *mut *mut _);
}

fn decode_packet(
    packet: *mut ffi::AVPacket,
    arc_codec_context: &Mutex<CodecContextWrapper>,
    frame: &mut ffi::AVFrame,
) -> Result<(), String> {
    let lock = arc_codec_context.try_lock();
    if let Err(_) = lock {
        return Err(String::from("Error while locking the codec context."));
    }
    let codec_context = unsafe {lock.unwrap().ptr.as_mut().unwrap()};
    let mut response = unsafe { ffi::avcodec_send_packet(codec_context, packet) };

    if response < 0 {
        error!("Error while sending a packet to the decoder. {:?}", ffi::av_err2str(response));
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
        debug!(
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
